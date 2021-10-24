use super::{Error, Result};
use crate::common::{ReadFrom, WriteTo};
use byteorder::{ReadBytesExt, WriteBytesExt, LE};
use std::io::{Read, Write};

pub(super) const HEADER_SIZE: u32 = 0x18;

/// The header of a globals.bin file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct FileHeader {
    /// Offset of the metadata partition.
    pub(super) metadata_offset: u32,
    /// Size of the metadata partition.
    pub(super) metadata_size: u32,
    /// Offset of the collision partition.
    pub(super) collision_offset: u32,
    /// Size of the collision partition.
    pub(super) collision_size: u32,
    /// Offset of the libs partition.
    pub(super) libs_offset: u32,
    /// Size of the libs partition.
    pub(super) libs_size: u32,
}

impl FileHeader {
    pub(super) fn set_metadata(&mut self, start: u32, end: u32) {
        self.metadata_offset = start - HEADER_SIZE;
        self.metadata_size = end - start;
    }

    pub(super) fn set_collision(&mut self, start: u32, end: u32) {
        self.collision_offset = start - HEADER_SIZE;
        self.collision_size = end - start;
    }

    pub(super) fn set_libs(&mut self, start: u32, end: u32) {
        self.libs_offset = start - HEADER_SIZE;
        self.libs_size = end - start;
    }
}

impl<R: Read + ?Sized> ReadFrom<R> for FileHeader {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let metadata_offset = reader.read_u32::<LE>()?;
        // Can we reliably check anything else here?
        if metadata_offset != 0 {
            return Err(Error::InvalidHeader);
        }
        Ok(Self {
            metadata_offset,
            metadata_size: reader.read_u32::<LE>()?,
            collision_offset: reader.read_u32::<LE>()?,
            collision_size: reader.read_u32::<LE>()?,
            libs_offset: reader.read_u32::<LE>()?,
            libs_size: reader.read_u32::<LE>()?,
        })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for FileHeader {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_u32::<LE>(self.metadata_offset)?;
        writer.write_u32::<LE>(self.metadata_size)?;
        writer.write_u32::<LE>(self.collision_offset)?;
        writer.write_u32::<LE>(self.collision_size)?;
        writer.write_u32::<LE>(self.libs_offset)?;
        writer.write_u32::<LE>(self.libs_size)?;
        Ok(())
    }
}
