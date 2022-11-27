use crate::label::{LabelId, LabelMap};
use crate::lexer::Number;
use crate::opcodes::{AsmMsgOp, DirOp, NamedOpcode};
use crate::{Error, Result};
use bitflags::bitflags;
use num_traits::NumCast;
use smallvec::SmallVec;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt::{self, Display, Formatter};
use unplug::common::Text;
use unplug::event::opcodes::{CmdOp, ExprOp, Opcode, TypeOp};
use unplug::event::BlockId;
use unplug::stage::Event;

mod private {
    pub trait Sealed {}
}
use private::Sealed;

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

impl Operand {
    /// Casts the operand to an integer type. Returns the integer on success, and returns an
    /// appropriate error on failure.
    pub fn cast<T: CastOperand>(&self) -> Result<T> {
        let result = match *self {
            Operand::I8(x) => T::from(x).ok_or_else(|| Number::I8(x.into())),
            Operand::U8(x) => T::from(x).ok_or_else(|| Number::U8(x.into())),
            Operand::I16(x) => T::from(x).ok_or_else(|| Number::I16(x.into())),
            Operand::U16(x) => T::from(x).ok_or_else(|| Number::U16(x.into())),
            Operand::I32(x) => T::from(x).ok_or(Number::I32(x)),
            Operand::U32(x) => T::from(x).ok_or(Number::U32(x)),
            _ => return Err(Error::ExpectedInteger),
        };
        result.map_err(T::error)
    }

    /// If the operand is a `Label`, returns the label ID, otherwise returns an appropriate error.
    pub fn label(&self) -> Result<LabelId> {
        match *self {
            Operand::Label(id) => Ok(id),
            Operand::ElseLabel(_) => Err(Error::UnexpectedElseLabel),
            _ => Err(Error::ExpectedLabel),
        }
    }
}

// `From` implementations for `Operand`
macro_rules! impl_operand_from {
    ($type:ty, $variant:ident) => {
        impl From<$type> for Operand {
            fn from(x: $type) -> Self {
                Self::$variant(x.into())
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

/// Trait for a primitive type which an operand can be cast to.
pub trait CastOperand: NumCast + Sealed {
    /// Maps `num` to an error corresponding to this type.
    fn error(num: Number) -> Error;
}

// `CastOperand` implementations
macro_rules! impl_cast_operand {
    ($type:ty, $err:path) => {
        impl Sealed for $type {}
        impl CastOperand for $type {
            fn error(num: Number) -> Error {
                $err(num)
            }
        }
    };
}
impl_cast_operand!(i8, Error::CannotConvertToI8);
impl_cast_operand!(u8, Error::CannotConvertToU8);
impl_cast_operand!(i16, Error::CannotConvertToI16);
impl_cast_operand!(u16, Error::CannotConvertToU16);
impl_cast_operand!(i32, Error::CannotConvertToI32);
impl_cast_operand!(u32, Error::CannotConvertToU32);

/// Operand type hints.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum OperandType {
    /// The operand may be any type.
    Unknown,
    /// The operand must be a byte.
    Byte,
    /// The operand must be a word.
    Word,
    /// The operand must be a dword.
    Dword,
    /// The operand must be a message command.
    Message,
}

impl Display for OperandType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match *self {
            OperandType::Unknown => "unknown",
            OperandType::Byte => "byte",
            OperandType::Word => "word",
            OperandType::Dword => "dword",
            OperandType::Message => "message",
        })
    }
}

/// Trait for getting an operand type hint from an opcode.
pub trait TypeHint: Opcode {
    /// Returns the operand type hint corresponding to this opcode.
    fn type_hint(self) -> OperandType;
}

impl TypeHint for CmdOp {
    fn type_hint(self) -> OperandType {
        match self {
            CmdOp::Msg | CmdOp::Select => OperandType::Message,
            _ => OperandType::Unknown,
        }
    }
}

impl TypeHint for ExprOp {
    fn type_hint(self) -> OperandType {
        OperandType::Unknown
    }
}

impl TypeHint for DirOp {
    fn type_hint(self) -> OperandType {
        match self {
            DirOp::Byte => OperandType::Byte,
            DirOp::Word => OperandType::Word,
            DirOp::Dword => OperandType::Dword,
            _ => OperandType::Unknown,
        }
    }
}

impl TypeHint for AsmMsgOp {
    fn type_hint(self) -> OperandType {
        OperandType::Unknown
    }
}

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
        /// The block is associated with at least one entry point.
        const ENTRY_POINT = 1 << 1;
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
        matches!(&self.content, Some(BlockContent::Code(_)))
    }

    /// Returns true if the block contains data.
    pub fn is_data(&self) -> bool {
        matches!(&self.content, Some(BlockContent::Data(_)))
    }

    /// Appends a command to the end of a code block. If the block is empty, it will become a code
    /// block. ***Panics*** if the block already contains data.
    pub fn push_command(&mut self, command: Operation<CmdOp>) {
        match self.content.get_or_insert(BlockContent::Code(vec![])) {
            BlockContent::Code(c) => c.push(command),
            BlockContent::Data(_) => panic!("cannot append a command to a data block"),
        }
    }

    /// Appends data to the end of a data block. If the block is empty, it will become a data block.
    /// ***Panics*** if the block already contains code.
    pub fn push_data(&mut self, data: impl IntoIterator<Item = Operand>) {
        match self.content.get_or_insert(BlockContent::Data(vec![])) {
            BlockContent::Code(_) => panic!("cannot append a command to a data block"),
            BlockContent::Data(d) => d.extend(data),
        }
    }
}

/// A kind of entry point into a program.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(variant_size_differences)]
pub enum EntryPoint {
    /// A global library function.
    Lib(i16),
    /// An event in a stage file.
    Event(Event),
}

impl EntryPoint {
    /// Returns the opcode of the directive which declares the entry point.
    pub fn directive(self) -> DirOp {
        match self {
            Self::Lib(_) => DirOp::Lib,
            Self::Event(Event::Prologue) => DirOp::Prologue,
            Self::Event(Event::Startup) => DirOp::Startup,
            Self::Event(Event::Dead) => DirOp::Dead,
            Self::Event(Event::Pose) => DirOp::Pose,
            Self::Event(Event::TimeCycle) => DirOp::TimeCycle,
            Self::Event(Event::TimeUp) => DirOp::TimeUp,
            Self::Event(Event::Interact(_)) => DirOp::Interact,
        }
    }
}

/// A target specifier indicating the purpose of a script.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Target {
    /// The script defines global library functions.
    Globals,
    /// The script is for the stage with the given name (no extension).
    Stage(String),
}

/// An assembly program.
#[derive(Default)]
pub struct Program {
    /// An optional target specifier.
    pub target: Option<Target>,
    /// The blocks making up the program. Each block's ID is its index in this list.
    pub blocks: Vec<Block>,
    /// The ID of the first block in the program, or `None` if the program is empty.
    pub first_block: Option<BlockId>,
    /// Map of entry points to block IDs.
    pub entry_points: HashMap<EntryPoint, BlockId>,
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
        Self { target: None, blocks: blocks.into(), first_block, ..Default::default() }
    }

    /// Inserts `block` after `after_id` in program order and returns the new block's ID.
    /// If `after_id` is `None`, the block will be inserted at the beginning of the program.
    pub fn insert_after(&mut self, after_id: Option<BlockId>, mut block: Block) -> BlockId {
        let new_id = BlockId::new(self.blocks.len() as u32);
        let next = match after_id {
            Some(id) => id.get(&self.blocks).next,
            None => self.first_block,
        };
        block.next = next;
        self.blocks.push(block);
        match after_id {
            Some(id) => id.get_mut(&mut self.blocks).next = Some(new_id),
            None => self.first_block = Some(new_id),
        };
        new_id
    }

    /// Scans through commands in the program and marks subroutines and entry points with the
    /// correct block flags.
    pub fn mark_subroutines(&mut self) {
        for index in 0..self.blocks.len() {
            // We can't get a mutable reference to self.blocks inside the operand loop, so
            // split it into (before, block, after) instead
            let (before, right) = self.blocks.split_at_mut(index);
            let (block, after) = right.split_first_mut().unwrap();
            if let Some(BlockContent::Code(code)) = &block.content {
                for cmd in code {
                    // Assume any top-level label reference operand which is not part of a control
                    // flow command refers to a subroutine
                    if !cmd.operands.is_empty() && !cmd.opcode.is_control_flow() {
                        for operand in &cmd.operands {
                            if let Operand::Label(label) = operand {
                                let target_id = self.labels.get(*label).block.unwrap();
                                let target_index = target_id.index();
                                let flags = match target_index.cmp(&index) {
                                    Ordering::Less => &mut before[target_index].flags,
                                    Ordering::Equal => &mut block.flags,
                                    Ordering::Greater => &mut after[target_index - index - 1].flags,
                                };
                                flags.insert(BlockFlags::SUBROUTINE);
                            }
                        }
                    }
                }
            }
        }

        // Entry points are also subroutines
        for block_id in self.entry_points.values() {
            let block = block_id.get_mut(&mut self.blocks);
            block.flags.insert(BlockFlags::ENTRY_POINT | BlockFlags::SUBROUTINE);
        }
    }
}
