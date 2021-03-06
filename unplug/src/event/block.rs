use super::expr::{ObjBone, ObjPair};
use super::Command;
use crate::common::{ReadFrom, WriteTo};
use byteorder::{ReadBytesExt, LE};
use std::convert::TryFrom;
use std::ffi::CString;
use std::fmt;
use std::io::{self, Read, Write};
use std::num::{NonZeroU32, TryFromIntError};

/// The offset of the object table in stage files.
const STAGE_OBJECTS_OFFSET: u32 = 0x48;

/// Refers to a specific code block in an event.
#[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockId {
    // Stored as index + 1 so it doesn't take up space in an Optional
    id: NonZeroU32,
}

impl BlockId {
    /// Constructs an ID from an index into a block list.
    pub fn new(index: u32) -> Self {
        Self { id: NonZeroU32::new(index + 1).expect("Invalid block index") }
    }

    /// Gets the block list index corresponding to this ID.
    pub fn index(&self) -> usize {
        self.id.get() as usize - 1
    }

    /// Uses this ID to get a reference to an object in a slice.
    pub fn get<'a, T>(&self, slice: &'a [T]) -> &'a T {
        &slice[self.index()]
    }

    /// Uses this ID to get a mutable reference to an object in a slice.
    pub fn get_mut<'a, T>(&self, slice: &'a mut [T]) -> &'a mut T {
        &mut slice[self.index()]
    }
}

impl fmt::Debug for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.index())
    }
}

impl TryFrom<usize> for BlockId {
    type Error = TryFromIntError;
    fn try_from(index: usize) -> Result<Self, Self::Error> {
        Ok(Self::new(u32::try_from(index)?))
    }
}

/// An instruction pointer which can be read as a file offset and then resolved to a block ID.
#[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ip {
    Offset(u32),
    Block(BlockId),
}

impl Ip {
    /// If this is an offset, retrieve it.
    pub fn offset(&self) -> Option<u32> {
        match self {
            Self::Offset(o) => Some(*o),
            _ => None,
        }
    }

    /// If this is a reference to a block, retrieve the `BlockId`.
    pub fn block(&self) -> Option<BlockId> {
        match self {
            Self::Block(id) => Some(*id),
            _ => None,
        }
    }

    /// Returns `true` if this points inside the stage header.
    pub fn is_in_header(&self) -> bool {
        match self {
            Self::Offset(o) => (*o <= STAGE_OBJECTS_OFFSET),
            _ => false,
        }
    }
}

impl From<u32> for Ip {
    fn from(offset: u32) -> Self {
        Self::Offset(offset)
    }
}

impl From<NonZeroU32> for Ip {
    fn from(offset: NonZeroU32) -> Self {
        Self::Offset(offset.get())
    }
}

impl From<BlockId> for Ip {
    fn from(id: BlockId) -> Self {
        Self::Block(id)
    }
}

impl fmt::Debug for Ip {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Offset(offset) => write!(f, "Offset({:#x})", offset),
            Self::Block(id) => write!(f, "Block({:?})", id),
        }
    }
}

impl<R: Read> ReadFrom<R> for Ip {
    type Error = io::Error;
    fn read_from(reader: &mut R) -> io::Result<Self> {
        Ok(reader.read_u32::<LE>()?.into())
    }
}

impl<W: Write + WriteIp> WriteTo<W> for Ip {
    type Error = io::Error;
    fn write_to(&self, writer: &mut W) -> io::Result<()> {
        writer.write_ip(*self)
    }
}

/// A trait for an object that can write an `Ip`.
/// This permits the writer to write a placeholder and then fill the actual IP in later.
pub trait WriteIp {
    fn write_ip(&mut self, ip: Ip) -> io::Result<()>;
}

/// A block of data in a script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Placeholder,
    Code(CodeBlock),
    Data(DataBlock),
}

impl Block {
    /// Returns whether this block is a placeholder.
    pub fn is_placeholder(&self) -> bool {
        matches!(self, Self::Placeholder)
    }

    /// Returns whether this block is a code block.
    pub fn is_code(&self) -> bool {
        self.code().is_some()
    }

    /// If this block is a code block, returns a reference to the `CodeBlock`.
    pub fn code(&self) -> Option<&CodeBlock> {
        match self {
            Self::Code(c) => Some(c),
            _ => None,
        }
    }

    /// If this block is a code block, returns a mutable reference to the `CodeBlock`.
    pub fn code_mut(&mut self) -> Option<&mut CodeBlock> {
        match self {
            Self::Code(c) => Some(c),
            _ => None,
        }
    }

    /// If this block is a code block, returns a reference to its commands.
    pub fn commands(&self) -> Option<&[Command]> {
        match self {
            Self::Code(c) => Some(&c.commands),
            _ => None,
        }
    }

    /// If this block is a code block, returns a mutable reference to its commands.
    pub fn commands_mut(&mut self) -> Option<&mut [Command]> {
        match self {
            Self::Code(c) => Some(&mut c.commands),
            _ => None,
        }
    }

    /// Returns whether this block is a data block.
    pub fn is_data(&self) -> bool {
        self.data().is_some()
    }

    /// If this block is a data block, returns a reference to the `DataBlock`.
    pub fn data(&self) -> Option<&DataBlock> {
        match self {
            Self::Data(d) => Some(d),
            _ => None,
        }
    }
}

impl Default for Block {
    fn default() -> Self {
        Self::Placeholder
    }
}

impl From<CodeBlock> for Block {
    fn from(code: CodeBlock) -> Self {
        Self::Code(code)
    }
}

impl From<DataBlock> for Block {
    fn from(data: DataBlock) -> Self {
        Self::Data(data)
    }
}

/// A [basic block] of commands in an event which has single points of entry and exit.
///
/// [basic block]: https://en.wikipedia.org/wiki/Basic_block
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodeBlock {
    /// The list of commands in the block.
    pub commands: Vec<Command>,
    /// The ID of the block to jump to after this block, if any.
    pub next_block: Option<Ip>,
    /// The ID of the block to jump to if the block's condition fails, if any.
    pub else_block: Option<Ip>,
}

impl CodeBlock {
    /// Constructs an empty code block.
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataBlock {
    ArrayI8(Vec<i8>),
    ArrayU8(Vec<u8>),
    ArrayI16(Vec<i16>),
    ArrayU16(Vec<u16>),
    ArrayI32(Vec<i32>),
    ArrayU32(Vec<u32>),
    ArrayIp(Vec<Ip>),
    ObjBone(ObjBone),
    ObjPair(ObjPair),
    String(CString),
}

macro_rules! impl_data_block_from {
    ($type:ty, $name:ident) => {
        impl From<$type> for DataBlock {
            fn from(val: $type) -> Self {
                Self::$name(val)
            }
        }
    };
}

impl_data_block_from!(Vec<i8>, ArrayI8);
impl_data_block_from!(Vec<u8>, ArrayU8);
impl_data_block_from!(Vec<i16>, ArrayI16);
impl_data_block_from!(Vec<u16>, ArrayU16);
impl_data_block_from!(Vec<i32>, ArrayI32);
impl_data_block_from!(Vec<u32>, ArrayU32);
impl_data_block_from!(Vec<Ip>, ArrayIp);
impl_data_block_from!(ObjBone, ObjBone);
impl_data_block_from!(ObjPair, ObjPair);
impl_data_block_from!(CString, String);
