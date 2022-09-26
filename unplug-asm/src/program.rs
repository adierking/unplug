use crate::label::{LabelId, LabelMap};
use crate::opcodes::{AsmMsgOp, NamedOpcode};
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
    /// Creates an empty operation with `opcode`.
    pub fn new(opcode: T) -> Self {
        Self { opcode, operands: SmallVec::new() }
    }

    /// Creates an empty operation with `opcode` and `operands`.
    pub fn with_operands(opcode: T, operands: impl IntoIterator<Item = Operand>) -> Self {
        Self { opcode, operands: operands.into_iter().collect() }
    }
}

/// Encapsulates possible code operation types.
pub enum CodeOperation {
    Command(Operation<CmdOp>),
    Expr(Operation<ExprOp>),
    MsgCommand(Operation<AsmMsgOp>),
}

impl CodeOperation {
    /// Appends `operand` onto the operation.
    pub fn push_operand(&mut self, operand: Operand) {
        match self {
            Self::Command(op) => op.operands.push(operand),
            Self::Expr(op) => op.operands.push(operand),
            Self::MsgCommand(op) => op.operands.push(operand),
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
}

impl From<Operation<CmdOp>> for CodeOperation {
    fn from(op: Operation<CmdOp>) -> Self {
        Self::Command(op)
    }
}

impl From<Operation<ExprOp>> for CodeOperation {
    fn from(op: Operation<ExprOp>) -> Self {
        Self::Expr(op)
    }
}

impl From<Operation<AsmMsgOp>> for CodeOperation {
    fn from(op: Operation<AsmMsgOp>) -> Self {
        Self::MsgCommand(op)
    }
}

bitflags! {
    /// Flags used to mark blocks with hints for serialization/deserialization.
    #[derive(Default)]
    pub struct BlockFlags: u8 {
        /// The block is the beginning of a subroutine.
        const SUBROUTINE = 1 << 0;
        /// The block is associated with at least one event.
        const EVENT = 1 << 1;
    }
}

/// Contents of a block.
#[derive(Debug, Clone)]
pub enum BlockContent {
    Code(Vec<Operation<CmdOp>>),
    Data(Vec<Operand>),
}

/// A block of instructions corresponding to a script block.
#[derive(Debug, Default, Clone)]
pub struct Block {
    /// The offset of the block in the original file (if known).
    pub offset: u32,
    /// Flags describing block properties.
    pub flags: BlockFlags,
    /// The block's content.
    pub content: Option<BlockContent>,
    /// The ID of the next block in program order, or `None` if this is the last block.
    pub next: Option<BlockId>,
}

impl Block {
    /// Creates an empty block.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a code block containing `content`.
    pub fn with_content(content: BlockContent) -> Self {
        Self { content: Some(content), ..Default::default() }
    }

    /// Creates a code block populated from `commands`.
    pub fn with_code(commands: impl IntoIterator<Item = Operation<CmdOp>>) -> Self {
        Self::with_content(BlockContent::Code(commands.into_iter().collect()))
    }

    /// Creates a data block populated from `operands`.
    pub fn with_data(operands: impl IntoIterator<Item = Operand>) -> Self {
        Self::with_content(BlockContent::Data(operands.into_iter().collect()))
    }

    /// Returns true if there is nothing in the block.
    pub fn is_empty(&self) -> bool {
        match &self.content {
            None => true,
            Some(BlockContent::Code(c)) => c.is_empty(),
            Some(BlockContent::Data(d)) => d.is_empty(),
        }
    }

    /// Returns true if the block contains code.
    pub fn is_code(&self) -> bool {
        matches!(&self.content, Some(BlockContent::Data(_)))
    }

    /// Returns true if the block contains data.
    pub fn is_data(&self) -> bool {
        matches!(&self.content, Some(BlockContent::Data(_)))
    }
}

/// An assembly program.
#[derive(Default)]
pub struct Program {
    /// The blocks making up the program. Each block's ID is its index in this list.
    pub blocks: Vec<Block>,
    /// The ID of the first block in the program, or `None` if the program is empty.
    pub first_block: Option<BlockId>,
    /// Map of event types to block IDs.
    pub events: HashMap<Event, BlockId>,
    /// Label information.
    pub labels: LabelMap,
}

impl Program {
    /// Creates an empty program.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a program with blocks populated from `blocks`.
    pub fn with_blocks(blocks: impl Into<Vec<Block>>, first_block: Option<BlockId>) -> Self {
        Self { blocks: blocks.into(), first_block, ..Default::default() }
    }
}
