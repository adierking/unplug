use super::block::{Ip, WriteIp};
use super::opcodes::{ExprOp, Ggte, OpcodeMap, TypeOp};
use crate::common::{ReadFrom, WriteTo};
use crate::data::{Atc, Item, Music, Object, Sfx, Sound};
use byteorder::{ReadBytesExt, WriteBytesExt, LE};
use std::convert::{TryFrom, TryInto};
use std::fmt::{self, Debug};
use std::io::{self, Read, Write};
use thiserror::Error;
use unplug_proc::{ReadFrom, WriteTo};

/// The result type for expression operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for expression operations.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("unrecognized expression opcode: {0}")]
    UnrecognizedOp(u8),

    #[error("unsupported expression: {0:?}")]
    NotSupported(ExprOp),

    #[error("unrecognized type code: {0}")]
    UnrecognizedType(i32),

    #[error("unsupported type: {0:?}")]
    UnsupportedType(TypeOp),

    #[error("expression is not a constant: {0:?}")]
    NonConstant(Box<Expr>),

    #[error("expression is not a valid ATC ID: {0:?}")]
    InvalidAtc(Box<Expr>),

    #[error("expression is not a valid item ID: {0:?}")]
    InvalidItem(Box<Expr>),

    #[error("expression is not a valid object ID: {0:?}")]
    InvalidObject(Box<Expr>),

    #[error(transparent)]
    Io(Box<io::Error>),
}

from_error_boxed!(Error::Io, io::Error);

/// An expression which evaluates to a 32-bit signed integer.
#[derive(Clone, PartialEq, Eq)]
pub enum Expr {
    /// Evaluates to 1 if the two operands are equal.
    Equal(Box<BinaryOp>),
    /// Evaluates to 1 if the two operands are not equal.
    NotEqual(Box<BinaryOp>),
    /// Evaluates to 1 if the left-hand operand is less than the right-hand operand.
    Less(Box<BinaryOp>),
    /// Evaluates to 1 if the left-hand operand is less than or equal to the right-hand operand.
    LessEqual(Box<BinaryOp>),
    /// Evaluates to 1 if the left-hand operand is greater than the right-hand operand.
    Greater(Box<BinaryOp>),
    /// Evaluates to 1 if the left-hand operand is greater than or equal to the right-hand operand.
    GreaterEqual(Box<BinaryOp>),
    /// Evaluates to 1 if the operand is false.
    Not(Box<Self>),
    /// Adds the two operands.
    Add(Box<BinaryOp>),
    /// Subtracts the right-hand operand from the left-hand operand.
    Subtract(Box<BinaryOp>),
    /// Multiplies the two operands.
    Multiply(Box<BinaryOp>),
    /// Divides the left-hand operand by the right-hand operand.
    Divide(Box<BinaryOp>),
    /// Evaluates to the left-hand operand modulo the right-hand operand.
    Modulo(Box<BinaryOp>),
    /// Bitwise ANDs the two operands.
    BitAnd(Box<BinaryOp>),
    /// Bitwise ORs the two operands.
    BitOr(Box<BinaryOp>),
    /// Bitwise XORs the two operands.
    BitXor(Box<BinaryOp>),
    /// `set()` only: `lhs = lhs + rhs`
    AddAssign(Box<BinaryOp>),
    /// `set()` only: `lhs = lhs - rhs`
    SubtractAssign(Box<BinaryOp>),
    /// `set()` only: `lhs = lhs * rhs`
    MultiplyAssign(Box<BinaryOp>),
    /// `set()` only: `lhs = lhs / rhs`
    DivideAssign(Box<BinaryOp>),
    /// `set()` only: `lhs = lhs % rhs`
    ModuloAssign(Box<BinaryOp>),
    /// `set()` only: `lhs = lhs & rhs`
    BitAndAssign(Box<BinaryOp>),
    /// `set()` only: `lhs = lhs | rhs`
    BitOrAssign(Box<BinaryOp>),
    /// `set()` only: `lhs = lhs ^ rhs`
    BitXorAssign(Box<BinaryOp>),
    /// An immediate 16-bit signed integer.
    Imm16(i16),
    /// An immediate 32-bit signed integer.
    Imm32(i32),
    /// Evaluates to the memory address corresponding to an offset in the stage file.
    AddressOf(Ip),
    /// Retrieves a value from the current stack frame (note that the stack grows upwards).
    /// Typically used to retrieve subroutine arguments.
    Stack(u8),
    /// Retrieves a value from the parent stack frame (note that the stack grows upwards).
    /// Typically used to retrieve subroutine arguments while another stack frame is built.
    ParentStack(u8),
    /// Retrieves the value of a flag.
    /// The index must be in the range [-3, 4096).
    Flag(Box<Self>),
    /// Retrieves the value of a variable.
    /// The index must be in the range [0, 2048).
    Variable(Box<Self>),
    /// Retrieves the primary variable that commands can use to communicate results.
    Result1,
    /// Retrieves the secondary variable that commands can use to communicate results.
    Result2,
    /// Retrieves the current gamepad state:
    /// * 0: Buttons down
    /// * 1: Buttons pressed
    /// * 2: Control stick X: -100 (left) to 100 (right)
    /// * 3: Control stick Y: -100 (down) to 100 (up)
    /// * 4: Control stick angle in degrees: up = 0, left = 90, down = 180, right = 270
    /// * 5: Control stick magnitude (0 - 100)
    /// * 6: Control stick angle relative to the camera
    /// * 7: Unknown
    ///
    /// Button flags:
    /// * 1 = left, 2 = right, 4 = down, 8 = up
    /// * 16 = Z, 32 = R, 64 = L
    /// * 256 = A, 512 = B, 1024 = X, 2048 = Y
    /// * 4096 = Start
    Pad(Box<Self>),
    /// Retrieves the player's battery level in hundredths of watts (e.g. 8000 = 80.0).
    /// An operand of 0 returns the current battery level and 1 returns the maximum.
    Battery(Box<Self>),
    /// Retrieves the player's money count.
    Money,
    /// Retrieves an item's inventory count.
    /// The index must be in the range [0, 159).
    Item(Box<Self>),
    /// Retrieves whether an attachment (ATC) is unlocked.
    /// The index must be in the range [0, 9).
    Atc(Box<Self>),
    /// Retrieves the player's chibi-ranking.
    Rank,
    /// Retrieves the player's happy point total.
    Exp,
    /// Retrieves the player's upgrade level. (14 = Super Chibi-Robo)
    Level,
    /// Retrieves the ID of the item that the player is currently holding, or -1 if nothing.
    /// Note that this maps the player's plug to ID 1000.
    Hold,
    /// Retrieves the ID of the current (0) or previous (1) map.
    Map(Box<Self>),
    /// Evaluates to the address of an actor's display name (e.g. "The Great Peekoe").
    ActorName(Box<Self>),
    /// Evaluates to the address of an item's display name (e.g. "Frog Ring").
    ItemName(Box<Self>),
    /// Retrieves various information related to the in-game time:
    /// * 0: the current time of day (0 = day, 1 = night)
    /// * 1: the current time as a percentage counting upwards from 0 to 100
    /// * 2: the time rate (0 = paused, 67 = 15 minutes, 100 = 10 minutes, 200 = 5 minutes)
    Time(Box<Self>),
    /// Retrieves the ID of the suit that the player is wearing.
    CurrentSuit,
    /// Retrieves the player's scrap count.
    Scrap,
    /// Retrieves the ID of the attachment that the player has equipped.
    CurrentAtc,
    /// Retrieves the ID of the item (e.g. toothbrush) that was used to trigger the event.
    Use,
    /// Retrieves the ID of the projectile (e.g. water) that triggered the event.
    Hit,
    /// Evaluates to the address of a sticker's display name (e.g. "Cooking").
    StickerName(Box<Self>),
    /// Retrieves various object-related info (e.g. object direction).
    Obj(Box<ObjExpr>),
    /// Generates a random 15-bit number between 0 and the operand (inclusive).
    Random(Box<Self>),
    /// Calculates `sin(x)`.
    /// Input units are hundredths of degrees (e.g. 9000 = 90 deg).
    /// Output units are hundredths (e.g. `sin(9000)` = 100).
    Sin(Box<Self>),
    /// Calculates `cos(x)`.
    /// Input units are hundredths of degrees (e.g. 9000 = 90 deg).
    /// Output units are hundredths (e.g. `cos(0)` = 100).
    Cos(Box<Self>),
    /// Reads a value from an array.
    ArrayElement(Box<ArrayElementExpr>),
}

impl Expr {
    /// Constructs a new `Expr` from a variable index.
    #[must_use]
    pub fn from_var(index: i32) -> Self {
        Self::Variable(Self::Imm32(index).into())
    }

    /// Constructs a new `Expr` from a flag index.
    #[must_use]
    pub fn from_flag(index: i32) -> Self {
        Self::Flag(Self::Imm32(index).into())
    }

    /// Return's the expression's constant value if it has one.
    #[must_use]
    pub fn value(&self) -> Option<i32> {
        match self {
            Self::Imm16(x) => Some(*x as i32),
            Self::Imm32(x) => Some(*x),
            _ => None,
        }
    }

    /// Returns `true` if the expression accepts two operands.
    #[must_use]
    pub fn is_binary_op(&self) -> bool {
        self.binary_op().is_some()
    }

    /// If the expression accepts two operands, gets a reference to the `BinaryOp`.
    #[must_use]
    pub fn binary_op(&self) -> Option<&BinaryOp> {
        match self {
            Self::Equal(op)
            | Self::NotEqual(op)
            | Self::Less(op)
            | Self::LessEqual(op)
            | Self::Greater(op)
            | Self::GreaterEqual(op)
            | Self::Add(op)
            | Self::Subtract(op)
            | Self::Multiply(op)
            | Self::Divide(op)
            | Self::Modulo(op)
            | Self::BitAnd(op)
            | Self::BitOr(op)
            | Self::BitXor(op)
            | Self::AddAssign(op)
            | Self::SubtractAssign(op)
            | Self::MultiplyAssign(op)
            | Self::DivideAssign(op)
            | Self::ModuloAssign(op)
            | Self::BitAndAssign(op)
            | Self::BitOrAssign(op)
            | Self::BitXorAssign(op) => Some(op),
            _ => None,
        }
    }

    /// If the expression accepts two operands, gets a reference to the left-hand operand.
    #[must_use]
    pub fn lhs(&self) -> Option<&Self> {
        self.binary_op().map(|op| &op.lhs)
    }

    /// If the expression accepts two operands, gets a reference to the left-hand operand.
    #[must_use]
    pub fn rhs(&self) -> Option<&Self> {
        self.binary_op().map(|op| &op.rhs)
    }

    /// Returns `true` if the expression is an in-place assignment (e.g. `+=`).
    #[must_use]
    pub fn is_assign(&self) -> bool {
        matches!(
            self,
            Self::AddAssign(_)
                | Self::SubtractAssign(_)
                | Self::MultiplyAssign(_)
                | Self::DivideAssign(_)
                | Self::ModuloAssign(_)
                | Self::BitAndAssign(_)
                | Self::BitOrAssign(_)
                | Self::BitXorAssign(_)
        )
    }

    /// Creates an expression from an immediate value, selecting the smallest size that fits.
    #[must_use]
    #[inline]
    pub fn imm(val: i32) -> Self {
        match i16::try_from(val) {
            Ok(val16) => Self::Imm16(val16),
            Err(_) => Self::Imm32(val),
        }
    }

    /// Consumes the expression and returns its logical negation (e.g. `a == b` becomes `a != b`).
    #[must_use]
    pub fn negate(self) -> Self {
        match self {
            Self::Equal(op) => Self::NotEqual(op),
            Self::NotEqual(op) => Self::Equal(op),
            Self::Less(op) => Self::GreaterEqual(op),
            Self::LessEqual(op) => Self::Greater(op),
            Self::Greater(op) => Self::LessEqual(op),
            Self::GreaterEqual(op) => Self::Less(op),

            // We assume here that AND and OR are used for logical purposes
            Self::BitAnd(op) => Self::BitOr(BinaryOp::new(op.lhs.negate(), op.rhs.negate()).into()),
            Self::BitOr(op) => Self::BitAnd(BinaryOp::new(op.lhs.negate(), op.rhs.negate()).into()),

            Self::Not(op) => *op,
            expr => Self::Not(expr.into()),
        }
    }

    /// Returns the opcode corresponding to the expression.
    #[must_use]
    pub fn opcode(&self) -> ExprOp {
        match self {
            Self::Equal(_) => ExprOp::Equal,
            Self::NotEqual(_) => ExprOp::NotEqual,
            Self::Less(_) => ExprOp::Less,
            Self::LessEqual(_) => ExprOp::LessEqual,
            Self::Greater(_) => ExprOp::Greater,
            Self::GreaterEqual(_) => ExprOp::GreaterEqual,
            Self::Not(_) => ExprOp::Not,
            Self::Add(_) => ExprOp::Add,
            Self::Subtract(_) => ExprOp::Subtract,
            Self::Multiply(_) => ExprOp::Multiply,
            Self::Divide(_) => ExprOp::Divide,
            Self::Modulo(_) => ExprOp::Modulo,
            Self::BitAnd(_) => ExprOp::BitAnd,
            Self::BitOr(_) => ExprOp::BitOr,
            Self::BitXor(_) => ExprOp::BitXor,
            Self::AddAssign(_) => ExprOp::AddAssign,
            Self::SubtractAssign(_) => ExprOp::SubtractAssign,
            Self::MultiplyAssign(_) => ExprOp::MultiplyAssign,
            Self::DivideAssign(_) => ExprOp::DivideAssign,
            Self::ModuloAssign(_) => ExprOp::ModuloAssign,
            Self::BitAndAssign(_) => ExprOp::BitAndAssign,
            Self::BitOrAssign(_) => ExprOp::BitOrAssign,
            Self::BitXorAssign(_) => ExprOp::BitXorAssign,
            Self::Imm16(_) => ExprOp::Imm16,
            Self::Imm32(_) => ExprOp::Imm32,
            Self::AddressOf(_) => ExprOp::AddressOf,
            Self::Stack(_) => ExprOp::Stack,
            Self::ParentStack(_) => ExprOp::ParentStack,
            Self::Flag(_) => ExprOp::Flag,
            Self::Variable(_) => ExprOp::Variable,
            Self::Result1 => ExprOp::Result1,
            Self::Result2 => ExprOp::Result2,
            Self::Pad(_) => ExprOp::Pad,
            Self::Battery(_) => ExprOp::Battery,
            Self::Money => ExprOp::Money,
            Self::Item(_) => ExprOp::Item,
            Self::Atc(_) => ExprOp::Atc,
            Self::Rank => ExprOp::Rank,
            Self::Exp => ExprOp::Exp,
            Self::Level => ExprOp::Level,
            Self::Hold => ExprOp::Hold,
            Self::Map(_) => ExprOp::Map,
            Self::ActorName(_) => ExprOp::ActorName,
            Self::ItemName(_) => ExprOp::ItemName,
            Self::Time(_) => ExprOp::Time,
            Self::CurrentSuit => ExprOp::CurrentSuit,
            Self::Scrap => ExprOp::Scrap,
            Self::CurrentAtc => ExprOp::CurrentAtc,
            Self::Use => ExprOp::Use,
            Self::Hit => ExprOp::Hit,
            Self::StickerName(_) => ExprOp::StickerName,
            Self::Obj(_) => ExprOp::Obj,
            Self::Random(_) => ExprOp::Random,
            Self::Sin(_) => ExprOp::Sin,
            Self::Cos(_) => ExprOp::Cos,
            Self::ArrayElement(_) => ExprOp::ArrayElement,
        }
    }
}

impl<R: Read + ?Sized> ReadFrom<R> for Expr {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let opcode = reader.read_u8()?;
        let expr = Ggte::get(opcode).map_err(Error::UnrecognizedOp)?;
        Ok(match expr {
            ExprOp::Equal => Self::Equal(BinaryOp::read_from(reader)?.into()),
            ExprOp::NotEqual => Self::NotEqual(BinaryOp::read_from(reader)?.into()),
            ExprOp::Less => Self::Less(BinaryOp::read_from(reader)?.into()),
            ExprOp::LessEqual => Self::LessEqual(BinaryOp::read_from(reader)?.into()),
            ExprOp::Greater => Self::Greater(BinaryOp::read_from(reader)?.into()),
            ExprOp::GreaterEqual => Self::GreaterEqual(BinaryOp::read_from(reader)?.into()),
            ExprOp::Not => Self::Not(Self::read_from(reader)?.into()),
            ExprOp::Add => Self::Add(BinaryOp::read_from(reader)?.into()),
            ExprOp::Subtract => Self::Subtract(BinaryOp::read_from(reader)?.into()),
            ExprOp::Multiply => Self::Multiply(BinaryOp::read_from(reader)?.into()),
            ExprOp::Divide => Self::Divide(BinaryOp::read_from(reader)?.into()),
            ExprOp::Modulo => Self::Modulo(BinaryOp::read_from(reader)?.into()),
            ExprOp::BitAnd => Self::BitAnd(BinaryOp::read_from(reader)?.into()),
            ExprOp::BitOr => Self::BitOr(BinaryOp::read_from(reader)?.into()),
            ExprOp::BitXor => Self::BitXor(BinaryOp::read_from(reader)?.into()),
            ExprOp::AddAssign => Self::AddAssign(BinaryOp::read_from(reader)?.into()),
            ExprOp::SubtractAssign => Self::SubtractAssign(BinaryOp::read_from(reader)?.into()),
            ExprOp::MultiplyAssign => Self::MultiplyAssign(BinaryOp::read_from(reader)?.into()),
            ExprOp::DivideAssign => Self::DivideAssign(BinaryOp::read_from(reader)?.into()),
            ExprOp::ModuloAssign => Self::ModuloAssign(BinaryOp::read_from(reader)?.into()),
            ExprOp::BitAndAssign => Self::BitAndAssign(BinaryOp::read_from(reader)?.into()),
            ExprOp::BitOrAssign => Self::BitOrAssign(BinaryOp::read_from(reader)?.into()),
            ExprOp::BitXorAssign => Self::BitXorAssign(BinaryOp::read_from(reader)?.into()),
            ExprOp::Imm16 => Self::Imm16(reader.read_i16::<LE>()?),
            ExprOp::Imm32 => Self::Imm32(reader.read_i32::<LE>()?),
            ExprOp::AddressOf => Self::AddressOf(Ip::read_from(reader)?),
            ExprOp::Stack => Self::Stack(reader.read_u8()?),
            ExprOp::ParentStack => Self::ParentStack(reader.read_u8()?),
            ExprOp::Flag => Self::Flag(Self::read_from(reader)?.into()),
            ExprOp::Variable => Self::Variable(Self::read_from(reader)?.into()),
            ExprOp::Result1 => Self::Result1,
            ExprOp::Result2 => Self::Result2,
            ExprOp::Pad => Self::Pad(Self::read_from(reader)?.into()),
            ExprOp::Battery => Self::Battery(Self::read_from(reader)?.into()),
            ExprOp::Money => Self::Money,
            ExprOp::Item => Self::Item(Self::read_from(reader)?.into()),
            ExprOp::Atc => Self::Atc(Self::read_from(reader)?.into()),
            ExprOp::Rank => Self::Rank,
            ExprOp::Exp => Self::Exp,
            ExprOp::Level => Self::Level,
            ExprOp::Hold => Self::Hold,
            ExprOp::Map => Self::Map(Self::read_from(reader)?.into()),
            ExprOp::ActorName => Self::ActorName(Self::read_from(reader)?.into()),
            ExprOp::ItemName => Self::ItemName(Self::read_from(reader)?.into()),
            ExprOp::Time => Self::Time(Self::read_from(reader)?.into()),
            ExprOp::CurrentSuit => Self::CurrentSuit,
            ExprOp::Scrap => Self::Scrap,
            ExprOp::CurrentAtc => Self::CurrentAtc,
            ExprOp::Use => Self::Use,
            ExprOp::Hit => Self::Hit,
            ExprOp::StickerName => Self::StickerName(Self::read_from(reader)?.into()),
            ExprOp::Obj => Self::Obj(ObjExpr::read_from(reader)?.into()),
            ExprOp::Random => Self::Random(Self::read_from(reader)?.into()),
            ExprOp::Sin => Self::Sin(Self::read_from(reader)?.into()),
            ExprOp::Cos => Self::Cos(Self::read_from(reader)?.into()),
            ExprOp::ArrayElement => Self::ArrayElement(ArrayElementExpr::read_from(reader)?.into()),
        })
    }
}

impl<W: Write + WriteIp + ?Sized> WriteTo<W> for Expr {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        let opcode = Ggte::value(self.opcode()).map_err(Error::NotSupported)?;
        writer.write_u8(opcode)?;
        match self {
            Self::Equal(op)
            | Self::NotEqual(op)
            | Self::Less(op)
            | Self::LessEqual(op)
            | Self::Greater(op)
            | Self::GreaterEqual(op)
            | Self::Add(op)
            | Self::Subtract(op)
            | Self::Multiply(op)
            | Self::Divide(op)
            | Self::Modulo(op)
            | Self::BitAnd(op)
            | Self::BitOr(op)
            | Self::BitXor(op)
            | Self::AddAssign(op)
            | Self::SubtractAssign(op)
            | Self::MultiplyAssign(op)
            | Self::DivideAssign(op)
            | Self::ModuloAssign(op)
            | Self::BitAndAssign(op)
            | Self::BitOrAssign(op)
            | Self::BitXorAssign(op) => op.write_to(writer),
            Self::Not(e)
            | Self::Flag(e)
            | Self::Variable(e)
            | Self::Pad(e)
            | Self::Battery(e)
            | Self::Item(e)
            | Self::Atc(e)
            | Self::Map(e)
            | Self::ActorName(e)
            | Self::ItemName(e)
            | Self::Time(e)
            | Self::StickerName(e)
            | Self::Random(e)
            | Self::Sin(e)
            | Self::Cos(e) => e.write_to(writer),
            Self::Result1
            | Self::Result2
            | Self::Money
            | Self::Rank
            | Self::Exp
            | Self::Level
            | Self::Hold
            | Self::CurrentSuit
            | Self::Scrap
            | Self::CurrentAtc
            | Self::Use
            | Self::Hit => Ok(()),
            Self::Imm16(x) => Ok(writer.write_i16::<LE>(*x)?),
            Self::Imm32(x) => Ok(writer.write_i32::<LE>(*x)?),
            Self::AddressOf(ip) => Ok(ip.write_to(writer)?),
            Self::Stack(x) | Self::ParentStack(x) => Ok(writer.write_u8(*x)?),
            Self::Obj(e) => e.write_to(writer),
            Self::ArrayElement(e) => e.write_to(writer),
        }
    }
}

fn debug0(f: &mut fmt::Formatter<'_>, name: &str) -> fmt::Result {
    write!(f, "{}", name)
}

fn debug1(f: &mut fmt::Formatter<'_>, name: &str, arg: &impl Debug) -> fmt::Result {
    f.debug_tuple(name).field(arg).finish()
}

fn debug2(f: &mut fmt::Formatter<'_>, name: &str, arg: &BinaryOp) -> fmt::Result {
    f.debug_tuple(name).field(&arg.lhs).field(&arg.rhs).finish()
}

fn debug_item(f: &mut fmt::Formatter<'_>, name: &str, expr: &Expr) -> fmt::Result {
    if let Some(id) = expr.value() {
        if let Ok(item) = Item::try_from(id as i16) {
            return f.debug_tuple(name).field(&item).finish();
        }
    }
    f.debug_tuple(name).field(expr).finish()
}

fn debug_atc(f: &mut fmt::Formatter<'_>, name: &str, expr: &Expr) -> fmt::Result {
    if let Some(id) = expr.value() {
        if let Ok(atc) = Atc::try_from(id as i16) {
            return f.debug_tuple(name).field(&atc).finish();
        }
    }
    f.debug_tuple(name).field(expr).finish()
}

impl Debug for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Equal(op) => debug2(f, "Equal", op),
            Self::NotEqual(op) => debug2(f, "NotEqual", op),
            Self::Less(op) => debug2(f, "Less", op),
            Self::LessEqual(op) => debug2(f, "LessEqual", op),
            Self::Greater(op) => debug2(f, "Greater", op),
            Self::GreaterEqual(op) => debug2(f, "GreaterEqual", op),
            Self::Not(e) => debug1(f, "Not", e),
            Self::Add(op) => debug2(f, "Add", op),
            Self::Subtract(op) => debug2(f, "Subtract", op),
            Self::Multiply(op) => debug2(f, "Multiply", op),
            Self::Divide(op) => debug2(f, "Divide", op),
            Self::Modulo(op) => debug2(f, "Modulo", op),
            Self::BitAnd(op) => debug2(f, "BitAnd", op),
            Self::BitOr(op) => debug2(f, "BitOr", op),
            Self::BitXor(op) => debug2(f, "BitXor", op),
            Self::AddAssign(op) => debug2(f, "AddAssign", op),
            Self::SubtractAssign(op) => debug2(f, "SubtractAssign", op),
            Self::MultiplyAssign(op) => debug2(f, "MultiplyAssign", op),
            Self::DivideAssign(op) => debug2(f, "DivideAssign", op),
            Self::ModuloAssign(op) => debug2(f, "ModuloAssign", op),
            Self::BitAndAssign(op) => debug2(f, "BitAndAssign", op),
            Self::BitOrAssign(op) => debug2(f, "BitOrAssign", op),
            Self::BitXorAssign(op) => debug2(f, "BitXorAssign", op),
            Self::Imm16(e) => debug1(f, "Imm16", e),
            Self::Imm32(e) => debug1(f, "Imm32", e),
            Self::AddressOf(e) => debug1(f, "AddressOf", e),
            Self::Stack(e) => debug1(f, "Stack", e),
            Self::ParentStack(e) => debug1(f, "ParentStack", e),
            Self::Flag(e) => debug1(f, "Flag", e),
            Self::Variable(e) => debug1(f, "Variable", e),
            Self::Result1 => debug0(f, "Result1"),
            Self::Result2 => debug0(f, "Result2"),
            Self::Pad(e) => debug1(f, "Pad", e),
            Self::Battery(e) => debug1(f, "Battery", e),
            Self::Money => debug0(f, "Money"),
            Self::Item(e) => debug_item(f, "Item", e),
            Self::Atc(e) => debug_atc(f, "Atc", e),
            Self::Rank => debug0(f, "Rank"),
            Self::Exp => debug0(f, "Exp"),
            Self::Level => debug0(f, "Level"),
            Self::Hold => debug0(f, "Hold"),
            Self::Map(e) => debug1(f, "Map", e),
            Self::ActorName(e) => debug1(f, "ActorName", e),
            Self::ItemName(e) => debug_item(f, "ItemName", e),
            Self::Time(e) => debug1(f, "Time", e),
            Self::CurrentSuit => debug0(f, "CurrentSuit"),
            Self::Scrap => debug0(f, "Scrap"),
            Self::CurrentAtc => debug0(f, "CurrentAtc"),
            Self::Use => debug0(f, "Use"),
            Self::Hit => debug0(f, "Hit"),
            Self::StickerName(e) => debug1(f, "StickerName", e),
            Self::Obj(e) => debug1(f, "Obj", e),
            Self::Random(e) => debug1(f, "Random", e),
            Self::Sin(e) => debug1(f, "Sin", e),
            Self::Cos(e) => debug1(f, "Cos", e),
            Self::ArrayElement(e) => debug1(f, "ArrayElement", e),
        }
    }
}

/// A reference which appears on the left-hand side of an assignment.
#[derive(Clone, PartialEq, Eq)]
pub enum SetExpr {
    Stack(u8),
    Flag(Expr),
    Variable(Expr),
    Result1,
    Result2,
    Pad(Expr),
    Battery(Expr),
    Money,
    Item(Expr),
    Atc(Expr),
    Rank,
    Exp,
    Level,
    Time(Expr),
    CurrentSuit,
    Scrap,
    CurrentAtc,
}

impl SetExpr {
    /// Constructs a new `SetExpr` from a variable index.
    #[must_use]
    pub fn from_var(index: i32) -> Self {
        Self::Variable(Expr::Imm32(index))
    }

    /// Constructs a new `SetExpr` from a flag index.
    #[must_use]
    pub fn from_flag(index: i32) -> Self {
        Self::Flag(Expr::Imm32(index))
    }

    /// Returns the opcode corresponding to the expression.
    #[must_use]
    pub fn opcode(&self) -> ExprOp {
        match self {
            Self::Stack(_) => ExprOp::Stack,
            Self::Flag(_) => ExprOp::Flag,
            Self::Variable(_) => ExprOp::Variable,
            Self::Result1 => ExprOp::Result1,
            Self::Result2 => ExprOp::Result2,
            Self::Pad(_) => ExprOp::Pad,
            Self::Battery(_) => ExprOp::Battery,
            Self::Money => ExprOp::Money,
            Self::Item(_) => ExprOp::Item,
            Self::Atc(_) => ExprOp::Atc,
            Self::Rank => ExprOp::Rank,
            Self::Exp => ExprOp::Exp,
            Self::Level => ExprOp::Level,
            Self::Time(_) => ExprOp::Time,
            Self::CurrentSuit => ExprOp::CurrentSuit,
            Self::Scrap => ExprOp::Scrap,
            Self::CurrentAtc => ExprOp::CurrentAtc,
        }
    }
}

impl<R: Read + ?Sized> ReadFrom<R> for SetExpr {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let opcode = reader.read_u8()?;
        let expr = Ggte::get(opcode).map_err(Error::UnrecognizedOp)?;
        Ok(match expr {
            ExprOp::Stack => Self::Stack(reader.read_u8()?),
            ExprOp::Flag => Self::Flag(Expr::read_from(reader)?),
            ExprOp::Variable => Self::Variable(Expr::read_from(reader)?),
            ExprOp::Result1 => Self::Result1,
            ExprOp::Result2 => Self::Result2,
            ExprOp::Pad => Self::Pad(Expr::read_from(reader)?),
            ExprOp::Battery => Self::Battery(Expr::read_from(reader)?),
            ExprOp::Money => Self::Money,
            ExprOp::Item => Self::Item(Expr::read_from(reader)?),
            ExprOp::Atc => Self::Atc(Expr::read_from(reader)?),
            ExprOp::Rank => Self::Rank,
            ExprOp::Exp => Self::Exp,
            ExprOp::Level => Self::Level,
            ExprOp::Time => Self::Time(Expr::read_from(reader)?),
            ExprOp::CurrentSuit => Self::CurrentSuit,
            ExprOp::Scrap => Self::Scrap,
            ExprOp::CurrentAtc => Self::CurrentAtc,
            _ => return Err(Error::NotSupported(expr)),
        })
    }
}

impl<W: Write + WriteIp + ?Sized> WriteTo<W> for SetExpr {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        let opcode = Ggte::value(self.opcode()).map_err(Error::NotSupported)?;
        writer.write_u8(opcode)?;
        match self {
            Self::Stack(x) => Ok(writer.write_u8(*x)?),
            Self::Flag(e)
            | Self::Variable(e)
            | Self::Pad(e)
            | Self::Battery(e)
            | Self::Item(e)
            | Self::Atc(e)
            | Self::Time(e) => e.write_to(writer),
            Self::Result1
            | Self::Result2
            | Self::Money
            | Self::Rank
            | Self::Exp
            | Self::Level
            | Self::CurrentSuit
            | Self::Scrap
            | Self::CurrentAtc => Ok(()),
        }
    }
}

impl TryFrom<Expr> for SetExpr {
    type Error = Error;
    fn try_from(op: Expr) -> Result<Self> {
        Ok(match op {
            Expr::Stack(b) => Self::Stack(b),
            Expr::Flag(op) => Self::Flag(*op),
            Expr::Variable(op) => Self::Variable(*op),
            Expr::Result1 => Self::Result1,
            Expr::Result2 => Self::Result2,
            Expr::Pad(op) => Self::Pad(*op),
            Expr::Battery(op) => Self::Battery(*op),
            Expr::Money => Self::Money,
            Expr::Item(op) => Self::Item(*op),
            Expr::Atc(op) => Self::Atc(*op),
            Expr::Rank => Self::Rank,
            Expr::Exp => Self::Exp,
            Expr::Level => Self::Level,
            Expr::Time(op) => Self::Time(*op),
            Expr::CurrentSuit => Self::CurrentSuit,
            Expr::Scrap => Self::Scrap,
            Expr::CurrentAtc => Self::CurrentAtc,
            _ => return Err(Error::NotSupported(op.opcode())),
        })
    }
}

impl Debug for SetExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stack(e) => debug1(f, "Stack", e),
            Self::Flag(e) => debug1(f, "Flag", e),
            Self::Variable(e) => debug1(f, "Variable", e),
            Self::Result1 => debug0(f, "Result1"),
            Self::Result2 => debug0(f, "Result2"),
            Self::Pad(e) => debug1(f, "Pad", e),
            Self::Battery(e) => debug1(f, "Battery", e),
            Self::Money => debug0(f, "Money"),
            Self::Item(e) => debug_item(f, "Item", e),
            Self::Atc(e) => debug_atc(f, "Atc", e),
            Self::Rank => debug0(f, "Rank"),
            Self::Exp => debug0(f, "Exp"),
            Self::Level => debug0(f, "Level"),
            Self::Time(e) => debug1(f, "Time", e),
            Self::CurrentSuit => debug0(f, "CurrentSuit"),
            Self::Scrap => debug0(f, "Scrap"),
            Self::CurrentAtc => debug0(f, "CurrentAtc"),
        }
    }
}

#[derive(Clone, PartialEq, Eq, ReadFrom, WriteTo)]
#[read_from(error = Error)]
#[write_to(stream = Write + WriteIp, error = Error)]
pub struct BinaryOp {
    /// The right-hand operand.
    pub rhs: Expr,
    /// The left-hand operand.
    pub lhs: Expr,
}

impl BinaryOp {
    /// Constructs a new `BinaryOp` using `lhs` and `rhs`.
    pub fn new(lhs: Expr, rhs: Expr) -> Self {
        Self { rhs, lhs }
    }
}

// Custom Debug impl to print the left-hand side first
impl Debug for BinaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BinaryOp").field("lhs", &self.lhs).field("rhs", &self.rhs).finish()
    }
}

expr_enum! {
    type Error = Error;
    pub enum ObjExpr {
        Anim(ObjExprObj) => TypeOp::Anim,
        Dir(ObjExprObj) => TypeOp::Dir,
        PosX(ObjExprObj) => TypeOp::PosX,
        PosY(ObjExprObj) => TypeOp::PosY,
        PosZ(ObjExprObj) => TypeOp::PosZ,
        BoneX(ObjExprBone) => TypeOp::BoneX,
        BoneY(ObjExprBone) => TypeOp::BoneY,
        BoneZ(ObjExprBone) => TypeOp::BoneZ,
        DirTo(ObjExprPair) => TypeOp::DirTo,
        Distance(ObjExprPair) => TypeOp::Distance,
        Unk235(ObjExprObj) => TypeOp::Unk235,
        Unk247(ObjExprObj) => TypeOp::Unk247,
        Unk248(ObjExprObj) => TypeOp::Unk248,
        Unk249(ObjExprBone) => TypeOp::Unk249,
        Unk250(ObjExprBone) => TypeOp::Unk250,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ReadFrom, WriteTo)]
#[read_from(error = Error)]
#[write_to(stream = Write + WriteIp, error = Error)]
pub struct ObjExprObj {
    pub obj: Expr,
}

/// A pointer to an `ObjPair`.
#[derive(Debug, Clone, PartialEq, Eq, ReadFrom, WriteTo)]
#[read_from(error = Error)]
#[write_to(stream = Write + WriteIp, error = Error)]
pub struct ObjExprPair {
    pub address: Expr,
}

/// A pair of objects.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ObjPair {
    pub first: i16,
    pub second: i16,
}

impl<R: Read + ?Sized> ReadFrom<R> for ObjPair {
    type Error = io::Error;
    fn read_from(reader: &mut R) -> io::Result<Self> {
        Ok(Self { first: reader.read_i16::<LE>()?, second: reader.read_i16::<LE>()? })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for ObjPair {
    type Error = io::Error;
    fn write_to(&self, writer: &mut W) -> io::Result<()> {
        writer.write_i16::<LE>(self.first)?;
        writer.write_i16::<LE>(self.second)?;
        Ok(())
    }
}

/// A pointer to an `ObjBone`.
#[derive(Debug, Clone, PartialEq, Eq, ReadFrom, WriteTo)]
#[read_from(error = Error)]
#[write_to(stream = Write + WriteIp, error = Error)]
pub struct ObjExprBone {
    pub address: Expr,
}

/// A path to a bone in an object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjBone {
    pub obj: i16,
    /// Each element in this array is the index of the bone at each level in the model hierarchy.
    /// The first level is the level below the root bone.
    pub path: Vec<i16>,
}

/// A wrapper which makes it easier to work with expressions that reference sounds.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(variant_size_differences)]
pub enum SoundExpr {
    /// An immediate expression referring to no sound.
    None,
    /// An immediate expression referring to a sound effect.
    Sfx(Sfx),
    /// An immediate expression referring to a music track.
    Music(Music),
    /// An expression which evaulates to a sound ID.
    Expr(Box<Expr>),
}

impl Default for SoundExpr {
    fn default() -> Self {
        Self::None
    }
}

impl From<Sound> for SoundExpr {
    fn from(id: Sound) -> Self {
        match id {
            Sound::None => Self::None,
            Sound::Sfx(sound) => Self::Sfx(sound),
            Sound::Music(music) => Self::Music(music),
        }
    }
}

impl TryFrom<SoundExpr> for Sound {
    type Error = SoundExpr;
    fn try_from(expr: SoundExpr) -> std::result::Result<Self, Self::Error> {
        match expr {
            SoundExpr::None => Ok(Self::None),
            SoundExpr::Sfx(sound) => Ok(Self::Sfx(sound)),
            SoundExpr::Music(music) => Ok(Self::Music(music)),
            SoundExpr::Expr(_) => Err(expr),
        }
    }
}

impl From<Sfx> for SoundExpr {
    fn from(sound: Sfx) -> Self {
        Self::Sfx(sound)
    }
}

impl From<Music> for SoundExpr {
    fn from(music: Music) -> Self {
        Self::Music(music)
    }
}

impl From<Expr> for SoundExpr {
    fn from(expr: Expr) -> Self {
        let id = match expr.value() {
            Some(id) => id as u32,
            None => return Self::Expr(expr.into()),
        };
        match Sound::try_from(id) {
            Ok(id) => id.into(),
            Err(_) => Self::Expr(expr.into()),
        }
    }
}

impl From<SoundExpr> for Expr {
    fn from(sfx: SoundExpr) -> Self {
        match sfx {
            SoundExpr::None => Expr::Imm32(-1),
            SoundExpr::Sfx(sound) => Expr::Imm32(u32::from(Sound::Sfx(sound)) as i32),
            SoundExpr::Music(music) => Expr::Imm32(u32::from(Sound::Music(music)) as i32),
            SoundExpr::Expr(expr) => *expr,
        }
    }
}

impl<R: Read + ?Sized> ReadFrom<R> for SoundExpr {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Expr::read_from(reader)?.into())
    }
}

impl<W: Write + WriteIp + ?Sized> WriteTo<W> for SoundExpr {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        match self {
            Self::None | Self::Sfx(_) | Self::Music(_) => Expr::from(self.clone()).write_to(writer),
            Self::Expr(expr) => expr.write_to(writer),
        }
    }
}

impl<R: Read + ?Sized> ReadFrom<R> for ObjBone {
    type Error = io::Error;
    fn read_from(reader: &mut R) -> io::Result<Self> {
        let obj = reader.read_i16::<LE>()?;
        let count = reader.read_i16::<LE>()?;
        let path = if count > 0 {
            let mut path = vec![0i16; count as usize];
            reader.read_i16_into::<LE>(&mut path)?;
            path
        } else {
            vec![]
        };
        Ok(Self { obj, path })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for ObjBone {
    type Error = io::Error;
    fn write_to(&self, writer: &mut W) -> io::Result<()> {
        writer.write_i16::<LE>(self.obj)?;
        writer.write_i16::<LE>(self.path.len().try_into().expect("Bone size overflow"))?;
        for &bone in &self.path {
            writer.write_i16::<LE>(bone)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ReadFrom, WriteTo)]
#[read_from(error = Error)]
#[write_to(stream = Write + WriteIp, error = Error)]
pub struct ArrayElementExpr {
    /// The size/type of each element in the array:
    /// * `-4`: Signed 32-bit integer
    /// * `-2`: Signed 16-bit integer
    /// * `-1`: Signed 8-bit integer
    /// * `1`: Unsigned 8-bit integer
    /// * `2`: Unsigned 16-bit integer
    /// * `4`: Unsigned 32-bit integer
    pub element_type: Expr,
    /// The index of the element to retrieve.
    pub index: Expr,
    /// The address of the array.
    pub address: Expr,
}

impl From<i16> for Expr {
    fn from(imm: i16) -> Self {
        Self::Imm16(imm)
    }
}

impl From<i32> for Expr {
    fn from(imm: i32) -> Self {
        Self::imm(imm)
    }
}

impl From<Atc> for Expr {
    fn from(id: Atc) -> Self {
        Self::Imm16(id.into())
    }
}

impl From<Item> for Expr {
    fn from(id: Item) -> Self {
        Self::Imm16(id.into())
    }
}

impl From<Object> for Expr {
    fn from(id: Object) -> Self {
        Self::imm(id.into())
    }
}

/// Generates `TryFrom` implementations for converting from an `Expr` to an ID type.
macro_rules! impl_try_from_expr {
    ($idtype:ty, $base:ty, $err:path) => {
        #[allow(trivial_numeric_casts)]
        impl TryFrom<Expr> for $idtype {
            type Error = Error;
            fn try_from(expr: Expr) -> Result<Self> {
                if let Some(id) = expr.value() {
                    Ok(Self::try_from(id as $base).map_err(|_| $err(expr.into()))?)
                } else {
                    Err(Error::NonConstant(expr.into()))
                }
            }
        }

        #[allow(trivial_numeric_casts)]
        impl TryFrom<&Expr> for $idtype {
            type Error = Error;
            fn try_from(expr: &Expr) -> Result<Self> {
                if let Some(id) = expr.value() {
                    Ok(Self::try_from(id as $base).map_err(|_| $err(expr.clone().into()))?)
                } else {
                    Err(Error::NonConstant(expr.clone().into()))
                }
            }
        }
    };
}

impl_try_from_expr!(Atc, i16, Error::InvalidAtc);
impl_try_from_expr!(Item, i16, Error::InvalidItem);
impl_try_from_expr!(Object, i32, Error::InvalidObject);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_write_and_read;

    fn expr() -> Box<Expr> {
        Box::new(Expr::Imm32(123))
    }

    fn binary_op() -> Box<BinaryOp> {
        BinaryOp::new(Expr::Imm32(123), Expr::Imm32(456)).into()
    }

    fn obj_obj_expr() -> Box<ObjExpr> {
        Box::new(ObjExpr::PosX(ObjExprObj { obj: Expr::Imm32(20000) }))
    }

    fn obj_pair_expr() -> Box<ObjExpr> {
        Box::new(ObjExpr::Distance(ObjExprPair { address: Expr::AddressOf(Ip::Offset(123)) }))
    }

    fn obj_bone_expr() -> Box<ObjExpr> {
        Box::new(ObjExpr::BoneX(ObjExprBone { address: Expr::AddressOf(Ip::Offset(123)) }))
    }

    fn array_element_expr() -> Box<ArrayElementExpr> {
        Box::new(ArrayElementExpr {
            element_type: Expr::Imm32(-4),
            index: Expr::Imm32(123),
            address: Expr::AddressOf(Ip::Offset(123)),
        })
    }

    #[test]
    fn test_negate() {
        let original = *expr();
        let expected = Expr::Not(expr());
        let actual = original.clone().negate();
        assert_eq!(actual, expected);
        assert_eq!(actual.negate(), original);

        let original = Expr::Equal(binary_op());
        let expected = Expr::NotEqual(binary_op());
        let actual = original.clone().negate();
        assert_eq!(actual, expected);
        assert_eq!(actual.negate(), original);

        let original = Expr::Less(binary_op());
        let expected = Expr::GreaterEqual(binary_op());
        let actual = original.clone().negate();
        assert_eq!(actual, expected);
        assert_eq!(actual.negate(), original);

        let original = Expr::Greater(binary_op());
        let expected = Expr::LessEqual(binary_op());
        let actual = original.clone().negate();
        assert_eq!(actual, expected);
        assert_eq!(actual.negate(), original);

        let original = Expr::BitAnd(BinaryOp::new(*expr(), *expr()).into());
        let expected = Expr::BitOr(BinaryOp::new(Expr::Not(expr()), Expr::Not(expr())).into());
        let actual = original.clone().negate();
        assert_eq!(actual, expected);
        assert_eq!(actual.negate(), original);
    }

    #[test]
    fn test_imm() {
        assert_eq!(Expr::imm(123), Expr::Imm16(123));
        assert_eq!(Expr::imm(-123), Expr::Imm16(-123));
        assert_eq!(Expr::imm(0x12345678), Expr::Imm32(0x12345678));
        assert_eq!(Expr::imm(-0x12345678), Expr::Imm32(-0x12345678));
        assert_eq!(Expr::imm(i16::MAX as i32), Expr::Imm16(i16::MAX));
        assert_eq!(Expr::imm(i16::MIN as i32), Expr::Imm16(i16::MIN));
        assert_eq!(Expr::imm(i16::MAX as i32 + 1), Expr::Imm32(i16::MAX as i32 + 1));
        assert_eq!(Expr::imm(i16::MIN as i32 - 1), Expr::Imm32(i16::MIN as i32 - 1));
        assert_eq!(Expr::imm(i32::MAX), Expr::Imm32(i32::MAX));
        assert_eq!(Expr::imm(i32::MIN), Expr::Imm32(i32::MIN));
    }

    #[test]
    fn test_expr_from_imm() {
        assert_eq!(Expr::from(123i16), Expr::Imm16(123));
        assert_eq!(Expr::from(123i32), Expr::Imm16(123));
        assert_eq!(Expr::from(0x12345678), Expr::Imm32(0x12345678));
    }

    #[test]
    fn test_atc_from_expr() {
        let expected = Atc::Toothbrush;
        let expr = Expr::Imm16(expected.into());
        assert_eq!(Atc::try_from(&expr).unwrap(), expected);
        assert_eq!(Atc::try_from(expr).unwrap(), expected);

        assert!(matches!(Atc::try_from(Expr::Imm16(-1)), Err(Error::InvalidAtc(_))));
        assert!(matches!(Atc::try_from(Expr::Stack(0)), Err(Error::NonConstant(_))));
    }

    #[test]
    fn test_expr_from_atc() {
        assert_eq!(Expr::Imm16(Atc::Toothbrush.into()), Atc::Toothbrush.into());
    }

    #[test]
    fn test_item_from_expr() {
        let expected = Item::HotRod;
        let expr = Expr::Imm16(expected.into());
        assert_eq!(Item::try_from(&expr).unwrap(), expected);
        assert_eq!(Item::try_from(expr).unwrap(), expected);

        assert!(matches!(Item::try_from(Expr::Imm16(-1)), Err(Error::InvalidItem(_))));
        assert!(matches!(Item::try_from(Expr::Stack(0)), Err(Error::NonConstant(_))));
    }

    #[test]
    fn test_expr_from_item() {
        assert_eq!(Expr::Imm16(Item::HotRod.into()), Item::HotRod.into());
    }

    #[test]
    fn test_object_from_expr() {
        let expected = Object::NpcTonpy;
        let expr = Expr::Imm32(expected.into());
        assert_eq!(Object::try_from(&expr).unwrap(), expected);
        assert_eq!(Object::try_from(expr).unwrap(), expected);

        assert!(matches!(Object::try_from(Expr::Imm16(-1)), Err(Error::InvalidObject(_))));
        assert!(matches!(Object::try_from(Expr::Stack(0)), Err(Error::NonConstant(_))));
    }

    #[test]
    fn test_expr_from_object() {
        assert_eq!(Expr::imm(Object::NpcTonpy.into()), Object::NpcTonpy.into());
    }

    #[test]
    fn test_write_and_read_expr() {
        assert_write_and_read!(Expr::Equal(binary_op()));
        assert_write_and_read!(Expr::NotEqual(binary_op()));
        assert_write_and_read!(Expr::Less(binary_op()));
        assert_write_and_read!(Expr::LessEqual(binary_op()));
        assert_write_and_read!(Expr::Greater(binary_op()));
        assert_write_and_read!(Expr::GreaterEqual(binary_op()));
        assert_write_and_read!(Expr::Not(expr()));
        assert_write_and_read!(Expr::Add(binary_op()));
        assert_write_and_read!(Expr::Subtract(binary_op()));
        assert_write_and_read!(Expr::Multiply(binary_op()));
        assert_write_and_read!(Expr::Divide(binary_op()));
        assert_write_and_read!(Expr::Modulo(binary_op()));
        assert_write_and_read!(Expr::BitAnd(binary_op()));
        assert_write_and_read!(Expr::BitOr(binary_op()));
        assert_write_and_read!(Expr::BitXor(binary_op()));
        assert_write_and_read!(Expr::AddAssign(binary_op()));
        assert_write_and_read!(Expr::SubtractAssign(binary_op()));
        assert_write_and_read!(Expr::MultiplyAssign(binary_op()));
        assert_write_and_read!(Expr::DivideAssign(binary_op()));
        assert_write_and_read!(Expr::ModuloAssign(binary_op()));
        assert_write_and_read!(Expr::BitAndAssign(binary_op()));
        assert_write_and_read!(Expr::BitOrAssign(binary_op()));
        assert_write_and_read!(Expr::BitXorAssign(binary_op()));
        assert_write_and_read!(Expr::Imm16(123));
        assert_write_and_read!(Expr::Imm32(123));
        assert_write_and_read!(Expr::AddressOf(Ip::Offset(123)));
        assert_write_and_read!(Expr::Stack(123));
        assert_write_and_read!(Expr::ParentStack(123));
        assert_write_and_read!(Expr::Flag(expr()));
        assert_write_and_read!(Expr::Variable(expr()));
        assert_write_and_read!(Expr::Result1);
        assert_write_and_read!(Expr::Result2);
        assert_write_and_read!(Expr::Pad(expr()));
        assert_write_and_read!(Expr::Battery(expr()));
        assert_write_and_read!(Expr::Money);
        assert_write_and_read!(Expr::Item(expr()));
        assert_write_and_read!(Expr::Atc(expr()));
        assert_write_and_read!(Expr::Rank);
        assert_write_and_read!(Expr::Exp);
        assert_write_and_read!(Expr::Level);
        assert_write_and_read!(Expr::Hold);
        assert_write_and_read!(Expr::Map(expr()));
        assert_write_and_read!(Expr::ActorName(expr()));
        assert_write_and_read!(Expr::ItemName(expr()));
        assert_write_and_read!(Expr::Time(expr()));
        assert_write_and_read!(Expr::CurrentSuit);
        assert_write_and_read!(Expr::Scrap);
        assert_write_and_read!(Expr::CurrentAtc);
        assert_write_and_read!(Expr::Use);
        assert_write_and_read!(Expr::Hit);
        assert_write_and_read!(Expr::StickerName(expr()));
        assert_write_and_read!(Expr::Obj(obj_obj_expr()));
        assert_write_and_read!(Expr::Obj(obj_pair_expr()));
        assert_write_and_read!(Expr::Obj(obj_bone_expr()));
        assert_write_and_read!(Expr::Random(expr()));
        assert_write_and_read!(Expr::Sin(expr()));
        assert_write_and_read!(Expr::Cos(expr()));
        assert_write_and_read!(Expr::ArrayElement(array_element_expr()));
    }

    #[test]
    fn test_write_and_read_set_expr() {
        assert_write_and_read!(SetExpr::Stack(123));
        assert_write_and_read!(SetExpr::Flag(*expr()));
        assert_write_and_read!(SetExpr::Variable(*expr()));
        assert_write_and_read!(SetExpr::Result1);
        assert_write_and_read!(SetExpr::Result2);
        assert_write_and_read!(SetExpr::Pad(*expr()));
        assert_write_and_read!(SetExpr::Battery(*expr()));
        assert_write_and_read!(SetExpr::Money);
        assert_write_and_read!(SetExpr::Item(*expr()));
        assert_write_and_read!(SetExpr::Atc(*expr()));
        assert_write_and_read!(SetExpr::Rank);
        assert_write_and_read!(SetExpr::Exp);
        assert_write_and_read!(SetExpr::Level);
        assert_write_and_read!(SetExpr::Time(*expr()));
        assert_write_and_read!(SetExpr::CurrentSuit);
        assert_write_and_read!(SetExpr::Scrap);
        assert_write_and_read!(SetExpr::CurrentAtc);
    }

    #[test]
    fn test_write_and_read_obj_pair() {
        assert_write_and_read!(ObjPair { first: 123, second: 456 });
    }

    #[test]
    fn test_write_and_read_obj_bone() {
        assert_write_and_read!(ObjBone { obj: 123, path: vec![1, 2, 3, 4, 5, 6, 7, 8] });
    }

    #[test]
    fn test_write_and_read_sound_expr() {
        assert_write_and_read!(SoundExpr::None);
        assert_write_and_read!(SoundExpr::Sfx(Sfx::KitchenOil));
        assert_write_and_read!(SoundExpr::Music(Music::BgmNight));
        assert_write_and_read!(SoundExpr::Expr(Expr::from_var(0).into()));
    }
}
