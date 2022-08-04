use crate::label::LabelId;
use crate::opcodes::{AsmMsgOp, NamedOpcode};
use smallvec::SmallVec;
use unplug::common::Text;
use unplug::event::opcodes::{ExprOp, TypeOp};

/// An operation consisting of an opcode and zero or more operands.
#[derive(Debug, Clone)]
pub struct Operation<T: NamedOpcode> {
    pub opcode: T,
    pub operands: SmallVec<[Operand; 2]>,
}

impl<T: NamedOpcode> Operation<T> {
    pub fn new(opcode: T) -> Self {
        Self { opcode, operands: SmallVec::new() }
    }
}

/// Data which can be operated on.
#[derive(Debug, Clone)]
pub enum Operand {
    /// An 8-bit signed integer.
    I8(i8),
    /// An 8-bit unsigned integer.
    U8(u8),
    /// A 16-bit signed integer.
    I16(i16),
    /// A 16-bit unsigned integer.
    U16(u16),
    /// A 32-bit signed integer.
    I32(i32),
    /// A 32-bit unsigned integer.
    U32(u32),
    /// A printable text string.
    Text(Text),
    /// A label reference.
    Label(LabelId),
    /// A label reference indicating it is an "else" condition.
    ElseLabel(LabelId),
    /// A raw file offset reference.
    Offset(u32),
    /// A type expression.
    Type(TypeOp),
    /// An expression.
    Expr(Box<Operation<ExprOp>>),
    /// A message command.
    MsgCommand(Box<Operation<AsmMsgOp>>),
}

macro_rules! impl_operand_from {
    ($type:ty, $name:ident) => {
        impl From<$type> for Operand {
            fn from(x: $type) -> Self {
                Self::$name(x.into())
            }
        }
    };
}
impl_operand_from!(i8, I8);
impl_operand_from!(u8, U8);
impl_operand_from!(i16, I16);
impl_operand_from!(u16, U16);
impl_operand_from!(i32, I32);
impl_operand_from!(u32, U32);
impl_operand_from!(Text, Text);
impl_operand_from!(TypeOp, Type);
impl_operand_from!(Operation<ExprOp>, Expr);
impl_operand_from!(Operation<AsmMsgOp>, MsgCommand);
