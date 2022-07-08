mod opcodes;
mod writer;

pub use writer::{AsmBlock, Program, ProgramBuilder, ProgramWriter, Subroutine};

use opcodes::{AsmMsgOp, NamedOpcode};
use std::rc::Rc;
use unplug::common::Text;
use unplug::event::opcodes::{ExprOp, TypeOp};

/// An operation consisting of an opcode and zero or more operands.
#[derive(Debug, Clone)]
pub struct Operation<T: NamedOpcode> {
    pub opcode: T,
    pub operands: Vec<Operand>,
}

impl<T: NamedOpcode> Operation<T> {
    pub fn new(opcode: T) -> Self {
        Self { opcode, operands: vec![] }
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
    Label(Rc<String>),
    /// A raw file offset reference.
    Offset(u32),
    /// A type expression.
    Type(TypeOp),
    /// An expression.
    Expr(Operation<ExprOp>),
    /// A message command.
    MsgCommand(Operation<AsmMsgOp>),
}
