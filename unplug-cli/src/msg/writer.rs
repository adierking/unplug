use super::constants::*;
use super::{iter_messages, MessageId, MessageSource};
use crate::id::IdString;
use anyhow::Result;
use log::trace;
use quick_xml::events::{BytesDecl, BytesStart, BytesText, Event};
use quick_xml::Writer;
use std::borrow::Cow;
use std::io::Write;
use unplug::common::Text;
use unplug::event::msg::{
    DefaultFlags, MsgArgs, MsgCommand, MsgSfxType, MsgWaitType, QuestionFlags, ShakeFlags,
};
use unplug::event::Script;

/// Converts `text` to an escaped XML string.
/// quick-xml's built-in string escaping converts quote characters, which isn't user-friendly.
fn text_to_xml(text: &Text) -> Result<Cow<'_, str>> {
    let text_str = text.decode()?;
    trace!("text_to_xml({:?})", text_str);
    let mut escaped = String::new();
    let mut i = 0;
    while let Some(len) = text_str[i..].find(|c| c == '<' || c == '>' || c == '&') {
        let next = i + len;
        escaped.push_str(&text_str[i..next]);
        match text_str.as_bytes()[next] {
            b'<' => escaped.push_str("&lt;"),
            b'>' => escaped.push_str("&gt;"),
            b'&' => escaped.push_str("&amp;"),
            _ => (),
        }
        i = next + 1;
    }
    if i == 0 {
        // The string was not changed
        assert!(escaped.is_empty());
        return Ok(text_str);
    }
    escaped.push_str(&text_str[i..]);
    Ok(escaped.into())
}

pub struct MessageWriter<W: Write> {
    writer: Writer<W>,
    root: BytesStart<'static>,
    first_message: bool,
    new_line: bool,
    in_text: bool,
}

impl<W: Write> MessageWriter<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer: Writer::new(writer),
            root: BytesStart::new(ELEM_MESSAGES),
            first_message: true,
            new_line: false,
            in_text: false,
        }
    }

    pub fn start(&mut self) -> Result<()> {
        self.writer.write_event(Event::Decl(BytesDecl::new(
            XML_VERSION,
            Some(XML_ENCODING),
            None,
        )))?;
        self.writer.inner().write_all(b"\n")?;
        self.writer.write_event(Event::Start(self.root.borrow()))?;
        self.writer.inner().write_all(b"\n")?;
        Ok(())
    }

    pub fn write_script(&mut self, source: MessageSource, script: &Script) -> Result<()> {
        for (id, msg) in iter_messages(source, script) {
            self.write_message(id, msg)?;
        }
        Ok(())
    }

    fn write_message(&mut self, id: MessageId, msg: &MsgArgs) -> Result<()> {
        if self.first_message {
            self.first_message = false;
        } else {
            self.writer.inner().write_all(b"\n")?;
        }
        self.writer.inner().write_all(b"\t")?;
        let mut tag = BytesStart::new(ELEM_MESSAGE);
        tag.push_attribute((ATTR_ID, id.to_string().as_ref()));
        self.writer.write_event(Event::Start(tag.borrow()))?;
        self.writer.inner().write_all(b"\n")?;
        self.new_line = true;
        self.write_commands(&msg.commands)?;
        self.next_line()?;
        self.writer.inner().write_all(b"\t")?;
        self.writer.write_event(Event::End(tag.to_end()))?;
        self.writer.inner().write_all(b"\n")?;
        Ok(())
    }

    pub fn finish(&mut self) -> Result<()> {
        self.writer.write_event(Event::End(self.root.to_end()))?;
        self.writer.inner().write_all(b"\n")?;
        self.writer.inner().flush()?;
        Ok(())
    }

    fn write_commands(&mut self, commands: &[MsgCommand]) -> Result<()> {
        for command in commands {
            self.write_command(command)?;
        }
        self.end_text()?;
        Ok(())
    }

    fn write_command(&mut self, command: &MsgCommand) -> Result<()> {
        let is_text = matches!(
            command,
            MsgCommand::Text(_)
                | MsgCommand::Format(_)
                | MsgCommand::Icon(_)
                | MsgCommand::Newline
                | MsgCommand::NewlineVt
        );
        if !is_text {
            self.end_text()?;
            // Commands which are not part of text always have a newline before them
            self.next_line()?;
        }

        // Indent if this is the first command on the line
        if self.new_line {
            self.writer.inner().write_all(b"\t\t")?;
        }
        if is_text {
            // Open a <text> tag
            self.begin_text()?;
        }

        match command {
            MsgCommand::Speed(speed) => {
                let mut tag = BytesStart::new(ELEM_FONT);
                tag.push_attribute((ATTR_SPEED, speed.to_string().as_ref()));
                self.writer.write_event(Event::Empty(tag))?;
            }

            MsgCommand::Wait(ty) => {
                let mut tag = BytesStart::new(ELEM_WAIT);
                match ty {
                    MsgWaitType::Time(time) => {
                        tag.push_attribute((ATTR_TYPE, WAIT_TIME));
                        tag.push_attribute((ATTR_DURATION, time.to_string().as_ref()));
                    }
                    MsgWaitType::AtcMenu => tag.push_attribute((ATTR_TYPE, WAIT_ATC_MENU)),
                    MsgWaitType::SuitMenu => tag.push_attribute((ATTR_TYPE, WAIT_SUIT_MENU)),
                    MsgWaitType::LeftPlug => tag.push_attribute((ATTR_TYPE, WAIT_LEFT_PLUG)),
                    MsgWaitType::RightPlug => tag.push_attribute((ATTR_TYPE, WAIT_RIGHT_PLUG)),
                };
                self.writer.write_event(Event::Empty(tag))?;
            }

            MsgCommand::Anim(anim) => {
                let mut tag = BytesStart::new(ELEM_ANIM);
                tag.push_attribute((ATTR_FLAGS, anim.flags.to_string().as_ref()));
                tag.push_attribute((ATTR_OBJ, anim.obj.to_string().as_ref()));
                tag.push_attribute((ATTR_ID, anim.anim.to_string().as_ref()));
                self.writer.write_event(Event::Empty(tag))?;
            }

            MsgCommand::Sfx(id, ty) => {
                let mut tag = BytesStart::new(ELEM_SFX);
                tag.push_attribute((ATTR_NAME, id.name()));
                match ty {
                    MsgSfxType::Wait => tag.push_attribute((ATTR_CMD, SFX_WAIT)),
                    MsgSfxType::Stop => tag.push_attribute((ATTR_CMD, SFX_STOP)),
                    MsgSfxType::Play => tag.push_attribute((ATTR_CMD, SFX_PLAY)),
                    MsgSfxType::FadeOut(time) => {
                        tag.push_attribute((ATTR_CMD, SFX_FADE_OUT));
                        tag.push_attribute((ATTR_DURATION, time.to_string().as_ref()));
                    }
                    MsgSfxType::FadeIn(time) => {
                        tag.push_attribute((ATTR_CMD, SFX_FADE_IN));
                        tag.push_attribute((ATTR_DURATION, time.to_string().as_ref()));
                    }
                    MsgSfxType::Fade(arg) => {
                        tag.push_attribute((ATTR_CMD, SFX_FADE));
                        tag.push_attribute((ATTR_DURATION, arg.duration.to_string().as_ref()));
                        tag.push_attribute((ATTR_VOLUME, arg.volume.to_string().as_ref()));
                    }
                    MsgSfxType::Unk5 => tag.push_attribute((ATTR_CMD, SFX_UNK_5)),
                    MsgSfxType::Unk6 => tag.push_attribute((ATTR_CMD, SFX_UNK_6)),
                }
                self.writer.write_event(Event::Empty(tag))?;
            }

            MsgCommand::Voice(voice) => {
                let mut tag = BytesStart::new(ELEM_VOICE);
                tag.push_attribute((ATTR_ID, voice.to_id()));
                self.writer.write_event(Event::Empty(tag))?;
            }

            MsgCommand::Default(arg) => {
                let mut tag = BytesStart::new(ELEM_DEFAULT);
                if arg.flags.contains(DefaultFlags::VARIABLE) {
                    tag.push_attribute((ATTR_VAR, arg.index.to_string().as_ref()));
                } else {
                    tag.push_attribute((ATTR_ID, arg.index.to_string().as_ref()));
                }
                self.writer.write_event(Event::Empty(tag))?;
            }

            MsgCommand::Newline => {
                let tag = BytesStart::new(ELEM_NEWLINE);
                self.writer.write_event(Event::Empty(tag))?;
            }

            MsgCommand::NewlineVt => {
                let tag = BytesStart::new(ELEM_NEWLINE_VT);
                self.writer.write_event(Event::Empty(tag))?;
            }

            MsgCommand::Format(text) => {
                let tag = BytesStart::new(ELEM_FORMAT);
                self.writer.write_event(Event::Start(tag.borrow()))?;
                let decoded = text_to_xml(text)?;
                self.writer.write_event(Event::Text(BytesText::from_escaped(decoded)))?;
                self.writer.write_event(Event::End(tag.to_end()))?;
            }

            MsgCommand::Size(size) => {
                let mut tag = BytesStart::new(ELEM_FONT);
                tag.push_attribute((ATTR_SIZE, size.to_string().as_ref()));
                self.writer.write_event(Event::Empty(tag))?;
            }

            MsgCommand::Color(color) => {
                let mut tag = BytesStart::new(ELEM_FONT);
                tag.push_attribute((ATTR_COLOR, color.to_id()));
                self.writer.write_event(Event::Empty(tag))?;
            }

            MsgCommand::Rgba(rgba) => {
                let mut tag = BytesStart::new(ELEM_FONT);
                tag.push_attribute((ATTR_COLOR, format!("#{:08x}", rgba).as_ref()));
                self.writer.write_event(Event::Empty(tag))?;
            }

            MsgCommand::Proportional(b) => {
                let mut tag = BytesStart::new(ELEM_FONT);
                tag.push_attribute((ATTR_MONO, (!b).to_string().as_ref()));
                self.writer.write_event(Event::Empty(tag))?;
            }

            MsgCommand::Icon(icon) => {
                let mut tag = BytesStart::new(ELEM_ICON);
                tag.push_attribute((ATTR_ID, icon.to_id()));
                self.writer.write_event(Event::Empty(tag))?;
            }

            MsgCommand::Shake(arg) => {
                let mut tag = BytesStart::new(ELEM_SHAKE);
                let ty = if arg.flags.contains(ShakeFlags::JITTER) {
                    SHAKE_JITTER
                } else if arg.flags.contains(ShakeFlags::WAVE) {
                    SHAKE_WAVE
                } else {
                    SHAKE_NONE
                };
                tag.push_attribute((ATTR_TYPE, ty));
                if arg.flags.intersects(ShakeFlags::JITTER | ShakeFlags::WAVE) {
                    tag.push_attribute((ATTR_STRENGTH, arg.strength.to_string().as_ref()));
                    tag.push_attribute((ATTR_SPEED, arg.speed.to_string().as_ref()));
                    let x = arg.flags.contains(ShakeFlags::X);
                    let y = arg.flags.contains(ShakeFlags::Y);
                    let size = arg.flags.contains(ShakeFlags::SIZE);
                    let rotation = arg.flags.contains(ShakeFlags::ROTATION);
                    tag.push_attribute((ATTR_X, x.to_string().as_ref()));
                    tag.push_attribute((ATTR_Y, y.to_string().as_ref()));
                    tag.push_attribute((ATTR_SIZE, size.to_string().as_ref()));
                    tag.push_attribute((ATTR_ROTATION, rotation.to_string().as_ref()));
                }
                self.writer.write_event(Event::Empty(tag))?;
            }

            MsgCommand::Center(center) => {
                let mut tag = BytesStart::new(ELEM_FONT);
                tag.push_attribute((ATTR_ALIGN, if *center { ALIGN_CENTER } else { ALIGN_LEFT }));
                self.writer.write_event(Event::Empty(tag))?;
            }

            MsgCommand::Rotate(rotation) => {
                let mut tag = BytesStart::new(ELEM_FONT);
                tag.push_attribute((ATTR_ROTATION, rotation.to_string().as_ref()));
                self.writer.write_event(Event::Empty(tag))?;
            }

            MsgCommand::Scale(x, y) => {
                let mut tag = BytesStart::new(ELEM_FONT);
                tag.push_attribute((ATTR_SCALE, format!("{},{}", x, y).as_ref()));
                self.writer.write_event(Event::Empty(tag))?;
            }

            MsgCommand::NumInput(arg) => {
                let mut tag = BytesStart::new(ELEM_NUM_INPUT);
                tag.push_attribute((ATTR_DIGITS, arg.digits.to_string().as_ref()));
                tag.push_attribute((ATTR_EDITABLE, arg.editable.to_string().as_ref()));
                tag.push_attribute((ATTR_SELECTED, arg.selected.to_string().as_ref()));
                self.writer.write_event(Event::Empty(tag))?;
            }

            MsgCommand::Question(arg) => {
                let mut tag = BytesStart::new(ELEM_QUESTION);
                let yn = |f| if arg.flags.contains(f) { QUESTION_NO } else { QUESTION_YES };
                tag.push_attribute((ATTR_LEFT, yn(QuestionFlags::LEFT_NO).to_string().as_ref()));
                tag.push_attribute((ATTR_RIGHT, yn(QuestionFlags::RIGHT_NO).to_string().as_ref()));
                tag.push_attribute((ATTR_DEFAULT, arg.default.to_string().as_ref()));
                self.writer.write_event(Event::Empty(tag))?;
            }

            MsgCommand::Stay => {
                self.writer.write_event(Event::Empty(BytesStart::new(ELEM_STAY)))?;
            }

            MsgCommand::Text(text) => {
                let decoded = text_to_xml(text)?;
                self.writer.write_event(Event::Text(BytesText::from_escaped(decoded)))?;
            }
        }

        self.new_line = false;
        if is_text {
            // Line breaks terminate text tags and the line to make things easier to read
            if matches!(command, MsgCommand::Newline | MsgCommand::NewlineVt) {
                self.end_text()?;
                self.next_line()?;
            }
        } else {
            self.next_line()?;
        }
        Ok(())
    }

    fn begin_text(&mut self) -> Result<()> {
        if !self.in_text {
            let tag = BytesStart::new(ELEM_TEXT);
            self.writer.write_event(Event::Start(tag))?;
            self.in_text = true;
        }
        Ok(())
    }

    fn end_text(&mut self) -> Result<()> {
        if self.in_text {
            let tag = BytesStart::new(ELEM_TEXT);
            self.writer.write_event(Event::End(tag.to_end()))?;
            self.in_text = false;
        }
        Ok(())
    }

    fn next_line(&mut self) -> Result<()> {
        if !self.new_line {
            self.writer.inner().write_all(b"\n")?;
            self.new_line = true;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::str;
    use unplug::data::Music;
    use unplug::event::msg::{
        Color, DefaultArgs, Icon, MsgAnimArgs, MsgSfxFadeArgs, NumInputArgs, QuestionArgs,
        ShakeArgs, Voice,
    };

    fn text(s: &str) -> Text {
        Text::encode(s).unwrap()
    }

    fn xml(command: MsgCommand) -> Result<String> {
        let mut writer = MessageWriter::new(Cursor::<Vec<u8>>::default());
        writer.write_commands(&[command])?;
        let bytes = writer.writer.into_inner().into_inner();
        Ok(str::from_utf8(&bytes)?.trim().to_owned())
    }

    #[test]
    fn test_text_to_xml() -> Result<()> {
        assert!(matches!(&text_to_xml(&text(""))?, Cow::Borrowed(_)));
        assert!(matches!(&text_to_xml(&text("ABC"))?, Cow::Borrowed(_)));
        assert!(matches!(&text_to_xml(&text("ABC DEF"))?, Cow::Borrowed(_)));
        assert!(matches!(&text_to_xml(&text("\"ABC'DEF\""))?, Cow::Borrowed(_)));

        assert_eq!(text_to_xml(&text("<&>"))?, "&lt;&amp;&gt;");
        assert_eq!(text_to_xml(&text("< ABC & DEF >"))?, "&lt; ABC &amp; DEF &gt;");
        assert_eq!(text_to_xml(&text("<！>"))?, "&lt;！&gt;");
        Ok(())
    }

    #[test]
    fn test_export_speed() -> Result<()> {
        assert_eq!(xml(MsgCommand::Speed(2))?, "<font speed=\"2\"/>");
        Ok(())
    }

    #[test]
    fn test_export_wait() -> Result<()> {
        assert_eq!(
            xml(MsgCommand::Wait(MsgWaitType::Time(100)))?,
            "<wait type=\"time\" duration=\"100\"/>"
        );
        assert_eq!(xml(MsgCommand::Wait(MsgWaitType::LeftPlug))?, "<wait type=\"left-plug\"/>");
        assert_eq!(xml(MsgCommand::Wait(MsgWaitType::RightPlug))?, "<wait type=\"right-plug\"/>");
        assert_eq!(xml(MsgCommand::Wait(MsgWaitType::SuitMenu))?, "<wait type=\"suit-menu\"/>");
        assert_eq!(xml(MsgCommand::Wait(MsgWaitType::AtcMenu))?, "<wait type=\"atc-menu\"/>");
        Ok(())
    }

    #[test]
    fn test_export_anim() -> Result<()> {
        assert_eq!(
            xml(MsgCommand::Anim(MsgAnimArgs { flags: 1, obj: 2, anim: 3 }))?,
            "<animate flags=\"1\" obj=\"2\" id=\"3\"/>"
        );
        Ok(())
    }

    #[test]
    fn test_export_sfx() -> Result<()> {
        let sfx = Music::Bgm.into();
        assert_eq!(
            xml(MsgCommand::Sfx(sfx, MsgSfxType::Wait))?,
            "<sound name=\"bgm\" cmd=\"wait\"/>"
        );
        assert_eq!(
            xml(MsgCommand::Sfx(sfx, MsgSfxType::Stop))?,
            "<sound name=\"bgm\" cmd=\"stop\"/>"
        );
        assert_eq!(
            xml(MsgCommand::Sfx(sfx, MsgSfxType::Play))?,
            "<sound name=\"bgm\" cmd=\"play\"/>"
        );
        assert_eq!(
            xml(MsgCommand::Sfx(sfx, MsgSfxType::FadeOut(2)))?,
            "<sound name=\"bgm\" cmd=\"fade-out\" duration=\"2\"/>"
        );
        assert_eq!(
            xml(MsgCommand::Sfx(sfx, MsgSfxType::FadeIn(2)))?,
            "<sound name=\"bgm\" cmd=\"fade-in\" duration=\"2\"/>"
        );
        assert_eq!(
            xml(MsgCommand::Sfx(sfx, MsgSfxType::Fade(MsgSfxFadeArgs { duration: 2, volume: 3 })))?,
            "<sound name=\"bgm\" cmd=\"fade\" duration=\"2\" volume=\"3\"/>"
        );
        Ok(())
    }

    #[test]
    fn test_export_voice() -> Result<()> {
        assert_eq!(xml(MsgCommand::Voice(Voice::Peekoe))?, "<voice id=\"peekoe\"/>");
        Ok(())
    }

    #[test]
    fn test_export_default() -> Result<()> {
        assert_eq!(
            xml(MsgCommand::Default(DefaultArgs { flags: DefaultFlags::empty(), index: 1 }))?,
            "<default id=\"1\"/>"
        );
        assert_eq!(
            xml(MsgCommand::Default(DefaultArgs { flags: DefaultFlags::VARIABLE, index: 1 }))?,
            "<default var=\"1\"/>"
        );
        Ok(())
    }

    #[test]
    fn test_export_newline() -> Result<()> {
        assert_eq!(xml(MsgCommand::Newline)?, "<text><br/></text>");
        assert_eq!(xml(MsgCommand::NewlineVt)?, "<text><br-vt/></text>");
        Ok(())
    }

    #[test]
    fn test_export_format() -> Result<()> {
        assert_eq!(xml(MsgCommand::Format(Text::encode("%s")?))?, "<text><f>%s</f></text>");
        Ok(())
    }

    #[test]
    fn test_export_size() -> Result<()> {
        assert_eq!(xml(MsgCommand::Size(24))?, "<font size=\"24\"/>");
        Ok(())
    }

    #[test]
    fn test_export_color() -> Result<()> {
        assert_eq!(xml(MsgCommand::Color(Color::Lime))?, "<font color=\"lime\"/>");
        assert_eq!(xml(MsgCommand::Rgba(0x12345678))?, "<font color=\"#12345678\"/>");
        Ok(())
    }

    #[test]
    fn test_export_mono() -> Result<()> {
        assert_eq!(xml(MsgCommand::Proportional(false))?, "<font mono=\"true\"/>");
        assert_eq!(xml(MsgCommand::Proportional(true))?, "<font mono=\"false\"/>");
        Ok(())
    }

    #[test]
    fn test_export_icon() -> Result<()> {
        assert_eq!(xml(MsgCommand::Icon(Icon::Moolah))?, "<text><icon id=\"moolah\"/></text>");
        Ok(())
    }

    #[test]
    fn test_export_shake() -> Result<()> {
        assert_eq!(
            xml(MsgCommand::Shake(ShakeArgs {
                strength: 1,
                speed: 2,
                flags: ShakeFlags::JITTER
                    | ShakeFlags::X
                    | ShakeFlags::Y
                    | ShakeFlags::SIZE
                    | ShakeFlags::ROTATION
            }))?,
            "<shake type=\"jitter\" strength=\"1\" speed=\"2\" x=\"true\" y=\"true\" size=\"true\" \
                    rotation=\"true\"/>"
        );
        assert_eq!(
            xml(MsgCommand::Shake(ShakeArgs {
                strength: 1,
                speed: 2,
                flags: ShakeFlags::WAVE | ShakeFlags::X | ShakeFlags::Y
            }))?,
            "<shake type=\"wave\" strength=\"1\" speed=\"2\" x=\"true\" y=\"true\" size=\"false\" \
                    rotation=\"false\"/>"
        );
        assert_eq!(
            xml(MsgCommand::Shake(ShakeArgs {
                strength: 0,
                speed: 0,
                flags: ShakeFlags::empty()
            }))?,
            "<shake type=\"none\"/>"
        );
        Ok(())
    }

    #[test]
    fn test_export_align() -> Result<()> {
        assert_eq!(xml(MsgCommand::Center(false))?, "<font align=\"left\"/>");
        assert_eq!(xml(MsgCommand::Center(true))?, "<font align=\"center\"/>");
        Ok(())
    }

    #[test]
    fn test_export_rotation() -> Result<()> {
        assert_eq!(xml(MsgCommand::Rotate(180))?, "<font rotation=\"180\"/>");
        Ok(())
    }

    #[test]
    fn test_export_num_input() -> Result<()> {
        assert_eq!(
            xml(MsgCommand::NumInput(NumInputArgs { digits: 1, editable: 2, selected: 3 }))?,
            "<num-input digits=\"1\" editable=\"2\" selected=\"3\"/>"
        );
        Ok(())
    }

    #[test]
    fn test_export_question() -> Result<()> {
        assert_eq!(
            xml(MsgCommand::Question(QuestionArgs { flags: QuestionFlags::RIGHT_NO, default: 1 }))?,
            "<question left=\"yes\" right=\"no\" default=\"1\"/>"
        );
        assert_eq!(
            xml(MsgCommand::Question(QuestionArgs { flags: QuestionFlags::LEFT_NO, default: 1 }))?,
            "<question left=\"no\" right=\"yes\" default=\"1\"/>"
        );
        Ok(())
    }

    #[test]
    fn test_import_stay() -> Result<()> {
        assert_eq!(xml(MsgCommand::Stay)?, "<stay/>");
        Ok(())
    }
}
