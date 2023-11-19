mod builder;
mod reader;

pub use builder::ArchiveBuilder;
pub use reader::ArchiveReader;

use super::fst;
use crate::common::{ReadFrom, WriteTo};
use byteorder::{ReadBytesExt, WriteBytesExt, BE};
use std::io::{self, Read, Write};
use thiserror::Error;

const ARC_MAGIC: u32 = 0x55aa382d;
const ARC_RESERVED: u8 = 0xcc;

/// The result type for archive operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for archive operations.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("invalid archive magic")]
    InvalidMagic,

    #[error(transparent)]
    Fst(Box<fst::Error>),

    #[error(transparent)]
    Io(Box<io::Error>),
}

from_error_boxed!(Error::Fst, fst::Error);
from_error_boxed!(Error::Io, io::Error);

#[derive(Debug, Copy, Clone)]
struct Header {
    magic: u32,
    fst_offset: u32,
    fst_size: u32,
    data_offset: u32,
    reserved: [u8; 16],
}

impl Header {
    fn new() -> Self {
        Self {
            magic: ARC_MAGIC,
            fst_offset: 0,
            fst_size: 0,
            data_offset: 0,
            reserved: [ARC_RESERVED; 16],
        }
    }
}

impl<R: Read + ?Sized> ReadFrom<R> for Header {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let magic = reader.read_u32::<BE>()?;
        if magic != ARC_MAGIC {
            return Err(Error::InvalidMagic);
        }
        let fst_offset = reader.read_u32::<BE>()?;
        let fst_size = reader.read_u32::<BE>()?;
        let data_offset = reader.read_u32::<BE>()?;
        let mut reserved = [0u8; 16];
        reader.read_exact(&mut reserved)?;
        Ok(Self { magic, fst_offset, fst_size, data_offset, reserved })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for Header {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_u32::<BE>(self.magic)?;
        writer.write_u32::<BE>(self.fst_offset)?;
        writer.write_u32::<BE>(self.fst_size)?;
        writer.write_u32::<BE>(self.data_offset)?;
        writer.write_all(&self.reserved)?;
        Ok(())
    }
}
