use super::collision::ObjectColliders;
use super::header::{FileHeader, HEADER_SIZE};
use super::{Libs, Metadata, Result};
use crate::common::{ReadFrom, ReadSeek, Region};
use std::io::{self, BufReader, Seek, SeekFrom, Write};

/// A stream for reading globals.bin data.
pub struct GlobalsReader<R: ReadSeek> {
    reader: R,
    header: FileHeader,
}

impl<R: ReadSeek> GlobalsReader<R> {
    /// Constructs a new `GlobalsReader<R>` which reads existing disc data from `stream`. This does
    /// its own buffering, so `stream` should not be buffered.
    pub fn open(reader: R) -> Result<Self> {
        let mut reader = BufReader::new(reader);
        reader.seek(SeekFrom::Start(0))?;
        let header = FileHeader::read_from(&mut reader)?;
        Ok(Self { reader: reader.into_inner(), header })
    }

    /// Returns a region open on the metadata partition.
    pub fn open_metadata(&mut self) -> Region<&mut R> {
        self.open_partition(self.header.metadata_offset, self.header.metadata_size)
    }

    /// Returns a region open on the colliders partition.
    pub fn open_colliders(&mut self) -> Region<&mut R> {
        self.open_partition(self.header.collision_offset, self.header.collision_size)
    }

    /// Returns a region open on the libs partition.
    pub fn open_libs(&mut self) -> Region<&mut R> {
        self.open_partition(self.header.libs_offset, self.header.libs_size)
    }

    /// Reads the metadata from the file.
    pub fn read_metadata(&mut self) -> Result<Metadata> {
        Metadata::read_from(&mut self.open_metadata())
    }

    /// Reads the collision data from the file.
    pub fn read_colliders(&mut self) -> Result<ObjectColliders> {
        ObjectColliders::read_from(&mut self.open_colliders())
    }

    /// Reads the script library functions from the file.
    pub fn read_libs(&mut self) -> Result<Libs> {
        Libs::read_from(&mut self.open_libs())
    }

    /// Unwraps this `GlobalsReader<R>`, returning the underlying stream.
    pub fn into_inner(self) -> R {
        self.reader
    }

    fn open_partition(&mut self, offset: u32, size: u32) -> Region<&mut R> {
        // Offsets are relative to the end of the header
        Region::new(&mut self.reader, (offset + HEADER_SIZE) as u64, size as u64)
    }
}

/// Internal trait used to implement `GlobalsBuilder`.
pub trait CopyGlobals: private::Sealed {
    /// Copies the metadata partition into `writer`.
    fn copy_metadata(&mut self, writer: &mut dyn Write) -> Result<()>;

    /// Copies the colliders partition into `writer`.
    fn copy_colliders(&mut self, writer: &mut dyn Write) -> Result<()>;

    /// Copies the libs partition into `writer`.
    fn copy_libs(&mut self, writer: &mut dyn Write) -> Result<()>;
}

impl<R: ReadSeek> CopyGlobals for GlobalsReader<R> {
    fn copy_metadata(&mut self, writer: &mut dyn Write) -> Result<()> {
        io::copy(&mut self.open_metadata(), writer)?;
        Ok(())
    }

    fn copy_colliders(&mut self, writer: &mut dyn Write) -> Result<()> {
        io::copy(&mut self.open_colliders(), writer)?;
        Ok(())
    }

    fn copy_libs(&mut self, writer: &mut dyn Write) -> Result<()> {
        io::copy(&mut self.open_libs(), writer)?;
        Ok(())
    }
}

mod private {
    use super::*;
    pub trait Sealed {}
    impl<R: ReadSeek> Sealed for GlobalsReader<R> {}
}
