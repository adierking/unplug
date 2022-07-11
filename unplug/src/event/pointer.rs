use crate::common::{ReadFrom, WriteTo};
use byteorder::{ReadBytesExt, LE};
use std::convert::TryFrom;
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

/// A pointer which can be read as a file offset and then resolved to a block ID.
#[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum Pointer {
    Offset(u32),
    Block(BlockId),
}

impl Pointer {
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

    /// Returns `true` if this points to offset 0. Note that these are not necessarily null
    /// pointers; for example, sometimes script code will use an offset of 0 to get the base
    /// address for the script.
    pub fn is_zero(&self) -> bool {
        matches!(self, Self::Offset(0))
    }
}

impl From<u32> for Pointer {
    fn from(offset: u32) -> Self {
        Self::Offset(offset)
    }
}

impl From<NonZeroU32> for Pointer {
    fn from(offset: NonZeroU32) -> Self {
        Self::Offset(offset.get())
    }
}

impl From<BlockId> for Pointer {
    fn from(id: BlockId) -> Self {
        Self::Block(id)
    }
}

impl fmt::Debug for Pointer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Offset(offset) => write!(f, "Offset({:#x})", offset),
            Self::Block(id) => write!(f, "Block({:?})", id),
        }
    }
}

impl<R: Read + ?Sized> ReadFrom<R> for Pointer {
    type Error = io::Error;
    fn read_from(reader: &mut R) -> io::Result<Self> {
        Ok(reader.read_u32::<LE>()?.into())
    }
}

impl<W: Write + WritePointer + ?Sized> WriteTo<W> for Pointer {
    type Error = io::Error;
    fn write_to(&self, writer: &mut W) -> io::Result<()> {
        writer.write_pointer(*self)
    }
}

/// A trait for a stream that can write a `Pointer` even if the offset is not known yet.
/// This permits the writer to write a placeholder and then fill the actual offset in later.
pub trait WritePointer {
    /// Writes a pointer which will be fixed up later.
    fn write_pointer(&mut self, ptr: Pointer) -> io::Result<()>;

    /// Writes an offset computed relative to the current offset.
    fn write_rel_offset(&mut self, offset: i32) -> io::Result<()>;
}

// Blanket implementation for mutable references
impl<W: WritePointer> WritePointer for &mut W {
    fn write_pointer(&mut self, ptr: Pointer) -> io::Result<()> {
        (**self).write_pointer(ptr)
    }

    fn write_rel_offset(&mut self, offset: i32) -> io::Result<()> {
        (**self).write_rel_offset(offset)
    }
}
