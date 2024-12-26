use super::opcodes::MsgOp;
use super::serialize::{
    DeserializeEvent, Error as SerError, EventDeserializer, EventSerializer, Result as SerResult,
    SerializeEvent,
};
use crate::common::text::{self, Text, VecText};
use crate::data::{self, Sound};
use bitflags::bitflags;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::fmt;
use thiserror::Error;
use tracing::error;
use unplug_proc::{DeserializeEvent, SerializeEvent};

/// The maximum number of characters that can appear in a message.
const MAX_CHARS: usize = 400;

// Message wait type values
const MSG_WAIT_SUIT_MENU: u8 = 252; // fc
const MSG_WAIT_ATC_MENU: u8 = 253; // fd
const MSG_WAIT_LEFT_PLUG: u8 = 254; // fe
const MSG_WAIT_RIGHT_PLUG: u8 = 255; // ff

// Message SFX commands
const SFX_WAIT: i32 = -1; // ff
const SFX_STOP: i32 = 0; // 00
const SFX_PLAY: i32 = 1; // 01
const SFX_FADE_OUT: i32 = 2; // 02
const SFX_FADE_IN: i32 = 3; // 03
const SFX_FADE: i32 = 4; // 04
const SFX_UNK_5: i32 = 5; // 05
const SFX_UNK_6: i32 = 6; // 06

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

    #[error("unsupported message command: {0:?}")]
    Unsupported(MsgOp),

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

    #[error("unrecognized message layout: {0}")]
    UnrecognizedLayout(u8),

    #[error("unrecognized message voice: {0}")]
    UnrecognizedVoice(u8),

    #[error(transparent)]
    Data(Box<data::Error>),

    #[error(transparent)]
    Serialize(Box<SerError>),

    #[error(transparent)]
    Text(Box<text::Error>),
}

from_error_boxed!(Error::Data, data::Error);
from_error_boxed!(Error::Serialize, SerError);
from_error_boxed!(Error::Text, text::Error);

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
    Sfx(Sound, MsgSfxType),
    /// Sets the voice to play.
    Voice(Voice),
    /// Sets the index of the default option in a `Select` command.
    Default(DefaultArgs),
    /// A line break.
    Newline,
    /// Functionally equivalent to `Newline` except it maps to a vertical tab character.
    NewlineVt,
    /// A printf format specifier.
    Format(VecText),
    /// Sets the font size (higher = larger, normal size is 22). Use 255 to reset.
    Size(u8),
    /// Sets the font color from a preset.
    Color(Color),
    /// Sets the font color to an arbitrary RGBA color.
    Rgba(u32),
    /// Sets the font layout.
    Layout(Layout),
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
    Text(VecText),
}

impl MsgCommand {
    /// Returns the opcode corresponding to the command if there is one. `Text` will return `None`.
    #[must_use]
    pub fn opcode(&self) -> Option<MsgOp> {
        Some(match self {
            MsgCommand::Speed(_) => MsgOp::Speed,
            MsgCommand::Wait(_) => MsgOp::Wait,
            MsgCommand::Anim(_) => MsgOp::Anim,
            MsgCommand::Sfx(_, _) => MsgOp::Sfx,
            MsgCommand::Voice(_) => MsgOp::Voice,
            MsgCommand::Default(_) => MsgOp::Default,
            MsgCommand::Newline => MsgOp::Newline,
            MsgCommand::NewlineVt => MsgOp::NewlineVt,
            MsgCommand::Format(_) => MsgOp::Format,
            MsgCommand::Size(_) => MsgOp::Size,
            MsgCommand::Color(_) => MsgOp::Color,
            MsgCommand::Rgba(_) => MsgOp::Rgba,
            MsgCommand::Layout(_) => MsgOp::Layout,
            MsgCommand::Icon(_) => MsgOp::Icon,
            MsgCommand::Shake(_) => MsgOp::Shake,
            MsgCommand::Center(_) => MsgOp::Center,
            MsgCommand::Rotate(_) => MsgOp::Rotate,
            MsgCommand::Scale(_, _) => MsgOp::Scale,
            MsgCommand::NumInput(_) => MsgOp::NumInput,
            MsgCommand::Question(_) => MsgOp::Question,
            MsgCommand::Stay => MsgOp::Stay,
            MsgCommand::Text(_) => return None,
        })
    }
}

/// Arguments to the msg() and select() commands.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct MsgArgs {
    /// The commands that make up the message.
    pub commands: Vec<MsgCommand>,
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

impl From<VecText> for MsgArgs {
    fn from(text: VecText) -> Self {
        Self { commands: vec![MsgCommand::Text(text)] }
    }
}

impl From<MsgCommand> for MsgArgs {
    fn from(command: MsgCommand) -> Self {
        Self { commands: vec![command] }
    }
}

impl<T: Into<Vec<MsgCommand>>> From<T> for MsgArgs {
    fn from(commands: T) -> Self {
        Self { commands: commands.into() }
    }
}

impl DeserializeEvent for MsgArgs {
    type Error = Error;
    fn deserialize(de: &mut dyn EventDeserializer) -> Result<Self> {
        // Commands are encoded as control characters in the text. Any non-special character is
        // displayed as text. Keep reading until we hit a null terminator (MSG_END).
        let mut commands = vec![];
        let mut text = vec![];
        de.begin_variadic_args()?;
        loop {
            let ch = de.deserialize_msg_char()?;

            // Aggregate consecutive characters into single text commands.
            if let MsgOp::Char(x) = ch {
                text.push(match x {
                    // '$' is an escape character for '"'
                    b'$' => b'"',
                    _ => x,
                });
                continue;
            }
            if !text.is_empty() {
                commands.push(MsgCommand::Text(Text::from_bytes(text.split_off(0))?));
            }

            commands.push(match ch {
                MsgOp::End => break,
                MsgOp::Newline => MsgCommand::Newline,
                MsgOp::NewlineVt => MsgCommand::NewlineVt,

                MsgOp::Speed => MsgCommand::Speed(de.deserialize_u8()?),
                MsgOp::Wait => MsgCommand::Wait(MsgWaitType::deserialize(de)?),
                MsgOp::Anim => MsgCommand::Anim(MsgAnimArgs::deserialize(de)?),
                MsgOp::Default => MsgCommand::Default(DefaultArgs::deserialize(de)?),
                MsgOp::Size => MsgCommand::Size(de.deserialize_u8()?),
                MsgOp::Rgba => MsgCommand::Rgba(de.deserialize_rgba()?),
                MsgOp::Shake => MsgCommand::Shake(ShakeArgs::deserialize(de)?),
                MsgOp::Center => MsgCommand::Center(de.deserialize_u8()? != 0),
                MsgOp::Rotate => MsgCommand::Rotate(de.deserialize_i16()?),
                MsgOp::Scale => MsgCommand::Scale(de.deserialize_i16()?, de.deserialize_i16()?),
                MsgOp::NumInput => MsgCommand::NumInput(NumInputArgs::deserialize(de)?),
                MsgOp::Question => MsgCommand::Question(QuestionArgs::deserialize(de)?),
                MsgOp::Stay => MsgCommand::Stay,

                MsgOp::Sfx => {
                    let sound = de.deserialize_u32()?.try_into()?;
                    MsgCommand::Sfx(sound, MsgSfxType::deserialize(de)?)
                }
                MsgOp::Voice => {
                    let v = de.deserialize_u8()?;
                    MsgCommand::Voice(v.try_into().map_err(|_| Error::UnrecognizedVoice(v))?)
                }
                MsgOp::Color => {
                    let c = de.deserialize_u8()?;
                    MsgCommand::Color(c.try_into().map_err(|_| Error::UnrecognizedColor(c))?)
                }
                MsgOp::Layout => {
                    let l = de.deserialize_u8()?;
                    MsgCommand::Layout(l.try_into().map_err(|_| Error::UnrecognizedLayout(l))?)
                }
                MsgOp::Icon => {
                    let i = de.deserialize_u8()?;
                    MsgCommand::Icon(i.try_into().map_err(|_| Error::UnrecognizedIcon(i))?)
                }

                MsgOp::Format => {
                    // Read characters up to the next format boundary.
                    let mut chars = vec![];
                    loop {
                        match de.deserialize_msg_char()? {
                            MsgOp::Format => break,
                            MsgOp::Char(ch) => chars.push(ch),
                            other => return Err(Error::Unsupported(other)),
                        }
                    }
                    MsgCommand::Format(Text::from_bytes(chars)?)
                }

                MsgOp::Invalid => return Err(Error::Unsupported(ch)),
                MsgOp::Char(_) => unreachable!(),
            });
        }
        de.end_variadic_args()?;
        Ok(Self { commands })
    }
}

impl SerializeEvent for MsgArgs {
    type Error = Error;
    fn serialize(&self, ser: &mut dyn EventSerializer) -> Result<()> {
        ser.begin_variadic_args(self.commands.len())?;
        let mut num_chars = 0;
        for command in &self.commands {
            if let Some(opcode) = command.opcode() {
                ser.serialize_msg_char(opcode)?;
            }
            match command {
                MsgCommand::Speed(a) => ser.serialize_u8(*a)?,
                MsgCommand::Wait(ty) => ty.serialize(ser)?,
                MsgCommand::Anim(arg) => arg.serialize(ser)?,
                MsgCommand::Sfx(id, ty) => {
                    ser.serialize_u32(id.value())?;
                    ty.serialize(ser)?;
                }
                MsgCommand::Voice(voice) => ser.serialize_u8((*voice).into())?,
                MsgCommand::Default(arg) => arg.serialize(ser)?,
                MsgCommand::Newline | MsgCommand::NewlineVt => (),
                MsgCommand::Format(text) => {
                    for b in text.iter() {
                        ser.serialize_msg_char(MsgOp::Char(b))?;
                    }
                    ser.serialize_msg_char(MsgOp::Format)?;
                    // Note: format strings aren't counted much towards the MAX_CHARS limit here
                    // because they can expand to any length. If you use a format string in a long
                    // message, you're on your own.
                    num_chars += 1;
                }
                MsgCommand::Size(size) => ser.serialize_u8(*size)?,
                MsgCommand::Color(color) => ser.serialize_u8((*color).into())?,
                MsgCommand::Rgba(rgba) => ser.serialize_rgba(*rgba)?,
                MsgCommand::Layout(layout) => ser.serialize_u8((*layout).into())?,
                MsgCommand::Icon(icon) => ser.serialize_u8((*icon).into())?,
                MsgCommand::Shake(arg) => arg.serialize(ser)?,
                MsgCommand::Center(x) => ser.serialize_u8(*x as u8)?,
                MsgCommand::Rotate(angle) => ser.serialize_i16(*angle)?,
                MsgCommand::Scale(x, y) => {
                    ser.serialize_i16(*x)?;
                    ser.serialize_i16(*y)?;
                }
                MsgCommand::NumInput(arg) => arg.serialize(ser)?,
                MsgCommand::Question(arg) => arg.serialize(ser)?,
                MsgCommand::Stay => (),
                MsgCommand::Text(text) => {
                    for b in text.iter() {
                        let escaped = match b {
                            // '$' is an escape character for '"'
                            b'"' => b'$',
                            _ => b,
                        };
                        ser.serialize_msg_char(MsgOp::Char(escaped))?;
                        // For the purposes of enforcing the MAX_CHARS limit, assume each byte is
                        // one character because the NTSC-U version will always interpret text as
                        // Windows-1252. Supporting other regions may require changing this.
                        num_chars += 1;
                    }
                }
            }
        }
        if num_chars > MAX_CHARS {
            return Err(Error::TooManyChars { len: num_chars, max: MAX_CHARS });
        }
        ser.serialize_msg_char(MsgOp::End)?;
        Ok(ser.end_variadic_args()?)
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

impl DeserializeEvent for MsgWaitType {
    type Error = SerError;
    fn deserialize(de: &mut dyn EventDeserializer) -> SerResult<Self> {
        Ok(match de.deserialize_u8()? {
            MSG_WAIT_ATC_MENU => Self::AtcMenu,
            MSG_WAIT_SUIT_MENU => Self::SuitMenu,
            MSG_WAIT_LEFT_PLUG => Self::LeftPlug,
            MSG_WAIT_RIGHT_PLUG => Self::RightPlug,
            x => Self::Time(x),
        })
    }
}

impl SerializeEvent for MsgWaitType {
    type Error = SerError;
    fn serialize(&self, ser: &mut dyn EventSerializer) -> SerResult<()> {
        match self {
            Self::Time(x) => ser.serialize_u8(*x),
            Self::AtcMenu => ser.serialize_u8(MSG_WAIT_ATC_MENU),
            Self::SuitMenu => ser.serialize_u8(MSG_WAIT_SUIT_MENU),
            Self::LeftPlug => ser.serialize_u8(MSG_WAIT_LEFT_PLUG),
            Self::RightPlug => ser.serialize_u8(MSG_WAIT_RIGHT_PLUG),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, SerializeEvent, DeserializeEvent)]
pub struct MsgAnimArgs {
    pub flags: u8,
    pub obj: i16,
    pub anim: i32,
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

impl DeserializeEvent for MsgSfxType {
    type Error = Error;
    fn deserialize(de: &mut dyn EventDeserializer) -> Result<Self> {
        let ty = de.deserialize_i8()?;
        Ok(match ty as i32 {
            SFX_WAIT => Self::Wait,
            SFX_STOP => Self::Stop,
            SFX_PLAY => Self::Play,
            SFX_FADE_OUT => Self::FadeOut(de.deserialize_u16()?),
            SFX_FADE_IN => Self::FadeIn(de.deserialize_u16()?),
            SFX_FADE => Self::Fade(MsgSfxFadeArgs::deserialize(de)?),
            SFX_UNK_5 => Self::Unk5,
            SFX_UNK_6 => Self::Unk6,
            _ => return Err(Error::UnrecognizedSfx(ty)),
        })
    }
}

impl SerializeEvent for MsgSfxType {
    type Error = Error;
    fn serialize(&self, ser: &mut dyn EventSerializer) -> Result<()> {
        match self {
            Self::Wait => ser.serialize_i8(SFX_WAIT as i8)?,
            Self::Stop => ser.serialize_i8(SFX_STOP as i8)?,
            Self::Play => ser.serialize_i8(SFX_PLAY as i8)?,
            Self::FadeOut(duration) => {
                ser.serialize_i8(SFX_FADE_OUT as i8)?;
                ser.serialize_u16(*duration)?;
            }
            Self::FadeIn(duration) => {
                ser.serialize_i8(SFX_FADE_IN as i8)?;
                ser.serialize_u16(*duration)?;
            }
            Self::Fade(arg) => {
                ser.serialize_i8(SFX_FADE as i8)?;
                arg.serialize(ser)?;
            }
            Self::Unk5 => ser.serialize_i8(SFX_UNK_5 as i8)?,
            Self::Unk6 => ser.serialize_i8(SFX_UNK_6 as i8)?,
        }
        Ok(())
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, SerializeEvent, DeserializeEvent)]
pub struct MsgSfxFadeArgs {
    /// The fade duration.
    pub duration: u16,
    /// The target volume (0 = silent, 255 = max).
    pub volume: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, SerializeEvent, DeserializeEvent)]
pub struct DefaultArgs {
    /// Flags which control the meaning of `index`.
    pub flags: DefaultFlags,
    /// The default item index starting from 0 (or a variable index if the `VARIABLE` flag is set).
    pub index: i32,
}

bitflags! {
    pub struct DefaultFlags: u8 {
        /// The `index` is a variable index rather than a constant.
        const VARIABLE = 0x1;
    }
}

impl SerializeEvent for DefaultFlags {
    type Error = SerError;
    fn serialize(&self, ser: &mut dyn EventSerializer) -> SerResult<()> {
        ser.serialize_u8(self.bits())
    }
}

impl DeserializeEvent for DefaultFlags {
    type Error = SerError;
    fn deserialize(de: &mut dyn EventDeserializer) -> SerResult<Self> {
        Ok(Self::from_bits_truncate(de.deserialize_u8()?))
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

/// Text layouts.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum Layout {
    /// Characters are spaced apart evenly.
    Monospace = 0,
    /// Characters are spaced apart proportionally.
    #[default]
    Default = 1,
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, SerializeEvent, DeserializeEvent)]
pub struct ShakeArgs {
    /// Flags which describe how to shake characters. Either `WAVE` or `JITTER` must be set.
    pub flags: ShakeFlags,
    /// Shake strength (higher = stronger).
    pub strength: u8,
    /// Shake speed (higher = slower).
    pub speed: u8,
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

impl SerializeEvent for ShakeFlags {
    type Error = SerError;
    fn serialize(&self, ser: &mut dyn EventSerializer) -> SerResult<()> {
        ser.serialize_u8(self.bits())
    }
}

impl DeserializeEvent for ShakeFlags {
    type Error = SerError;
    fn deserialize(de: &mut dyn EventDeserializer) -> SerResult<Self> {
        Ok(Self::from_bits_truncate(de.deserialize_u8()?))
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, SerializeEvent, DeserializeEvent)]
pub struct NumInputArgs {
    /// The number of digits to enter.
    pub digits: u8,
    /// The number of digits which are editable (from the left).
    pub editable: u8,
    /// The initially-selected digit (1 = first digit on the right).
    pub selected: u8,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, SerializeEvent, DeserializeEvent)]
pub struct QuestionArgs {
    /// Flags indicating which items (if any) are "no".
    pub flags: QuestionFlags,
    /// The initially-selected item (0 = left, 1 = right).
    pub default: u8,
}

bitflags! {
    pub struct QuestionFlags: u8 {
        /// The left option is "no".
        const LEFT_NO = 0x1;
        /// The right option is "no".
        const RIGHT_NO = 0x2;
    }
}

impl SerializeEvent for QuestionFlags {
    type Error = SerError;
    fn serialize(&self, ser: &mut dyn EventSerializer) -> SerResult<()> {
        ser.serialize_u8(self.bits())
    }
}

impl DeserializeEvent for QuestionFlags {
    type Error = SerError;
    fn deserialize(de: &mut dyn EventDeserializer) -> SerResult<Self> {
        Ok(Self::from_bits_truncate(de.deserialize_u8()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::assert_reserialize;
    use crate::data::Music;
    use crate::event::bin::BinSerializer;
    use crate::event::opcodes::CmdOp;
    use std::io::Cursor;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct MsgWrapper(MsgArgs);

    impl SerializeEvent for MsgWrapper {
        type Error = Error;
        fn serialize(&self, ser: &mut dyn EventSerializer) -> Result<()> {
            ser.begin_command(CmdOp::Msg)?;
            self.0.serialize(ser)?;
            ser.end_command()?;
            Ok(())
        }
    }

    impl DeserializeEvent for MsgWrapper {
        type Error = Error;
        fn deserialize(de: &mut dyn EventDeserializer) -> Result<Self> {
            assert_eq!(de.begin_command()?, CmdOp::Msg);
            let args = MsgArgs::deserialize(de)?;
            de.end_command()?;
            Ok(Self(args))
        }
    }

    fn msg(command: MsgCommand) -> MsgWrapper {
        MsgWrapper(vec![command].into())
    }

    fn text(string: &str) -> VecText {
        VecText::encode(string).unwrap()
    }

    #[test]
    fn test_reserialize_msg() {
        let sound = Sound::Music(Music::Bgm);
        assert_reserialize!(msg(MsgCommand::Speed(1)));
        assert_reserialize!(msg(MsgCommand::Wait(MsgWaitType::Time(1))));
        assert_reserialize!(msg(MsgCommand::Wait(MsgWaitType::AtcMenu)));
        assert_reserialize!(msg(MsgCommand::Wait(MsgWaitType::SuitMenu)));
        assert_reserialize!(msg(MsgCommand::Wait(MsgWaitType::LeftPlug)));
        assert_reserialize!(msg(MsgCommand::Wait(MsgWaitType::RightPlug)));
        assert_reserialize!(msg(MsgCommand::Anim(MsgAnimArgs { flags: 1, obj: 2, anim: 3 })));
        assert_reserialize!(msg(MsgCommand::Sfx(sound, MsgSfxType::Wait)));
        assert_reserialize!(msg(MsgCommand::Sfx(sound, MsgSfxType::Stop)));
        assert_reserialize!(msg(MsgCommand::Sfx(sound, MsgSfxType::Play)));
        assert_reserialize!(msg(MsgCommand::Sfx(sound, MsgSfxType::FadeOut(2))));
        assert_reserialize!(msg(MsgCommand::Sfx(sound, MsgSfxType::FadeIn(2))));
        assert_reserialize!(msg(MsgCommand::Sfx(
            sound,
            MsgSfxType::Fade(MsgSfxFadeArgs { duration: 1, volume: 2 })
        )));
        assert_reserialize!(msg(MsgCommand::Sfx(sound, MsgSfxType::Unk5)));
        assert_reserialize!(msg(MsgCommand::Sfx(sound, MsgSfxType::Unk6)));
        assert_reserialize!(msg(MsgCommand::Voice(Voice::Gebah)));
        assert_reserialize!(msg(MsgCommand::Default(DefaultArgs {
            flags: DefaultFlags::VARIABLE,
            index: 1,
        })));
        assert_reserialize!(msg(MsgCommand::Newline));
        assert_reserialize!(msg(MsgCommand::NewlineVt));
        assert_reserialize!(msg(MsgCommand::Format(text("%s"))));
        assert_reserialize!(msg(MsgCommand::Size(1)));
        assert_reserialize!(msg(MsgCommand::Color(Color::White)));
        assert_reserialize!(msg(MsgCommand::Rgba(1)));
        assert_reserialize!(msg(MsgCommand::Layout(Layout::Monospace)));
        assert_reserialize!(msg(MsgCommand::Layout(Layout::Default)));
        assert_reserialize!(msg(MsgCommand::Icon(Icon::Moolah)));
        assert_reserialize!(msg(MsgCommand::Shake(ShakeArgs {
            flags: ShakeFlags::X | ShakeFlags::JITTER,
            strength: 1,
            speed: 2
        })));
        assert_reserialize!(msg(MsgCommand::Center(false)));
        assert_reserialize!(msg(MsgCommand::Center(true)));
        assert_reserialize!(msg(MsgCommand::Rotate(1)));
        assert_reserialize!(msg(MsgCommand::Scale(1, 2)));
        assert_reserialize!(msg(MsgCommand::NumInput(NumInputArgs {
            digits: 1,
            editable: 2,
            selected: 3,
        })));
        assert_reserialize!(msg(MsgCommand::Question(QuestionArgs {
            flags: QuestionFlags::RIGHT_NO,
            default: 1,
        })));
        assert_reserialize!(msg(MsgCommand::Stay));
        assert_reserialize!(msg(MsgCommand::Text(text("bunger"))));
        assert_reserialize!(msg(MsgCommand::Text(text("\"quoted\""))));
        assert_reserialize!(msg(MsgCommand::Text(text("スプラトゥーン"))));
    }

    #[test]
    fn test_reserialize_msg_multiple_commands() {
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
        assert_reserialize!(MsgWrapper(msg));
    }

    #[test]
    fn test_reserialize_none_voice() {
        assert_reserialize!(msg(MsgCommand::Voice(Voice::None)));
    }

    #[test]
    fn test_write_invalid_char() {
        let ch = msg(MsgCommand::Text(text("\x01")));
        let mut ser = BinSerializer::new(Cursor::new(vec![]));
        match ch.serialize(&mut ser) {
            Err(Error::Serialize(e)) => assert!(matches!(*e, SerError::InvalidMsgChar(1))),
            _ => panic!("not a serialization error"),
        };
    }
}
