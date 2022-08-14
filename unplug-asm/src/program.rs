use crate::label::{LabelId, LabelMap};
use crate::opcodes::{AsmMsgOp, DirOp, NamedOpcode};
use bitflags::bitflags;
use smallvec::SmallVec;
use std::collections::HashMap;
use unplug::common::Text;
use unplug::event::opcodes::{CmdOp, ExprOp, TypeOp};
use unplug::event::BlockId;
use unplug::stage::Event;

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

/// A command or directive.
#[derive(Debug, Clone)]
pub enum Instruction {
    Command(Operation<CmdOp>),
    Directive(Operation<DirOp>),
}

/// Encapsulates all possible operation types.
pub enum AnyOperation {
    Command(Operation<CmdOp>),
    Expr(Operation<ExprOp>),
    MsgCommand(Operation<AsmMsgOp>),
    Directive(Operation<DirOp>),
}

impl AnyOperation {
    /// Appends `operand` onto the operation.
    pub fn push_operand(&mut self, operand: Operand) {
        match self {
            Self::Command(op) => op.operands.push(operand),
            Self::Expr(op) => op.operands.push(operand),
            Self::MsgCommand(op) => op.operands.push(operand),
            Self::Directive(op) => op.operands.push(operand),
        }
    }

    /// Consumes this wrapper and returns the inner command.
    /// ***Panics*** if the operation is not a command.
    pub fn into_command(self) -> Operation<CmdOp> {
        match self {
            Self::Command(op) => op,
            _ => panic!("expected a command operation"),
        }
    }

    /// Consumes this wrapper and returns the inner expression.
    /// ***Panics*** if the operation is not an expression.
    pub fn into_expr(self) -> Operation<ExprOp> {
        match self {
            Self::Expr(op) => op,
            _ => panic!("expected an expr operation"),
        }
    }

    /// Consumes this wrapper and returns the inner message command.
    /// ***Panics*** if the operation is not a message command.
    pub fn into_msg_command(self) -> Operation<AsmMsgOp> {
        match self {
            Self::MsgCommand(op) => op,
            _ => panic!("expected a message command"),
        }
    }

    /// Consumes this wrapper and returns the inner directive.
    /// ***Panics*** if the operation is not a directive.
    pub fn into_directive(self) -> Operation<DirOp> {
        match self {
            Self::Directive(op) => op,
            _ => panic!("expected a directive"),
        }
    }
}

impl From<Operation<CmdOp>> for AnyOperation {
    fn from(op: Operation<CmdOp>) -> Self {
        Self::Command(op)
    }
}

impl From<Operation<ExprOp>> for AnyOperation {
    fn from(op: Operation<ExprOp>) -> Self {
        Self::Expr(op)
    }
}

impl From<Operation<AsmMsgOp>> for AnyOperation {
    fn from(op: Operation<AsmMsgOp>) -> Self {
        Self::MsgCommand(op)
    }
}

impl From<Operation<DirOp>> for AnyOperation {
    fn from(op: Operation<DirOp>) -> Self {
        Self::Directive(op)
    }
}

bitflags! {
    /// Flags used to mark blocks with hints for serialization/deserialization.
    pub struct BlockFlags: u8 {
        /// The block is the beginning of a subroutine.
        const SUBROUTINE = 1 << 0;
        /// The block is associated with at least one event.
        const EVENT = 1 << 1;
        /// The block contains data instructions.
        const DATA = 1 << 2;
    }
}

/// A block of instructions corresponding to a script block.
#[derive(Debug, Clone)]
pub struct Block {
    pub id: BlockId,
    pub offset: u32,
    pub flags: BlockFlags,
    pub instructions: Vec<Instruction>,
}

impl Block {
    /// Creates an empty code block associated with block `id`.
    pub fn new(id: BlockId) -> Self {
        Self { id, offset: 0, flags: BlockFlags::empty(), instructions: vec![] }
    }

    /// Creates a code block associated with block `id` and populated from `commands`.
    pub fn with_commands(
        id: BlockId,
        commands: impl IntoIterator<Item = Operation<CmdOp>>,
    ) -> Self {
        let mut block = Self::new(id);
        block.extend(commands);
        block
    }

    /// Creates a data block associated with block `id` and populated from `directives`.
    pub fn with_data(id: BlockId, directives: impl IntoIterator<Item = Operation<DirOp>>) -> Self {
        let mut block = Self::new(id);
        block.extend(directives);
        block
    }

    /// Returns true if there are no instructions in the block.
    pub fn is_empty(&self) -> bool {
        self.instructions.is_empty()
    }
}

impl Extend<Operation<CmdOp>> for Block {
    fn extend<T: IntoIterator<Item = Operation<CmdOp>>>(&mut self, iter: T) {
        self.instructions.extend(iter.into_iter().map(Instruction::Command));
    }
}

impl Extend<Operation<DirOp>> for Block {
    fn extend<T: IntoIterator<Item = Operation<DirOp>>>(&mut self, iter: T) {
        self.instructions.extend(iter.into_iter().map(Instruction::Directive));
    }
}

/// An assembly program.
#[derive(Default)]
pub struct Program {
    pub blocks: Vec<Block>,
    pub events: HashMap<Event, BlockId>,
    pub labels: LabelMap,
}

impl Program {
    /// Creates an empty program.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a program with blocks populated from `blocks`.
    pub fn with_blocks(blocks: impl Into<Vec<Block>>) -> Self {
        Self { blocks: blocks.into(), ..Default::default() }
    }
}
