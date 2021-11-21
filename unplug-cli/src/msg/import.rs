use super::common::*;
use crate::common::{edit_iso_optional, open_qp_required, read_globals_qp, read_stage_qp, QP_PATH};
use crate::id::IdString;
use crate::opt::ImportMessagesOpt;
use anyhow::{anyhow, bail, ensure, Result};
use log::{debug, info, warn};
use quick_xml::events::attributes::Attribute;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use std::collections::{HashMap, HashSet};
use std::convert::TryInto;
use std::fs::File;
use std::io::{BufRead, BufReader, Cursor, Seek, SeekFrom};
use std::mem;
use std::path::Path;
use std::str;
use tempfile::NamedTempFile;
use unplug::common::{SoundId, Text, WriteTo};
use unplug::data::music::MUSIC;
use unplug::data::sound_event::SOUND_EVENTS;
use unplug::data::stage::{StageDefinition, GLOBALS_PATH};
use unplug::dvd::ArchiveBuilder;
use unplug::event::msg::*;
use unplug::event::Script;
use unplug::globals::GlobalsBuilder;

/// Parses a 32-bit integer which may be represented in either hex or decimal.
fn parse_int(string: &str) -> Result<i32> {
    if let Some(hex) = string.strip_prefix("0x") {
        Ok(u32::from_str_radix(hex, 16)? as i32)
    } else if let Some(hex) = string.strip_prefix("-0x") {
        if hex == "80000000" {
            Ok(-0x80000000)
        } else {
            Ok(-i32::from_str_radix(hex, 16)?)
        }
    } else {
        Ok(string.parse()?)
    }
}

/// Parses a boolean value.
fn parse_bool(string: &str) -> Result<bool> {
    match string {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        _ => bail!("Invalid boolean: {}", string),
    }
}

/// Parses a yes/no value, where `yes` maps to `true` and `no` maps to `false`.
fn parse_yes_no(string: &str) -> Result<bool> {
    match string {
        QUESTION_YES => Ok(true),
        QUESTION_NO => Ok(false),
        _ => bail!("Invalid question value: {}", string),
    }
}

/// Parses a sound or music name into a `SoundId`.
fn parse_sound(name: &str) -> Result<SoundId> {
    for music in MUSIC {
        if unicase::eq(music.name, name) {
            return Ok(music.id.into());
        }
    }
    for sound in SOUND_EVENTS {
        if unicase::eq(sound.name, name) {
            return Ok(sound.id.into());
        }
    }
    bail!("Invalid sound name: \"{}\"", name);
}

/// Reads messages from an XML file.
struct MessageReader<R: BufRead> {
    reader: Reader<R>,
    buf: Vec<u8>,
    command_buf: Vec<u8>,
    attr_buf: String,
    in_text: bool,
}

impl<R: BufRead> MessageReader<R> {
    /// Constructs a new `MessageReader<R>` which reads XML data from `reader`.
    fn new(reader: R) -> Self {
        let mut reader = Reader::from_reader(reader);
        reader.trim_text(false).expand_empty_elements(true).check_comments(true);
        Self { reader, buf: vec![], command_buf: vec![], attr_buf: String::new(), in_text: false }
    }

    /// Reads the header of the XML file up through the `<messages>` tag.
    fn read_header(&mut self) -> Result<()> {
        loop {
            match self.read_event()? {
                Event::Start(e) => {
                    let name = str::from_utf8(e.name())?;
                    ensure!(name == ELEM_MESSAGES, "Unexpected element: {}", name);
                    return Ok(());
                }
                Event::DocType(_) | Event::Decl(_) => (),
                e => Self::unhandled_event(e)?,
            }
        }
    }

    /// Reads from the `</messages>` tag to the end of the file.
    fn read_footer(&mut self) -> Result<()> {
        loop {
            match self.read_event()? {
                Event::Eof => return Ok(()),
                e => Self::unhandled_event(e)?,
            }
        }
    }

    /// Reads a single message from the file. Returns `None` if there are no more messages.
    fn read_message(&mut self) -> Result<Option<(MessageId, MsgArgs)>> {
        let mut id = None;
        loop {
            match self.read_event()?.into_owned() {
                Event::Start(e) => {
                    let name = str::from_utf8(e.name())?;
                    ensure!(name == ELEM_MESSAGE, "Unexpected element: {}", name);
                    for attr in e.attributes() {
                        let (key, value) = self.decode_attribute(attr?)?;
                        match key {
                            ATTR_ID => id = Some(MessageId::parse(value)?),
                            _ => bail!("Unexpected attribute: {}", key),
                        }
                    }
                    break;
                }
                Event::End(e) => {
                    let name = str::from_utf8(e.name())?;
                    ensure!(name == ELEM_MESSAGES, "Unexpected end element: {}", name);
                    return Ok(None);
                }
                e => Self::unhandled_event(e)?,
            }
        }
        let id = id.ok_or_else(|| anyhow!("Missing {} attribute", ATTR_ID))?;
        let mut msg = MsgArgs::new();
        while let Some(command) = self.read_command()? {
            msg.commands.push(command);
        }
        Ok(Some((id, msg)))
    }

    /// Reads a single message command from the file. Returns `None` if there are no more commands.
    fn read_command(&mut self) -> Result<Option<MsgCommand>> {
        // Grab hold of the command buffer for this call
        let mut command_buf = mem::take(&mut self.command_buf);
        let result = self.read_command_buffered(&mut command_buf);
        command_buf.clear();
        self.command_buf = command_buf;
        result
    }

    /// Reads a message command from the file using `buf` as the internal buffer.
    fn read_command_buffered(&mut self, buf: &mut Vec<u8>) -> Result<Option<MsgCommand>> {
        loop {
            match self.reader.read_event(buf)? {
                Event::Start(e) => {
                    let name = str::from_utf8(e.name())?;

                    if self.in_text {
                        let command = match name {
                            ELEM_FORMAT => self.read_format(e)?,
                            ELEM_ICON => self.read_icon(e)?,
                            ELEM_NEWLINE => self.read_simple(e, MsgCommand::Newline)?,
                            ELEM_NEWLINE_VT => self.read_simple(e, MsgCommand::NewlineVt)?,
                            _ => bail!("Unexpected element: {}", name),
                        };
                        if !matches!(command, MsgCommand::Format(_)) {
                            self.read_to_end()?;
                        }
                        return Ok(Some(command));
                    }

                    if name == ELEM_TEXT {
                        self.in_text = true;
                        continue;
                    }

                    let command = match name {
                        ELEM_ANIM => self.read_anim(e)?,
                        ELEM_DEFAULT => self.read_default(e)?,
                        ELEM_FONT => self.read_font(e)?,
                        ELEM_NUM_INPUT => self.read_num_input(e)?,
                        ELEM_QUESTION => self.read_question(e)?,
                        ELEM_SFX => self.read_sfx(e)?,
                        ELEM_SHAKE => self.read_shake(e)?,
                        ELEM_STAY => self.read_simple(e, MsgCommand::Stay)?,
                        ELEM_VOICE => self.read_voice(e)?,
                        ELEM_WAIT => self.read_wait(e)?,
                        _ => bail!("Unexpected element: {}", name),
                    };
                    self.read_to_end()?;
                    return Ok(Some(command));
                }

                Event::Text(text) if self.in_text => {
                    // Any raw text maps to a `Text` command
                    let unescaped_bytes = text.unescaped()?;
                    let unescaped_str = str::from_utf8(&unescaped_bytes)?;
                    if unescaped_str.is_empty() {
                        continue;
                    }
                    let msg_text = Text::encode(unescaped_str)?;
                    return Ok(Some(MsgCommand::Text(msg_text)));
                }

                Event::End(e) => {
                    let name = str::from_utf8(e.name())?;
                    if self.in_text && name == ELEM_TEXT {
                        self.in_text = false;
                    } else if !self.in_text && name == ELEM_MESSAGE {
                        return Ok(None);
                    } else {
                        bail!("Unexpected end element: {}", name);
                    }
                }

                e => Self::unhandled_event(e)?,
            }
        }
    }

    fn read_anim(&mut self, elem: BytesStart<'_>) -> Result<MsgCommand> {
        let (mut flags, mut obj, mut anim) = (None, None, None);
        for attr in elem.attributes() {
            let (key, value) = self.decode_attribute(attr?)?;
            match key {
                ATTR_FLAGS => flags = Some(parse_int(value)? as u8),
                ATTR_OBJ => obj = Some(parse_int(value)? as i16),
                ATTR_ID => anim = Some(parse_int(value)?),
                _ => bail!("Unexpected attribute: {}", key),
            }
        }
        Ok(MsgCommand::Anim(MsgAnimArgs {
            flags: flags.ok_or_else(|| anyhow!("Missing {} attribute", ATTR_FLAGS))?,
            obj: obj.ok_or_else(|| anyhow!("Missing {} attribute", ATTR_OBJ))?,
            anim: anim.ok_or_else(|| anyhow!("Missing {} attribute", ATTR_ID))?,
        }))
    }

    fn read_default(&mut self, elem: BytesStart<'_>) -> Result<MsgCommand> {
        let mut flags = DefaultFlags::empty();
        let mut index = None;
        for attr in elem.attributes() {
            let (key, value) = self.decode_attribute(attr?)?;
            match key {
                ATTR_VAR => {
                    flags = DefaultFlags::VARIABLE;
                    index = Some(parse_int(value)?);
                }
                ATTR_ID => index = Some(parse_int(value)?),
                _ => bail!("Unexpected attribute: {}", key),
            }
        }
        Ok(MsgCommand::Default(DefaultArgs {
            flags,
            index: index.ok_or_else(|| anyhow!("Missing {} or {} attribute", ATTR_ID, ATTR_VAR))?,
        }))
    }

    fn read_font(&mut self, elem: BytesStart<'_>) -> Result<MsgCommand> {
        // TODO: Support multiple commands
        let mut cmd = None;
        for attr in elem.attributes() {
            let (key, value) = self.decode_attribute(attr?)?;
            match key {
                ATTR_SPEED => cmd = Some(MsgCommand::Speed(parse_int(value)? as u8)),
                ATTR_SIZE => cmd = Some(MsgCommand::Size(parse_int(value)? as u8)),
                ATTR_COLOR => {
                    cmd = if let Some(hex) = value.strip_prefix('#') {
                        Some(MsgCommand::Rgba(u32::from_str_radix(hex, 16)?))
                    } else {
                        Some(MsgCommand::Color(Color::try_from_id(value)?))
                    };
                }
                ATTR_MONO => cmd = Some(MsgCommand::Proportional(!parse_bool(value)?)),
                ATTR_ALIGN => {
                    cmd = Some(MsgCommand::Center(match value {
                        ALIGN_LEFT => false,
                        ALIGN_CENTER => true,
                        _ => bail!("Invalid alignment: {}", value),
                    }));
                }
                ATTR_ROTATION => cmd = Some(MsgCommand::Rotate(parse_int(value)? as i16)),
                _ => bail!("Unexpected attribute: {}", key),
            }
        }
        cmd.ok_or_else(|| anyhow!("<{}> requires an attribute", ELEM_FONT))
    }

    fn read_format(&mut self, elem: BytesStart<'_>) -> Result<MsgCommand> {
        if let Some(attr) = elem.attributes().next() {
            bail!("Unexpected attribute: {}", str::from_utf8(attr?.key)?);
        }
        let mut bytes = vec![];
        loop {
            match self.read_event()? {
                Event::Text(text) => {
                    bytes.extend_from_slice(&text.unescaped()?);
                }
                Event::End(_) => break,
                e => Self::unhandled_event(e)?,
            }
        }
        Ok(MsgCommand::Format(Text::encode(str::from_utf8(&bytes)?)?))
    }

    fn read_icon(&mut self, elem: BytesStart<'_>) -> Result<MsgCommand> {
        let mut icon = None;
        for attr in elem.attributes() {
            let (key, value) = self.decode_attribute(attr?)?;
            match key {
                ATTR_ID => icon = Some(Icon::try_from_id(value)?),
                _ => bail!("Unexpected attribute: {}", key),
            }
        }
        Ok(MsgCommand::Icon(icon.ok_or_else(|| anyhow!("Missing {} attribute", ATTR_ID))?))
    }

    fn read_num_input(&mut self, elem: BytesStart<'_>) -> Result<MsgCommand> {
        let (mut digits, mut editable, mut selected) = (None, None, None);
        for attr in elem.attributes() {
            let (key, value) = self.decode_attribute(attr?)?;
            match key {
                ATTR_DIGITS => digits = Some(parse_int(value)? as u8),
                ATTR_EDITABLE => editable = Some(parse_int(value)? as u8),
                ATTR_SELECTED => selected = Some(parse_int(value)? as u8),
                _ => bail!("Unexpected attribute: {}", key),
            }
        }
        Ok(MsgCommand::NumInput(NumInputArgs {
            digits: digits.ok_or_else(|| anyhow!("Missing {} attribute", ATTR_DIGITS))?,
            editable: editable.ok_or_else(|| anyhow!("Missing {} attribute", ATTR_EDITABLE))?,
            selected: selected.ok_or_else(|| anyhow!("Missing {} attribute", ATTR_SELECTED))?,
        }))
    }

    fn read_question(&mut self, elem: BytesStart<'_>) -> Result<MsgCommand> {
        let (mut left, mut right, mut default) = (None, None, None);
        for attr in elem.attributes() {
            let (key, value) = self.decode_attribute(attr?)?;
            match key {
                ATTR_LEFT => left = Some(parse_yes_no(value)?),
                ATTR_RIGHT => right = Some(parse_yes_no(value)?),
                ATTR_DEFAULT => default = Some(parse_int(value)? as u8),
                _ => bail!("Unexpected attribute: {}", key),
            }
        }
        let mut flags = QuestionFlags::empty();
        let left = left.ok_or_else(|| anyhow!("Missing {} attribute", ATTR_LEFT))?;
        let right = right.ok_or_else(|| anyhow!("Missing {} attribute", ATTR_RIGHT))?;
        flags.set(QuestionFlags::LEFT_NO, !left);
        flags.set(QuestionFlags::RIGHT_NO, !right);
        Ok(MsgCommand::Question(QuestionArgs {
            flags,
            default: default.ok_or_else(|| anyhow!("Missing {} attribute", ATTR_DEFAULT))?,
        }))
    }

    fn read_sfx(&mut self, elem: BytesStart<'_>) -> Result<MsgCommand> {
        let (mut id, mut name, mut cmd, mut duration, mut volume) = (None, None, None, None, None);
        for attr in elem.attributes() {
            let (key, value) = self.decode_attribute(attr?)?;
            match key {
                ATTR_ID => id = Some(parse_int(value)? as u32), // Deprecated
                ATTR_NAME => name = Some(value.to_owned()),
                ATTR_CMD => cmd = Some(value.to_owned()),
                ATTR_DURATION => duration = Some(parse_int(value)? as u16),
                ATTR_VOLUME => volume = Some(parse_int(value)? as u8),
                _ => bail!("Unexpected attribute: {}", key),
            }
        }

        let sound = if let Some(name) = &name {
            parse_sound(name)?
        } else if let Some(id) = id {
            id.try_into().map_err(|id| anyhow!("Invalid sound ID: {:#x}", id))?
        } else {
            bail!("Missing {} attribute", ATTR_NAME);
        };

        let cmd = cmd.ok_or_else(|| anyhow!("Missing {} attribute", ATTR_CMD))?;
        // duration and volume are optional so the errors here are only checked if necessary
        let duration = duration.ok_or_else(|| anyhow!("Missing {} attribute", ATTR_DURATION));
        let volume = volume.ok_or_else(|| anyhow!("Missing {} attribute", ATTR_VOLUME));
        Ok(MsgCommand::Sfx(
            sound,
            match &*cmd {
                SFX_FADE_IN => MsgSfxType::FadeIn(duration?),
                SFX_FADE_OUT => MsgSfxType::FadeOut(duration?),
                SFX_FADE => {
                    MsgSfxType::Fade(MsgSfxFadeArgs { duration: duration?, volume: volume? })
                }
                SFX_PLAY => MsgSfxType::Play,
                SFX_STOP => MsgSfxType::Stop,
                SFX_UNK_5 => MsgSfxType::Unk5,
                SFX_UNK_6 => MsgSfxType::Unk6,
                SFX_WAIT => MsgSfxType::Wait,
                _ => bail!("Invalid sfx command: {}", cmd),
            },
        ))
    }

    fn read_shake(&mut self, elem: BytesStart<'_>) -> Result<MsgCommand> {
        let (mut ty, mut strength, mut speed) = (None, None, None);
        let mut flags = ShakeFlags::empty();
        for attr in elem.attributes() {
            let (key, value) = self.decode_attribute(attr?)?;
            match key {
                ATTR_TYPE => ty = Some(value.to_owned()),
                ATTR_STRENGTH => strength = Some(parse_int(value)? as u8),
                ATTR_SPEED => speed = Some(parse_int(value)? as u8),
                ATTR_X => flags.set(ShakeFlags::X, parse_bool(value)?),
                ATTR_Y => flags.set(ShakeFlags::Y, parse_bool(value)?),
                ATTR_SIZE => flags.set(ShakeFlags::SIZE, parse_bool(value)?),
                ATTR_ROTATION => flags.set(ShakeFlags::ROTATION, parse_bool(value)?),
                _ => bail!("Unexpected attribute: {}", key),
            }
        }
        let ty = ty.ok_or_else(|| anyhow!("Missing {} attribute", ATTR_TYPE))?;
        match &*ty {
            SHAKE_NONE => {
                // These are optional for "none"
                strength = strength.or(Some(0));
                speed = speed.or(Some(0));
            }
            SHAKE_JITTER => flags.insert(ShakeFlags::JITTER),
            SHAKE_WAVE => flags.insert(ShakeFlags::WAVE),
            _ => bail!("Invalid shake type: {}", ty),
        }
        Ok(MsgCommand::Shake(ShakeArgs {
            flags,
            strength: strength.ok_or_else(|| anyhow!("Missing {} attribute", ATTR_STRENGTH))?,
            speed: speed.ok_or_else(|| anyhow!("Missing {} attribute", ATTR_SPEED))?,
        }))
    }

    fn read_voice(&mut self, elem: BytesStart<'_>) -> Result<MsgCommand> {
        let mut voice = None;
        for attr in elem.attributes() {
            let (key, value) = self.decode_attribute(attr?)?;
            match key {
                ATTR_ID => voice = Some(Voice::try_from_id(value)?),
                _ => bail!("Unexpected attribute: {}", key),
            }
        }
        Ok(MsgCommand::Voice(voice.ok_or_else(|| anyhow!("Missing {} attribute", ATTR_ID))?))
    }

    fn read_wait(&mut self, elem: BytesStart<'_>) -> Result<MsgCommand> {
        let (mut ty, mut duration) = (None, None);
        for attr in elem.attributes() {
            let (key, value) = self.decode_attribute(attr?)?;
            match key {
                ATTR_TYPE => ty = Some(value.to_owned()),
                ATTR_DURATION => duration = Some(parse_int(value)? as u8),
                _ => bail!("Unexpected attribute: {}", key),
            }
        }
        let ty = ty.ok_or_else(|| anyhow!("Missing {} attribute", ATTR_TYPE))?;
        Ok(MsgCommand::Wait(match &*ty {
            WAIT_ATC_MENU => MsgWaitType::AtcMenu,
            WAIT_LEFT_PLUG => MsgWaitType::LeftPlug,
            WAIT_RIGHT_PLUG => MsgWaitType::RightPlug,
            WAIT_SUIT_MENU => MsgWaitType::SuitMenu,
            WAIT_TIME => MsgWaitType::Time(
                duration.ok_or_else(|| anyhow!("Missing {} attribute", ATTR_DURATION))?,
            ),
            _ => bail!("Invalid wait type: {}", ty),
        }))
    }

    /// Reads a message command which takes no attributes or inner text and returns `command`.
    fn read_simple(&mut self, elem: BytesStart<'_>, command: MsgCommand) -> Result<MsgCommand> {
        if let Some(attr) = elem.attributes().next() {
            bail!("Unexpected attribute: {}", str::from_utf8(attr?.key)?);
        }
        Ok(command)
    }

    /// Reads an XML event, clearing the internal buffer first.
    fn read_event(&mut self) -> Result<Event<'_>> {
        self.buf.clear();
        Ok(self.reader.read_event(&mut self.buf)?)
    }

    /// Decodes an XML attribute into key and value strings.
    fn decode_attribute(&mut self, attr: Attribute<'_>) -> Result<(&str, &str)> {
        let key = str::from_utf8(attr.key)?;
        let key_len = key.len();
        let value_bytes = attr.unescaped_value()?;
        let value = str::from_utf8(&value_bytes)?;
        // Storing the key and value in our internal attribute buffer reduces the number of
        // allocations we have to make
        self.attr_buf.clear();
        self.attr_buf.push_str(key);
        self.attr_buf.push_str(value);
        Ok((&self.attr_buf[..key_len], &self.attr_buf[key_len..]))
    }

    /// Reads up until an end tag and throws an error on any unexpected data.
    fn read_to_end(&mut self) -> Result<()> {
        loop {
            match self.read_event()? {
                Event::End(_) => return Ok(()),
                e => Self::unhandled_event(e)?,
            }
        }
    }

    /// If an XML event was not handled, reports an error for it if necessary.
    fn unhandled_event(e: Event<'_>) -> Result<()> {
        match e {
            Event::Start(e) | Event::Empty(e) => {
                let name = String::from_utf8_lossy(e.name());
                bail!("Unexpected element: {}", name);
            }
            Event::End(e) => {
                let name = String::from_utf8_lossy(e.name());
                bail!("Unexpected end element: {}", name);
            }
            Event::Text(t) | Event::CData(t) => {
                let text = String::from_utf8_lossy(t.escaped());
                let trimmed = text.trim();
                ensure!(trimmed.is_empty(), "Unexpected text: \"{}\"", trimmed);
                Ok(())
            }
            Event::Decl(_) => {
                bail!("Unexpected XML declaration");
            }
            Event::PI(_) => {
                bail!("Processing instructions are not supported");
            }
            Event::DocType(_) => {
                bail!("Unexpected XML doctype");
            }
            Event::Eof => {
                bail!("Unexpected end of file");
            }
            // Ignore comments
            Event::Comment(_) => Ok(()),
        }
    }
}

/// Replaces messages in `script` using `messages`. After a message is replaced, it is removed from
/// the map.
fn apply_messages(
    source: MessageSource,
    script: &mut Script,
    messages: &mut HashMap<MessageId, MsgArgs>,
) {
    for (id, old_message) in iter_messages_mut(source, script) {
        if let Some(new_message) = messages.remove(&id) {
            *old_message = new_message;
        }
    }
}

pub fn import_messages(opt: ImportMessagesOpt) -> Result<()> {
    info!("Reading messages from {}", opt.input.to_str().unwrap());
    let file = BufReader::new(File::open(opt.input)?);
    let mut reader = MessageReader::new(file);
    reader.read_header()?;
    let mut messages = HashMap::new();
    let mut sources = HashSet::new();
    while let Some((id, mut msg)) = reader.read_message()? {
        sources.insert(id.source);
        msg.extra_data = encode_block_offset(id.block_offset).to_vec();
        messages.insert(id, msg);
    }
    reader.read_footer()?;
    if messages.is_empty() {
        info!("No messages read - stopping");
        return Ok(());
    }
    let mut sources: Vec<_> = sources.into_iter().collect();
    sources.sort_unstable();

    let mut iso = edit_iso_optional(opt.container.iso.as_ref())?;
    let mut qp = open_qp_required(iso.as_mut(), &opt.container)?;

    info!("Reading script globals");
    let mut globals = read_globals_qp(&mut qp)?;
    let mut rebuilt_files = vec![];
    let mut libs = globals.read_libs()?;
    if sources[0] == MessageSource::Globals {
        info!("Rebuilding globals.bin");
        apply_messages(MessageSource::Globals, &mut libs.script, &mut messages);
        let mut writer = Cursor::new(vec![]);
        GlobalsBuilder::new().base(&mut globals).libs(&libs).write_to(&mut writer)?;
        let bytes = writer.into_inner().into_boxed_slice();
        rebuilt_files.push((GLOBALS_PATH.to_owned(), bytes));
    }
    drop(globals);

    for source in sources {
        let stage_id = match source {
            MessageSource::Globals => continue,
            MessageSource::Stage(id) => id,
        };
        let stage_def = StageDefinition::get(stage_id);
        info!("Rebuilding {}.bin", stage_def.name);
        let mut stage = read_stage_qp(&mut qp, stage_def.name, &libs)?;
        apply_messages(source, &mut stage.script, &mut messages);
        let mut writer = Cursor::new(vec![]);
        stage.write_to(&mut writer)?;
        let bytes = writer.into_inner().into_boxed_slice();
        rebuilt_files.push((stage_def.path(), bytes));
    }

    if !messages.is_empty() {
        let mut unused_ids: Vec<_> = messages.into_iter().map(|(k, _)| k).collect();
        unused_ids.sort_unstable();
        for id in unused_ids {
            warn!("Message was not found: {}", id.to_string());
        }
    }

    info!("Rebuilding qp.bin");
    let mut qp_temp = match &opt.container.qp {
        Some(path) => NamedTempFile::new_in(path.parent().unwrap_or_else(|| Path::new(".")))?,
        None => NamedTempFile::new()?,
    };
    let mut qp_builder = ArchiveBuilder::with_archive(&mut qp);
    for (path, bytes) in rebuilt_files {
        qp_builder.replace_at(&path, || Cursor::new(bytes))?;
    }
    debug!("Writing new qp.bin to {}", qp_temp.path().to_string_lossy());
    qp_builder.write_to(&mut qp_temp)?;
    drop(qp_builder);
    drop(qp);

    if let Some(mut iso) = iso {
        info!("Updating ISO");
        qp_temp.seek(SeekFrom::Start(0))?;
        iso.replace_file_at(QP_PATH, qp_temp)?;
    } else {
        qp_temp.persist(opt.container.qp.unwrap())?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::Cursor;
    use unplug::data::{Music, SoundEvent};

    fn cmd(xml: &[u8]) -> Result<MsgCommand> {
        let mut reader = MessageReader::new(Cursor::new(xml));
        reader.read_command().transpose().unwrap()
    }

    #[test]
    fn test_parse_int() -> Result<()> {
        assert_eq!(parse_int("0")?, 0);
        assert_eq!(parse_int("12345678")?, 12345678);
        assert_eq!(parse_int("-12345678")?, -12345678);
        assert_eq!(parse_int("0xabcdabc")?, 0xabcdabc);
        assert_eq!(parse_int("0xABCDABC")?, 0xabcdabc);
        assert_eq!(parse_int("-0xabcdabc")?, -0xabcdabc);
        assert_eq!(parse_int("0x7fffffff")?, 0x7fffffff);
        assert_eq!(parse_int("-0x7fffffff")?, -0x7fffffff);
        assert_eq!(parse_int("-0x80000000")?, -0x80000000);
        assert!(parse_int("").is_err());
        assert!(parse_int("abcd").is_err());
        assert!(parse_int("0abcd").is_err());
        assert!(parse_int("0x0abcdefg").is_err());
        assert!(parse_int("0x100000000").is_err());
        Ok(())
    }

    #[test]
    fn test_parse_bool() -> Result<()> {
        assert!(!parse_bool("false")?);
        assert!(parse_bool("true")?);
        assert!(!parse_bool("0")?);
        assert!(parse_bool("1")?);
        assert!(parse_bool("").is_err());
        assert!(parse_bool("2").is_err());
        Ok(())
    }

    #[test]
    fn test_parse_sound() -> Result<()> {
        assert_eq!(parse_sound("elec")?, SoundId::Sound(SoundEvent::Elec));
        assert_eq!(parse_sound("ElEc")?, SoundId::Sound(SoundEvent::Elec));
        assert_eq!(parse_sound("bgm_night")?, SoundId::Music(Music::BgmNight));
        assert_eq!(parse_sound("BgM_NiGhT")?, SoundId::Music(Music::BgmNight));
        assert!(parse_sound("foo").is_err());
        Ok(())
    }

    #[test]
    fn test_import_speed() -> Result<()> {
        assert_eq!(cmd(b"<font speed=\"2\"/>")?, MsgCommand::Speed(2));
        Ok(())
    }

    #[test]
    fn test_import_wait() -> Result<()> {
        assert_eq!(
            cmd(b"<wait type=\"time\" duration=\"100\"/>")?,
            MsgCommand::Wait(MsgWaitType::Time(100))
        );
        assert_eq!(cmd(b"<wait type=\"left-plug\"/>")?, MsgCommand::Wait(MsgWaitType::LeftPlug));
        assert_eq!(cmd(b"<wait type=\"right-plug\"/>")?, MsgCommand::Wait(MsgWaitType::RightPlug));
        assert_eq!(cmd(b"<wait type=\"suit-menu\"/>")?, MsgCommand::Wait(MsgWaitType::SuitMenu));
        assert_eq!(cmd(b"<wait type=\"atc-menu\"/>")?, MsgCommand::Wait(MsgWaitType::AtcMenu));
        Ok(())
    }

    #[test]
    fn test_import_anim() -> Result<()> {
        assert_eq!(
            cmd(b"<animate flags=\"1\" obj=\"2\" id=\"3\"/>")?,
            MsgCommand::Anim(MsgAnimArgs { flags: 1, obj: 2, anim: 3 })
        );
        Ok(())
    }

    #[test]
    fn test_import_sfx() -> Result<()> {
        let sfx = Music::Bgm.into();
        assert_eq!(
            cmd(b"<sound name=\"bgm\" cmd=\"wait\"/>")?,
            MsgCommand::Sfx(sfx, MsgSfxType::Wait)
        );
        assert_eq!(
            cmd(b"<sound name=\"bgm\" cmd=\"stop\"/>")?,
            MsgCommand::Sfx(sfx, MsgSfxType::Stop)
        );
        assert_eq!(
            cmd(b"<sound name=\"bgm\" cmd=\"play\"/>")?,
            MsgCommand::Sfx(sfx, MsgSfxType::Play)
        );
        assert_eq!(
            cmd(b"<sound name=\"bgm\" cmd=\"fade-out\" duration=\"2\"/>")?,
            MsgCommand::Sfx(sfx, MsgSfxType::FadeOut(2))
        );
        assert_eq!(
            cmd(b"<sound name=\"bgm\" cmd=\"fade-in\" duration=\"2\"/>")?,
            MsgCommand::Sfx(sfx, MsgSfxType::FadeIn(2))
        );
        assert_eq!(
            cmd(b"<sound name=\"bgm\" cmd=\"fade\" duration=\"2\" volume=\"3\"/>")?,
            MsgCommand::Sfx(sfx, MsgSfxType::Fade(MsgSfxFadeArgs { duration: 2, volume: 3 }))
        );
        assert_eq!(
            cmd(b"<sound name=\"bgm\" cmd=\"unk5\"/>")?,
            MsgCommand::Sfx(sfx, MsgSfxType::Unk5)
        );
        assert_eq!(
            cmd(b"<sound name=\"bgm\" cmd=\"unk6\"/>")?,
            MsgCommand::Sfx(sfx, MsgSfxType::Unk6)
        );
        Ok(())
    }

    #[test]
    fn test_import_voice() -> Result<()> {
        assert_eq!(cmd(b"<voice id=\"peekoe\"/>")?, MsgCommand::Voice(Voice::Peekoe));
        Ok(())
    }

    #[test]
    fn test_import_default() -> Result<()> {
        assert_eq!(
            cmd(b"<default id=\"1\"/>")?,
            MsgCommand::Default(DefaultArgs { flags: DefaultFlags::empty(), index: 1 })
        );
        assert_eq!(
            cmd(b"<default var=\"1\"/>")?,
            MsgCommand::Default(DefaultArgs { flags: DefaultFlags::VARIABLE, index: 1 })
        );
        Ok(())
    }

    #[test]
    fn test_import_newline() -> Result<()> {
        assert_eq!(cmd(b"<text><br/></text>")?, MsgCommand::Newline);
        assert_eq!(cmd(b"<text><br-vt/></text>")?, MsgCommand::NewlineVt);
        Ok(())
    }

    #[test]
    fn test_import_format() -> Result<()> {
        assert_eq!(
            cmd(b"<text><f>%s</f></text>")?,
            MsgCommand::Format(Text::with_bytes(b"%s".to_vec()))
        );
        Ok(())
    }

    #[test]
    fn test_import_size() -> Result<()> {
        assert_eq!(cmd(b"<font size=\"24\"/>")?, MsgCommand::Size(24));
        Ok(())
    }

    #[test]
    fn test_import_color() -> Result<()> {
        assert_eq!(cmd(b"<font color=\"lime\"/>")?, MsgCommand::Color(Color::Lime));
        assert_eq!(cmd(b"<font color=\"#12345678\"/>")?, MsgCommand::Rgba(0x12345678));
        Ok(())
    }

    #[test]
    fn test_import_mono() -> Result<()> {
        assert_eq!(cmd(b"<font mono=\"true\"/>")?, MsgCommand::Proportional(false));
        assert_eq!(cmd(b"<font mono=\"false\"/>")?, MsgCommand::Proportional(true));
        Ok(())
    }

    #[test]
    fn test_import_icon() -> Result<()> {
        assert_eq!(cmd(b"<text><icon id=\"moolah\"/></text>")?, MsgCommand::Icon(Icon::Moolah));
        Ok(())
    }

    #[test]
    fn test_import_shake() -> Result<()> {
        assert_eq!(
            cmd(b"<shake type=\"jitter\" strength=\"1\" speed=\"2\" x=\"1\" y=\"1\" size=\"1\" \
                         rotation=\"1\"/>")?,
            MsgCommand::Shake(ShakeArgs {
                strength: 1,
                speed: 2,
                flags: ShakeFlags::JITTER
                    | ShakeFlags::X
                    | ShakeFlags::Y
                    | ShakeFlags::SIZE
                    | ShakeFlags::ROTATION
            })
        );
        assert_eq!(
            cmd(b"<shake type=\"wave\" strength=\"1\" speed=\"2\" x=\"1\" y=\"1\"/>")?,
            MsgCommand::Shake(ShakeArgs {
                strength: 1,
                speed: 2,
                flags: ShakeFlags::WAVE | ShakeFlags::X | ShakeFlags::Y
            })
        );
        assert_eq!(
            cmd(b"<shake type=\"none\"/>")?,
            MsgCommand::Shake(ShakeArgs { strength: 0, speed: 0, flags: ShakeFlags::empty() })
        );
        Ok(())
    }

    #[test]
    fn test_import_align() -> Result<()> {
        assert_eq!(cmd(b"<font align=\"left\"/>")?, MsgCommand::Center(false));
        assert_eq!(cmd(b"<font align=\"center\"/>")?, MsgCommand::Center(true));
        Ok(())
    }

    #[test]
    fn test_import_rotation() -> Result<()> {
        assert_eq!(cmd(b"<font rotation=\"180\"/>")?, MsgCommand::Rotate(180));
        Ok(())
    }

    #[test]
    fn test_import_num_input() -> Result<()> {
        assert_eq!(
            cmd(b"<num-input digits=\"1\" editable=\"2\" selected=\"3\"/>")?,
            MsgCommand::NumInput(NumInputArgs { digits: 1, editable: 2, selected: 3 })
        );
        Ok(())
    }

    #[test]
    fn test_import_question() -> Result<()> {
        assert_eq!(
            cmd(b"<question left=\"yes\" right=\"no\" default=\"1\"/>")?,
            MsgCommand::Question(QuestionArgs { flags: QuestionFlags::RIGHT_NO, default: 1 })
        );
        assert_eq!(
            cmd(b"<question left=\"no\" right=\"yes\" default=\"1\"/>")?,
            MsgCommand::Question(QuestionArgs { flags: QuestionFlags::LEFT_NO, default: 1 })
        );
        Ok(())
    }

    #[test]
    fn test_import_stay() -> Result<()> {
        assert_eq!(cmd(b"<stay/>")?, MsgCommand::Stay);
        Ok(())
    }

    #[test]
    fn test_import_text() -> Result<()> {
        assert_eq!(
            cmd(b"<text> Hello, world! </text>")?,
            MsgCommand::Text(Text::encode(" Hello, world! ")?)
        );
        Ok(())
    }
}
