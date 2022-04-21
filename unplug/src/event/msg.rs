use super::opcodes::*;
use crate::common::text::{self, Text};
use crate::common::{ReadFrom, SfxId, WriteTo};
use bitflags::bitflags;
use byteorder::{ReadBytesExt, WriteBytesExt, BE, LE};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::convert::TryFrom;
use std::fmt;
use std::io::{self, Read, Seek, SeekFrom, Write};
use thiserror::Error;
use tracing::error;

/// The maximum size of a serialized message command list in bytes.
const MAX_SIZE: u64 = 2048;
/// The maximum number of characters that can appear in a message.
const MAX_CHARS: usize = 400;

/// The result type for message operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for message operations.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("message text could not be encoded with SHIFT-JIS or Windows-1252: {0:?}")]
    Encode(String),

    #[error("message text could not be decoded with SHIFT-JIS or Windows-1252")]
    Decode,

    #[error("invalid message character: {0:#x}")]
    InvalidChar(u32),

    #[error("message is too large ({len} > {max})")]
    TooLarge { len: u64, max: u64 },

    #[error("message has too many characters ({len} > {max})")]
    TooManyChars { len: usize, max: usize },

    #[error("unrecognized message color: {0}")]
    UnrecognizedColor(u8),

    #[error("unrecognized message SFX command: {0}")]
    UnrecognizedSfx(i8),

    #[error("unrecognized sound ID in message: {0}")]
    UnrecognizedSound(u32),

    #[error("unrecognized message icon: {0}")]
    UnrecognizedIcon(u8),

    #[error("unrecognized message voice: {0}")]
    UnrecognizedVoice(u8),

    #[error("invalid message")]
    Invalid,

    #[error(transparent)]
    Text(Box<text::Error>),

    #[error(transparent)]
    Io(Box<io::Error>),
}

from_error_boxed!(Error::Text, text::Error);
from_error_boxed!(Error::Io, io::Error);

/// Commands that make up a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MsgCommand {
    /// Message speed (higher = slower, normal speed is 2). Use 255 to reset.
    Speed(u8),
    /// Waits for an amount of time or for a button to be pressed.
    Wait(MsgWaitType),
    /// Plays an animation.
    Anim(MsgAnimArgs),
    /// Plays a sound effect.
    Sfx(SfxId, MsgSfxType),
    /// Sets the voice to play.
    Voice(Voice),
    /// Sets the index of the default option in a `Select` command.
    Default(DefaultArgs),
    /// A line break.
    Newline,
    /// Functionally equivalent to `Newline` except it maps to a vertical tab character.
    NewlineVt,
    /// A printf format specifier.
    Format(Text),
    /// Sets the font size (higher = larger, normal size is 22). Use 255 to reset.
    Size(u8),
    /// Sets the font color from a preset.
    Color(Color),
    /// Sets the font color to an arbitrary RGBA color.
    Rgba(u32),
    /// Sets whether the font should be proportional (i.e. false = monospace).
    Proportional(bool),
    /// Displays an icon.
    Icon(Icon),
    /// Shakes characters.
    Shake(ShakeArgs),
    /// Sets whether text should be centered (false = left-aligned).
    Center(bool),
    /// Sets the rotation of each character in degrees.
    Rotate(i16),
    /// Sets the X and Y scale of each character as percentages.
    Scale(i16, i16),
    /// Prompts the player to input a number.
    NumInput(NumInputArgs),
    /// Prompts the player to answer a yes/no question.
    Question(QuestionArgs),
    /// Exits the msg() command but keeps the message on-screen.
    Stay,
    /// Displays raw text.
    Text(Text),
}

/// Arguments to the msg() and select() commands.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct MsgArgs {
    /// The commands that make up the message.
    pub commands: Vec<MsgCommand>,
    /// Data stored after the command list. Completely ignored by the game; useful for e.g. message
    /// editors to tag messages.
    pub extra_data: Vec<u8>,
}

impl MsgArgs {
    /// Constructs an empty `MsgArgs`.
    pub fn new() -> Self {
        Self::default()
    }
}

impl fmt::Debug for MsgArgs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.commands.fmt(f)
    }
}

impl From<Text> for MsgArgs {
    fn from(text: Text) -> Self {
        Self { commands: vec![MsgCommand::Text(text)], extra_data: vec![] }
    }
}

impl From<MsgCommand> for MsgArgs {
    fn from(command: MsgCommand) -> Self {
        Self { commands: vec![command], extra_data: vec![] }
    }
}

impl<T: Into<Vec<MsgCommand>>> From<T> for MsgArgs {
    fn from(commands: T) -> Self {
        Self { commands: commands.into(), extra_data: vec![] }
    }
}

impl<R: Read + Seek + ?Sized> ReadFrom<R> for MsgArgs {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        // The message string is prefixed with the offset of the next command. Technically this can
        // be anything, but the official game always stores the offset immediately after the null
        // terminator. We can abuse this to enable editors to tag messages with arbitrary data.
        let end_offset = reader.read_i32::<LE>()? as u64;
        let start_offset = reader.seek(SeekFrom::Current(0))?;
        if end_offset <= start_offset {
            error!(
                "Message end offset ({:#x}) is before the start offset ({:#x})!",
                end_offset, start_offset
            );
            return Err(Error::Invalid);
        }
        let expected_length = end_offset - start_offset;

        // Commands are encoded as control characters in the text. Any non-special character is
        // displayed as text. Keep reading until we hit a null terminator (MSG_END).
        let mut commands = vec![];
        let mut text = Vec::with_capacity((expected_length - 1) as usize);
        loop {
            let b = reader.read_u8()?;
            let command = match b {
                MSG_END => None,
                MSG_SPEED => Some(MsgCommand::Speed(reader.read_u8()?)),
                MSG_WAIT => Some(MsgCommand::Wait(MsgWaitType::read_from(reader)?)),
                MSG_ANIM => Some(MsgCommand::Anim(MsgAnimArgs::read_from(reader)?)),
                MSG_SFX => {
                    let sound = match SfxId::try_from(reader.read_u32::<LE>()?) {
                        Ok(sound) => sound,
                        Err(id) => return Err(Error::UnrecognizedSound(id)),
                    };
                    Some(MsgCommand::Sfx(sound, MsgSfxType::read_from(reader)?))
                }
                MSG_VOICE => {
                    let voice = reader.read_u8()?;
                    match Voice::try_from(voice) {
                        Ok(voice) => Some(MsgCommand::Voice(voice)),
                        Err(_) => return Err(Error::UnrecognizedVoice(voice)),
                    }
                }
                MSG_DEFAULT => Some(MsgCommand::Default(DefaultArgs::read_from(reader)?)),
                MSG_NEWLINE => Some(MsgCommand::Newline),
                MSG_NEWLINE_VT => Some(MsgCommand::NewlineVt),
                MSG_FORMAT => {
                    let mut format_text = vec![];
                    loop {
                        let b = reader.read_u8()?;
                        if b == MSG_FORMAT {
                            break;
                        }
                        format_text.push(b);
                    }
                    Some(MsgCommand::Format(Text::with_bytes(format_text)))
                }
                MSG_SIZE => Some(MsgCommand::Size(reader.read_u8()?)),
                MSG_COLOR => {
                    let color = reader.read_u8()?;
                    match Color::try_from(color) {
                        Ok(color) => Some(MsgCommand::Color(color)),
                        Err(_) => return Err(Error::UnrecognizedColor(color)),
                    }
                }
                MSG_RGBA => {
                    // Yes, this is big-endian...
                    Some(MsgCommand::Rgba(reader.read_u32::<BE>()?))
                }
                MSG_PROPORTIONAL => Some(MsgCommand::Proportional(reader.read_u8()? != 0)),
                MSG_ICON => {
                    let icon = reader.read_u8()?;
                    match Icon::try_from(icon) {
                        Ok(icon) => Some(MsgCommand::Icon(icon)),
                        Err(_) => return Err(Error::UnrecognizedIcon(icon)),
                    }
                }
                MSG_SHAKE => Some(MsgCommand::Shake(ShakeArgs::read_from(reader)?)),
                MSG_CENTER => Some(MsgCommand::Center(reader.read_u8()? != 0)),
                MSG_ROTATE => Some(MsgCommand::Rotate(reader.read_i16::<LE>()?)),
                MSG_SCALE => {
                    Some(MsgCommand::Scale(reader.read_i16::<LE>()?, reader.read_i16::<LE>()?))
                }
                MSG_NUM_INPUT => Some(MsgCommand::NumInput(NumInputArgs::read_from(reader)?)),
                MSG_QUESTION => Some(MsgCommand::Question(QuestionArgs::read_from(reader)?)),
                MSG_STAY => Some(MsgCommand::Stay),
                _ => None,
            };
            if command.is_some() || b == MSG_END {
                if !text.is_empty() {
                    commands.push(MsgCommand::Text(Text::with_bytes(text.clone())));
                    text.clear();
                }
                if let Some(command) = command {
                    commands.push(command);
                } else if b == MSG_END {
                    break;
                }
            } else {
                text.push(match b {
                    // '$' is an escape character for '"'
                    b'$' => b'"',
                    _ => b,
                });
            }
        }

        // Read the "extra" data between the null terminator and the next command
        let offset = reader.seek(SeekFrom::Current(0))?;
        if offset > end_offset {
            error!("Read past the end of the message!");
            return Err(Error::Invalid);
        }
        let extra_data_len = (end_offset - offset) as usize;
        let mut extra_data = vec![0u8; extra_data_len];
        reader.read_exact(&mut extra_data)?;

        commands.shrink_to_fit();
        Ok(MsgArgs { commands, extra_data })
    }
}

impl<W: Write + Seek + ?Sized> WriteTo<W> for MsgArgs {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        // Write a end offset of 0 for now
        let start_offset = writer.seek(SeekFrom::Current(0))?;
        writer.write_i32::<LE>(0)?;

        let mut num_chars = 0;
        for command in &self.commands {
            match command {
                MsgCommand::Speed(a) => {
                    writer.write_u8(MSG_SPEED)?;
                    writer.write_u8(*a)?;
                }
                MsgCommand::Wait(ty) => {
                    writer.write_u8(MSG_WAIT)?;
                    ty.write_to(writer)?;
                }
                MsgCommand::Anim(arg) => {
                    writer.write_u8(MSG_ANIM)?;
                    arg.write_to(writer)?;
                }
                MsgCommand::Sfx(id, ty) => {
                    writer.write_u8(MSG_SFX)?;
                    writer.write_u32::<LE>(u32::from(*id))?;
                    ty.write_to(writer)?;
                }
                MsgCommand::Voice(voice) => {
                    writer.write_u8(MSG_VOICE)?;
                    writer.write_u8((*voice).into())?;
                }
                MsgCommand::Default(arg) => {
                    writer.write_u8(MSG_DEFAULT)?;
                    arg.write_to(writer)?;
                }
                MsgCommand::Newline => {
                    writer.write_u8(MSG_NEWLINE)?;
                }
                MsgCommand::NewlineVt => {
                    writer.write_u8(MSG_NEWLINE_VT)?;
                }
                MsgCommand::Format(text) => {
                    // Note: format strings aren't counted towards the MAX_CHARS limit here because
                    // they can expand to any length. If you use a format string in a long message,
                    // you're on your own.
                    writer.write_u8(MSG_FORMAT)?;
                    writer.write_all(text.as_bytes())?;
                    writer.write_u8(MSG_FORMAT)?;
                }
                MsgCommand::Size(size) => {
                    writer.write_u8(MSG_SIZE)?;
                    writer.write_u8(*size)?;
                }
                MsgCommand::Color(color) => {
                    writer.write_u8(MSG_COLOR)?;
                    writer.write_u8((*color).into())?;
                }
                MsgCommand::Rgba(rgba) => {
                    writer.write_u8(MSG_RGBA)?;
                    // Yes, this is big-endian...
                    writer.write_u32::<BE>(*rgba)?;
                }
                MsgCommand::Proportional(x) => {
                    writer.write_u8(MSG_PROPORTIONAL)?;
                    writer.write_u8(*x as u8)?;
                }
                MsgCommand::Icon(icon) => {
                    writer.write_u8(MSG_ICON)?;
                    writer.write_u8((*icon).into())?;
                }
                MsgCommand::Shake(arg) => {
                    writer.write_u8(MSG_SHAKE)?;
                    arg.write_to(writer)?;
                }
                MsgCommand::Center(x) => {
                    writer.write_u8(MSG_CENTER)?;
                    writer.write_u8(*x as u8)?;
                }
                MsgCommand::Rotate(angle) => {
                    writer.write_u8(MSG_ROTATE)?;
                    writer.write_i16::<LE>(*angle)?;
                }
                MsgCommand::Scale(x, y) => {
                    writer.write_u8(MSG_SCALE)?;
                    writer.write_i16::<LE>(*x)?;
                    writer.write_i16::<LE>(*y)?;
                }
                MsgCommand::NumInput(arg) => {
                    writer.write_u8(MSG_NUM_INPUT)?;
                    arg.write_to(writer)?;
                }
                MsgCommand::Question(arg) => {
                    writer.write_u8(MSG_QUESTION)?;
                    arg.write_to(writer)?;
                }
                MsgCommand::Stay => {
                    writer.write_u8(MSG_STAY)?;
                }
                MsgCommand::Text(text) => {
                    for &b in text.as_bytes() {
                        // Bell, backspace, and tab don't have opcodes assigned
                        if b <= MSG_OPCODE_MAX && b != b'\x07' && b != b'\x08' && b != b'\t' {
                            return Err(Error::InvalidChar(b as u32));
                        }
                        writer.write_u8(match b {
                            // '$' is an escape character for '"'
                            b'"' => b'$',
                            _ => b,
                        })?;
                    }
                    // For the purposes of enforcing the MAX_CHARS limit, assume each byte is one
                    // character. We shouldn't call `from_str()` here because it handles SHIFT-JIS
                    // and the NTSC-U version will always interpret text as Windows-1252. Supporting
                    // other regions may require changing this calculation.
                    num_chars += text.as_bytes().len();
                }
            }
        }
        if num_chars > MAX_CHARS {
            return Err(Error::TooManyChars { len: num_chars, max: MAX_CHARS });
        }

        // Add a null terminator
        writer.write_u8(MSG_END)?;

        // Ensure we don't overflow the game's message buffer
        let msg_end_offset = writer.seek(SeekFrom::Current(0))?;
        let msg_size = msg_end_offset - start_offset;
        if msg_size > MAX_SIZE {
            return Err(Error::TooLarge { len: msg_size, max: MAX_SIZE });
        }

        // Write the extra data after the null terminator because the game will ignore it
        writer.write_all(&self.extra_data)?;

        // Now go back and fill in the end offset
        let end_offset = writer.seek(SeekFrom::Current(0))?;
        writer.seek(SeekFrom::Start(start_offset))?;
        writer.write_i32::<LE>(i32::try_from(end_offset).expect("Message offset overflow"))?;
        writer.seek(SeekFrom::Start(end_offset))?;
        Ok(())
    }
}

/// Message wait types.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum MsgWaitType {
    Time(u8),
    /// Waits for the player to press X and then opens the attachment menu.
    AtcMenu,
    /// Waits for the player to press X and then opens the suit menu.
    SuitMenu,
    /// Waits for the player to press A/B and shows a plug facing to the left.
    LeftPlug,
    /// Waits for the player to press A/B and shows a plug facing to the right.
    RightPlug,
}

impl<R: Read + ?Sized> ReadFrom<R> for MsgWaitType {
    type Error = io::Error;
    fn read_from(reader: &mut R) -> io::Result<Self> {
        let ty = reader.read_u8()?;
        Ok(match ty {
            MSG_WAIT_ATC_MENU => Self::AtcMenu,
            MSG_WAIT_SUIT_MENU => Self::SuitMenu,
            MSG_WAIT_LEFT_PLUG => Self::LeftPlug,
            MSG_WAIT_RIGHT_PLUG => Self::RightPlug,
            x => Self::Time(x),
        })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for MsgWaitType {
    type Error = io::Error;
    fn write_to(&self, writer: &mut W) -> io::Result<()> {
        match self {
            Self::Time(x) => writer.write_u8(*x)?,
            Self::AtcMenu => writer.write_u8(MSG_WAIT_ATC_MENU)?,
            Self::SuitMenu => writer.write_u8(MSG_WAIT_SUIT_MENU)?,
            Self::LeftPlug => writer.write_u8(MSG_WAIT_LEFT_PLUG)?,
            Self::RightPlug => writer.write_u8(MSG_WAIT_RIGHT_PLUG)?,
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsgAnimArgs {
    pub flags: u8,
    pub obj: i16,
    pub anim: i32,
}

impl<R: Read + ?Sized> ReadFrom<R> for MsgAnimArgs {
    type Error = io::Error;
    fn read_from(reader: &mut R) -> io::Result<Self> {
        Ok(Self {
            flags: reader.read_u8()?,
            obj: reader.read_i16::<LE>()?,
            anim: reader.read_i32::<LE>()?,
        })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for MsgAnimArgs {
    type Error = io::Error;
    fn write_to(&self, writer: &mut W) -> io::Result<()> {
        writer.write_u8(self.flags)?;
        writer.write_i16::<LE>(self.obj)?;
        writer.write_i32::<LE>(self.anim)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MsgSfxType {
    /// Waits for the sound to finish playing.
    Wait,
    /// Stops the sound.
    Stop,
    /// Plays the sound.
    Play,
    /// Fades the sound out for the specified amount of time.
    FadeOut(u16),
    /// Fades the sound in for the specified amount of time.
    FadeIn(u16),
    /// Fades the sound to a volume level over time.
    Fade(MsgSfxFadeArgs),
    Unk5,
    Unk6,
}

impl<R: Read + ?Sized> ReadFrom<R> for MsgSfxType {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let ty = reader.read_i8()?;
        Ok(match ty as i32 {
            SFX_WAIT => Self::Wait,
            SFX_STOP => Self::Stop,
            SFX_PLAY => Self::Play,
            SFX_FADE_OUT => Self::FadeOut(reader.read_u16::<LE>()?),
            SFX_FADE_IN => Self::FadeIn(reader.read_u16::<LE>()?),
            SFX_FADE => Self::Fade(MsgSfxFadeArgs::read_from(reader)?),
            SFX_UNK_5 => Self::Unk5,
            SFX_UNK_6 => Self::Unk6,
            _ => return Err(Error::UnrecognizedSfx(ty)),
        })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for MsgSfxType {
    type Error = io::Error;
    fn write_to(&self, writer: &mut W) -> io::Result<()> {
        match self {
            Self::Wait => writer.write_i8(SFX_WAIT as i8)?,
            Self::Stop => writer.write_i8(SFX_STOP as i8)?,
            Self::Play => writer.write_i8(SFX_PLAY as i8)?,
            Self::FadeOut(duration) => {
                writer.write_i8(SFX_FADE_OUT as i8)?;
                writer.write_u16::<LE>(*duration)?;
            }
            Self::FadeIn(duration) => {
                writer.write_i8(SFX_FADE_IN as i8)?;
                writer.write_u16::<LE>(*duration)?;
            }
            Self::Fade(arg) => {
                writer.write_i8(SFX_FADE as i8)?;
                arg.write_to(writer)?;
            }
            Self::Unk5 => writer.write_i8(SFX_UNK_5 as i8)?,
            Self::Unk6 => writer.write_i8(SFX_UNK_6 as i8)?,
        }
        Ok(())
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct MsgSfxFadeArgs {
    /// The fade duration.
    pub duration: u16,
    /// The target volume (0 = silent, 255 = max).
    pub volume: u8,
}

impl<R: Read + ?Sized> ReadFrom<R> for MsgSfxFadeArgs {
    type Error = io::Error;
    fn read_from(reader: &mut R) -> io::Result<Self> {
        Ok(Self { duration: reader.read_u16::<LE>()?, volume: reader.read_u8()? })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for MsgSfxFadeArgs {
    type Error = io::Error;
    fn write_to(&self, writer: &mut W) -> io::Result<()> {
        writer.write_u16::<LE>(self.duration)?;
        writer.write_u8(self.volume)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultArgs {
    /// Flags which control the meaning of `index`.
    pub flags: DefaultFlags,
    /// The default item index starting from 0 (or a variable index if the `VARIABLE` flag is set).
    pub index: i32,
}

impl<R: Read + ?Sized> ReadFrom<R> for DefaultArgs {
    type Error = io::Error;
    fn read_from(reader: &mut R) -> io::Result<Self> {
        Ok(Self {
            flags: DefaultFlags::from_bits_truncate(reader.read_u8()?),
            index: reader.read_i32::<LE>()?,
        })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for DefaultArgs {
    type Error = io::Error;
    fn write_to(&self, writer: &mut W) -> io::Result<()> {
        writer.write_u8(self.flags.bits())?;
        writer.write_i32::<LE>(self.index)?;
        Ok(())
    }
}

bitflags! {
    pub struct DefaultFlags: u8 {
        /// The `index` is a variable index rather than a constant.
        const VARIABLE = 0x1;
    }
}

/// Character voices.
#[derive(Debug, Copy, Clone, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum Voice {
    None = 255,
    Telly = 0,
    Frog = 1,
    Jenny = 2,
    Papa = 3,
    Mama = 4,
    Unk5 = 5,
    Unk6 = 6,
    Drake = 7,
    Captain = 8,
    Soldier = 9,
    Peekoe = 10,
    Sophie = 11,
    News1 = 12,
    Sarge = 13,
    JennyFrog = 14,
    Primo = 15,
    Prongs = 16,
    Et = 17,
    Funky = 18,
    Dinah = 19,
    Pitts = 20,
    Mort = 21,
    Sunshine = 22,
    SunshineHungry = 23,
    DinahToothless = 24,
    Fred = 25,
    Freida = 26,
    Tao = 27,
    Ufo = 28,
    Underwater = 29,
    Eggplant = 30,
    Phillies = 31,
    Gebah = 32,
    News2 = 33,
}

/// Message color presets.
#[derive(Debug, Copy, Clone, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum Color {
    /// (255, 255, 255, 255)
    White = 0,
    /// (128, 128, 128, 255)
    Gray = 1,
    /// (16, 16, 16, 255)
    DarkGray = 2,
    /// (0, 255, 255, 255)
    Cyan = 3,
    /// (0, 224, 0, 255)
    Lime = 4,
    /// (0, 0, 224, 255)
    Blue = 5,
    /// (255, 0, 255, 255)
    Magenta = 6,
    /// (224, 0, 0, 255)
    Red = 7,
    /// (255, 255, 0, 255)
    Yellow = 8,
    /// (255, 128, 0, 255)
    Orange = 9,
    /// Reset to the default color (white).
    Reset = 255,
}

/// Icons.
#[derive(Debug, Copy, Clone, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum Icon {
    Analog = 0,
    Up = 1,
    Right = 2,
    Down = 3,
    Left = 4,
    A = 5,
    B = 6,
    C = 7,
    X = 8,
    Y = 9,
    Z = 10,
    L = 11,
    R = 12,
    Start = 13,
    Moolah = 14,
    Yes = 15,
    No = 16,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ShakeArgs {
    /// Flags which describe how to shake characters. Either `WAVE` or `JITTER` must be set.
    pub flags: ShakeFlags,
    /// Shake strength (higher = stronger).
    pub strength: u8,
    /// Shake speed (higher = slower).
    pub speed: u8,
}

impl<R: Read + ?Sized> ReadFrom<R> for ShakeArgs {
    type Error = io::Error;
    fn read_from(reader: &mut R) -> io::Result<Self> {
        Ok(Self {
            flags: ShakeFlags::from_bits_truncate(reader.read_u8()?),
            strength: reader.read_u8()?,
            speed: reader.read_u8()?,
        })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for ShakeArgs {
    type Error = io::Error;
    fn write_to(&self, writer: &mut W) -> io::Result<()> {
        writer.write_u8(self.flags.bits())?;
        writer.write_u8(self.strength)?;
        writer.write_u8(self.speed)?;
        Ok(())
    }
}

bitflags! {
    pub struct ShakeFlags: u8 {
        /// Shake each character's Y position.
        const Y = 0x1;
        /// Shake each character's X position.
        const X = 0x2;
        /// Shake each character's size.
        const SIZE = 0x4;
        /// Shake each character's rotation.
        const ROTATION = 0x8;
        /// Animate smoothly.
        const WAVE = 0x10;
        /// Animate sharply.
        const JITTER = 0x20;
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct NumInputArgs {
    /// The number of digits to enter.
    pub digits: u8,
    /// The number of digits which are editable (from the left).
    pub editable: u8,
    /// The initially-selected digit (1 = first digit on the right).
    pub selected: u8,
}

impl<R: Read + ?Sized> ReadFrom<R> for NumInputArgs {
    type Error = io::Error;
    fn read_from(reader: &mut R) -> io::Result<Self> {
        Ok(Self {
            digits: reader.read_u8()?,
            editable: reader.read_u8()?,
            selected: reader.read_u8()?,
        })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for NumInputArgs {
    type Error = io::Error;
    fn write_to(&self, writer: &mut W) -> io::Result<()> {
        writer.write_u8(self.digits)?;
        writer.write_u8(self.editable)?;
        writer.write_u8(self.selected)?;
        Ok(())
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct QuestionArgs {
    /// Flags indicating which items (if any) are "no".
    pub flags: QuestionFlags,
    /// The initially-selected item (0 = left, 1 = right).
    pub default: u8,
}

impl<R: Read + ?Sized> ReadFrom<R> for QuestionArgs {
    type Error = io::Error;
    fn read_from(reader: &mut R) -> io::Result<Self> {
        Ok(Self {
            flags: QuestionFlags::from_bits_truncate(reader.read_u8()?),
            default: reader.read_u8()?,
        })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for QuestionArgs {
    type Error = io::Error;
    fn write_to(&self, writer: &mut W) -> io::Result<()> {
        writer.write_u8(self.flags.bits())?;
        writer.write_u8(self.default)?;
        Ok(())
    }
}

bitflags! {
    pub struct QuestionFlags: u8 {
        /// The left option is "no".
        const LEFT_NO = 0x1;
        /// The right option is "no".
        const RIGHT_NO = 0x2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::assert_write_and_read;
    use crate::data::Music;

    fn msg(command: MsgCommand) -> MsgArgs {
        vec![command].into()
    }

    fn text(string: &str) -> Text {
        Text::encode(string).unwrap()
    }

    #[test]
    fn test_write_and_read_msg() {
        let sfx = SfxId::Music(Music::Bgm);
        assert_write_and_read!(msg(MsgCommand::Speed(1)));
        assert_write_and_read!(msg(MsgCommand::Wait(MsgWaitType::Time(1))));
        assert_write_and_read!(msg(MsgCommand::Wait(MsgWaitType::AtcMenu)));
        assert_write_and_read!(msg(MsgCommand::Wait(MsgWaitType::SuitMenu)));
        assert_write_and_read!(msg(MsgCommand::Wait(MsgWaitType::LeftPlug)));
        assert_write_and_read!(msg(MsgCommand::Wait(MsgWaitType::RightPlug)));
        assert_write_and_read!(msg(MsgCommand::Anim(MsgAnimArgs { flags: 1, obj: 2, anim: 3 })));
        assert_write_and_read!(msg(MsgCommand::Sfx(sfx, MsgSfxType::Wait)));
        assert_write_and_read!(msg(MsgCommand::Sfx(sfx, MsgSfxType::Stop)));
        assert_write_and_read!(msg(MsgCommand::Sfx(sfx, MsgSfxType::Play)));
        assert_write_and_read!(msg(MsgCommand::Sfx(sfx, MsgSfxType::FadeOut(2))));
        assert_write_and_read!(msg(MsgCommand::Sfx(sfx, MsgSfxType::FadeIn(2))));
        assert_write_and_read!(msg(MsgCommand::Sfx(
            sfx,
            MsgSfxType::Fade(MsgSfxFadeArgs { duration: 1, volume: 2 })
        )));
        assert_write_and_read!(msg(MsgCommand::Sfx(sfx, MsgSfxType::Unk5)));
        assert_write_and_read!(msg(MsgCommand::Sfx(sfx, MsgSfxType::Unk6)));
        assert_write_and_read!(msg(MsgCommand::Voice(Voice::Gebah)));
        assert_write_and_read!(msg(MsgCommand::Default(DefaultArgs {
            flags: DefaultFlags::VARIABLE,
            index: 1,
        })));
        assert_write_and_read!(msg(MsgCommand::Newline));
        assert_write_and_read!(msg(MsgCommand::NewlineVt));
        assert_write_and_read!(msg(MsgCommand::Format(text("%s"))));
        assert_write_and_read!(msg(MsgCommand::Size(1)));
        assert_write_and_read!(msg(MsgCommand::Color(Color::White)));
        assert_write_and_read!(msg(MsgCommand::Rgba(1)));
        assert_write_and_read!(msg(MsgCommand::Proportional(false)));
        assert_write_and_read!(msg(MsgCommand::Proportional(true)));
        assert_write_and_read!(msg(MsgCommand::Icon(Icon::Moolah)));
        assert_write_and_read!(msg(MsgCommand::Shake(ShakeArgs {
            flags: ShakeFlags::X | ShakeFlags::JITTER,
            strength: 1,
            speed: 2
        })));
        assert_write_and_read!(msg(MsgCommand::Center(false)));
        assert_write_and_read!(msg(MsgCommand::Center(true)));
        assert_write_and_read!(msg(MsgCommand::Rotate(1)));
        assert_write_and_read!(msg(MsgCommand::Scale(1, 2)));
        assert_write_and_read!(msg(MsgCommand::NumInput(NumInputArgs {
            digits: 1,
            editable: 2,
            selected: 3,
        })));
        assert_write_and_read!(msg(MsgCommand::Question(QuestionArgs {
            flags: QuestionFlags::RIGHT_NO,
            default: 1,
        })));
        assert_write_and_read!(msg(MsgCommand::Stay));
        assert_write_and_read!(msg(MsgCommand::Text(text("bunger"))));
        assert_write_and_read!(msg(MsgCommand::Text(text("\"quoted\""))));
        assert_write_and_read!(msg(MsgCommand::Text(text("スプラトゥーン"))));
    }

    #[test]
    fn test_write_and_read_msg_multiple_commands() {
        let msg = MsgArgs::from(vec![
            MsgCommand::Speed(5),
            MsgCommand::Shake(ShakeArgs {
                flags: ShakeFlags::X | ShakeFlags::Y | ShakeFlags::JITTER,
                strength: 1,
                speed: 1,
            }),
            MsgCommand::Size(36),
            MsgCommand::Text(text("bunger ")),
            MsgCommand::Format(text("%s")),
            MsgCommand::Text(text(" bunger")),
            MsgCommand::Wait(MsgWaitType::LeftPlug),
        ]);
        assert_write_and_read!(msg);
    }

    #[test]
    fn test_write_and_read_msg_extra_data() {
        assert_write_and_read!(MsgArgs {
            commands: vec![MsgCommand::Size(36), MsgCommand::Text(text("bunger")),],
            extra_data: vec![0u8, 1u8, 2u8, 3u8],
        });
    }

    #[test]
    fn test_write_and_read_none_voice() {
        assert_write_and_read!(msg(MsgCommand::Voice(Voice::None)));
    }
}
