use super::{Header, Result};
use crate::common::{ReadFrom, ReadSeek};
use crate::dvd::fst::{self, EntryId, FileStringTable, FileTree};
use crate::dvd::OpenFile;
use std::io::{BufReader, Read, Seek, SeekFrom};

/// A stream for reading U8 archives.
#[non_exhaustive]
pub struct ArchiveReader<R: Read + Seek> {
    pub reader: R,
    pub files: FileTree,
}

impl<R: Read + Seek> ArchiveReader<R> {
    /// Constructs a new `ArchiveReader` which reads existing data from `reader`.
    /// `ArchiveReader` does its own buffering, so `stream` should not be buffered.
    pub fn open(reader: R) -> Result<Self> {
        let mut buf = BufReader::new(reader);
        buf.seek(SeekFrom::Start(0))?;
        let header = Header::read_from(&mut buf)?;
        buf.seek(SeekFrom::Start(header.fst_offset as u64))?;
        let fst = FileStringTable::read_from(&mut (&mut buf).take(header.fst_size as u64))?;
        Ok(Self { reader: buf.into_inner(), files: FileTree::from_fst(&fst)? })
    }
}

impl<S: Read + Seek> OpenFile for ArchiveReader<S> {
    fn open_file<'s>(&'s mut self, id: EntryId) -> fst::Result<Box<dyn ReadSeek + 's>> {
        self.files.file(id)?.open(&mut self.reader)
    }

    fn open_file_at<'s>(&'s mut self, path: &str) -> fst::Result<Box<dyn ReadSeek + 's>> {
        self.files.file_at(path)?.open(&mut self.reader)
    }
}
