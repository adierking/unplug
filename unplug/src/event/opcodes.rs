mod ggte;

pub use ggte::Ggte;

use std::fmt::Debug;
use std::hash::Hash;

/// Base trait for an opcode enum.
///
/// Opcodes must provide a default "invalid" value which can be used as a placeholder.
pub trait Opcode: Copy + Default + Eq + Ord + Hash + Debug + Sized {
    /// The type of a raw opcode value.
    type Value: Copy + Eq + Hash;

    /// Maps an unrecognized value to an opcode.
    fn map_unrecognized(value: Self::Value) -> Result<Self, Self::Value>;

    /// Maps an unsupported opcode to a value.
    fn map_unsupported(opcode: Self) -> Result<Self::Value, Self>;
}

/// Maps opcodes to and from raw values.
pub trait OpcodeMap<T: Opcode> {
    /// Gets the opcode corresponding to a value.
    fn get(value: T::Value) -> Result<T, T::Value>;
    /// Gets the value corresponding to an opcode.
    fn value(opcode: T) -> Result<T::Value, T>;
}

/// Declares a struct which implements `OpcodeMap` for sets of opcodes.
///
/// # Example
///
/// ```
/// # use unplug::event::opcodes::{ExprOp, CmdOp, OpcodeMap};
/// # use unplug::opcodes;
/// opcodes! {
///     pub struct Opcodes;
///
///     ExprOp {
///         Equal = 0,
///         NotEqual = 1,
///         // ...
///     }
///
///     CmdOp {
///         Abort = 1,
///         Return = 2,
///         // ...
///     }
/// }
///
/// assert_eq!(Opcodes::value(ExprOp::NotEqual).unwrap(), 1);
/// assert_eq!(Opcodes::value(CmdOp::Return).unwrap(), 2);
/// ```
#[macro_export]
macro_rules! opcodes {
    {
        $(#[$meta:meta])*
        $vis:vis struct $struct:ident;
        $($type:ident {
            $($name:ident = $value:literal),* $(,)*
        })*
    } => {
        $(#[$meta])*
        $vis struct $struct;
        $(impl $crate::event::opcodes::OpcodeMap<$type> for $struct {
            fn get(
                value: <$type as $crate::event::opcodes::Opcode>::Value
            ) -> ::std::result::Result<$type, <$type as $crate::event::opcodes::Opcode>::Value> {
                ::std::result::Result::Ok(match value {
                    $($value => $type::$name,)*
                    _ => return <$type as $crate::event::opcodes::Opcode>::map_unrecognized(value),
                })
            }

            #[allow(unreachable_patterns)]
            fn value(
                opcode: $type
            ) -> ::std::result::Result<<$type as $crate::event::opcodes::Opcode>::Value, $type> {
                ::std::result::Result::Ok(match opcode {
                    $($type::$name => $value,)*
                    _ => return <$type as $crate::event::opcodes::Opcode>::map_unsupported(opcode),
                })
            }
        })*
    };
}

/// An expression opcode. Refer to `Expr` for documentation.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExprOp {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Not,
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    BitAnd,
    BitOr,
    BitXor,
    AddAssign,
    SubtractAssign,
    MultiplyAssign,
    DivideAssign,
    ModuloAssign,
    BitAndAssign,
    BitOrAssign,
    BitXorAssign,
    Imm16,
    Imm32,
    AddressOf,
    Stack,
    ParentStack,
    Flag,
    Variable,
    Result1,
    Result2,
    Pad,
    Battery,
    Money,
    Item,
    Atc,
    Rank,
    Exp,
    Level,
    Hold,
    Map,
    ActorName,
    ItemName,
    Time,
    CurrentSuit,
    Scrap,
    CurrentAtc,
    Use,
    Hit,
    StickerName,
    Obj,
    Random,
    Sin,
    Cos,
    ArrayElement,
    /// Not an actual opcode and cannot be serialized
    #[default]
    Invalid,
}

impl Opcode for ExprOp {
    type Value = u8;
    fn map_unrecognized(value: Self::Value) -> Result<Self, Self::Value> {
        Err(value)
    }
    fn map_unsupported(opcode: Self) -> Result<Self::Value, Self> {
        Err(opcode)
    }
}

/// A command opcode. Refer to `Command` for documentation.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CmdOp {
    Abort,
    Return,
    Goto,
    Set,
    If,
    Elif,
    EndIf,
    Case,
    Expr,
    While,
    Break,
    Run,
    Lib,
    PushBp,
    PopBp,
    SetSp,
    Anim,
    Anim1,
    Anim2,
    Attach,
    Born,
    Call,
    Camera,
    Check,
    Color,
    Detach,
    Dir,
    MDir,
    Disp,
    Kill,
    Light,
    Menu,
    Move,
    MoveTo,
    Msg,
    Pos,
    PrintF,
    Ptcl,
    Read,
    Scale,
    MScale,
    Scrn,
    Select,
    Sfx,
    Timer,
    Wait,
    Warp,
    Win,
    Movie,
    /// Not an actual opcode and cannot be serialized
    #[default]
    Invalid,
}

impl CmdOp {
    /// Returns `true` if the command is an `if` statement.
    #[must_use]
    pub fn is_if(self) -> bool {
        matches!(self, Self::If | Self::Elif | Self::Case | Self::Expr | Self::While)
    }

    /// Returns `true` if the command always jumps to another offset.
    #[must_use]
    pub fn is_goto(self) -> bool {
        matches!(self, Self::Break | Self::EndIf | Self::Goto)
    }

    /// Returns `true` if the command may jump to another offset or end the event. Function calls are
    /// not included.
    #[must_use]
    pub fn is_control_flow(self) -> bool {
        matches!(
            self,
            Self::Abort
                | Self::Return
                | Self::Goto
                | Self::If
                | Self::Elif
                | Self::EndIf
                | Self::Case
                | Self::Expr
                | Self::While
                | Self::Break
        )
    }
}

impl Opcode for CmdOp {
    type Value = u8;
    fn map_unrecognized(value: Self::Value) -> Result<Self, Self::Value> {
        Err(value)
    }
    fn map_unsupported(opcode: Self) -> Result<Self::Value, Self> {
        Err(opcode)
    }
}

/// Fixed integers representing abstract strings known to the engine.
///
/// The actual string characters do not exist anywhere, and they don't need to, because atoms are
/// only ever compared for equality.
///
/// Atoms are often used to create subcommands which share similar meanings with each
/// other. For example, `@fade` passed to the `scrn` command will fade the screen, and `@fade`
/// passed to the `wait` command will wait for the fade to complete.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Atom {
    Time,
    Fade,
    Wipe,
    Unk203,
    Anim,
    Dir,
    Move,
    Pos,
    Obj,
    Reset,
    Unk210,
    Unk211,
    PosX,
    PosY,
    PosZ,
    BoneX,
    BoneY,
    BoneZ,
    DirTo,
    Color,
    Lead,
    Sfx,
    Modulate,
    Blend,
    Real,
    Cam,
    Hud,
    Unk227,
    Distance,
    Unk229,
    Unk230,
    Unk231,
    Unk232,
    Read,
    ZBlur,
    Unk235,
    Unk236,
    Unk237,
    Unk238,
    Letterbox,
    Unk240,
    Shake,
    Mono,
    Unk243,
    Scale,
    Cue,
    Unk246,
    Unk247,
    Unk248,
    Unk249,
    Unk250,
    Unk251,
    Unk252,
    /// Not an actual opcode and cannot be serialized
    #[default]
    Invalid,
}

impl Opcode for Atom {
    type Value = i32;
    fn map_unrecognized(value: Self::Value) -> Result<Self, Self::Value> {
        Err(value)
    }
    fn map_unsupported(opcode: Self) -> Result<Self::Value, Self> {
        Err(opcode)
    }
}

/// A message command opcode. Refer to `MsgCommand` for documentation.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MsgOp {
    End,
    Speed,
    Wait,
    Anim,
    Sfx,
    Voice,
    Default,
    Newline,
    NewlineVt,
    Format,
    Size,
    Color,
    Rgba,
    Layout,
    Icon,
    Shake,
    Center,
    Rotate,
    Scale,
    NumInput,
    Question,
    Stay,
    Char(u8),
    /// Not an actual opcode and cannot be serialized
    #[default]
    Invalid,
}

/// Maximum value of a message command opcode.
const MSG_OPCODE_MAX: u8 = 24;

impl Opcode for MsgOp {
    type Value = u8;
    fn map_unrecognized(value: Self::Value) -> Result<Self, Self::Value> {
        // Bell, backspace, and tab are interpreted as characters
        if value > MSG_OPCODE_MAX || value == b'\x07' || value == b'\x08' || value == b'\t' {
            Ok(Self::Char(value))
        } else {
            Err(value)
        }
    }
    fn map_unsupported(opcode: Self) -> Result<Self::Value, Self> {
        match opcode {
            Self::Char(ch) => Ok(ch),
            _ => Err(opcode),
        }
    }
}
