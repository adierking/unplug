use super::{ArchiveReader, Header, Result};
use crate::common::io::pad;
use crate::common::{ReadSeek, WriteTo};
use crate::dvd::fst::{self, EntryId, FileTree, FstEntryKind};
use crate::dvd::OpenFile;
use slotmap::SecondaryMap;
use std::cell::RefCell;
use std::convert::TryFrom;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::rc::Rc;
use tracing::trace;

const ARCHIVE_ALIGN: u64 = 0x20;

/// An `OpenFile` wrapper around a closure which returns a reader for a file.
struct Opener<R, F>(Option<F>)
where
    R: ReadSeek,
    F: FnOnce() -> R;

impl<R, F> OpenFile for Opener<R, F>
where
    R: ReadSeek,
    F: FnOnce() -> R,
{
    fn open_file<'s>(&'s mut self, _id: EntryId) -> fst::Result<Box<dyn ReadSeek + 's>> {
        let func = self.0.take().unwrap();
        Ok(Box::new(func()))
    }

    fn open_file_at<'s>(&'s mut self, _path: &str) -> fst::Result<Box<dyn ReadSeek + 's>> {
        let func = self.0.take().unwrap();
        Ok(Box::new(func()))
    }
}

/// Builds archive files.
pub struct ArchiveBuilder<'a> {
    /// The files to put in the archive.
    files: FileTree,
    /// Maps entries in the tree to `OpenFile` implementations which open readers for them.
    sources: SecondaryMap<EntryId, Rc<RefCell<Box<dyn OpenFile + 'a>>>>,
}

impl<'a> ArchiveBuilder<'a> {
    /// Constructs a new `ArchiveBuilder`.
    pub fn new() -> Self {
        Self { files: FileTree::new(), sources: SecondaryMap::new() }
    }

    /// Constructs a new `ArchiveBuilder` which imports files from `archive`.
    pub fn with_archive<S: ReadSeek>(archive: &'a mut ArchiveReader<S>) -> Self {
        let files = archive.files.clone();
        let mut sources = SecondaryMap::new();
        // OpenFile is implemented for &mut T
        let boxed: Box<dyn OpenFile> = Box::new(archive);
        let source = Rc::new(RefCell::new(boxed));
        for entry in files.entries.keys() {
            sources.insert(entry, source.clone());
        }
        Self { files, sources }
    }

    /// Returns a reference to the archive's `FileTree`.
    pub fn files(&self) -> &FileTree {
        &self.files
    }

    /// Registers `opener` as the data source for `entry`. When the archive is written out, `opener`
    /// will be called to get a reader for the file's data.
    pub fn replace<'s, R, F>(&'s mut self, entry: EntryId, opener: F) -> &'s mut Self
    where
        R: ReadSeek + 'a,
        F: (FnOnce() -> R) + 'a,
    {
        let boxed: Box<dyn OpenFile> = Box::new(Opener(Some(opener)));
        self.sources.insert(entry, Rc::new(RefCell::new(boxed)));
        self
    }

    /// Registers `opener` as the data source for the file at `path`. When the archive is written
    /// out, `opener` will be called to get a reader for the file's data.
    pub fn replace_at<'s, R, F>(&'s mut self, path: &str, opener: F) -> Result<&'s mut Self>
    where
        R: ReadSeek + 'a,
        F: (FnOnce() -> R) + 'a,
    {
        Ok(self.replace(self.files.at(path)?, opener))
    }

    /// Writes out an archive file.
    pub fn write_to(&self, mut writer: (impl Write + Seek)) -> Result<()> {
        // Write an empty header we can fill in later
        writer.seek(SeekFrom::Start(0))?;
        let mut buf = BufWriter::new(&mut writer);
        let mut header = Header::new();
        header.write_to(&mut buf)?;

        // Write an incomplete FST. We'll fill in the correct offsets and sizes at the end.
        header.fst_offset = buf.seek(SeekFrom::Current(0))? as u32;
        let (mut fst, ids) = self.files.to_fst()?;
        fst.write_to(&mut buf)?;
        header.fst_size = buf.seek(SeekFrom::Current(0))? as u32 - header.fst_offset;

        buf.flush()?;
        drop(buf);
        pad(&mut writer, ARCHIVE_ALIGN, 0)?;
        header.data_offset =
            u32::try_from(writer.seek(SeekFrom::Current(0))?).expect("File size overflow");

        for (entry, id) in fst.entries.iter_mut().zip(ids) {
            if entry.kind != FstEntryKind::File {
                continue;
            }

            pad(&mut writer, ARCHIVE_ALIGN, 0)?;
            let start_offset =
                u32::try_from(writer.seek(SeekFrom::Current(0))?).expect("File size overflow");

            trace!("Writing archive entry \"{}\" at {:#x}", self.files[id].name(), start_offset);
            let mut source = self.sources[id].borrow_mut();
            let mut reader = source.open_file(id)?;
            std::io::copy(reader.as_mut(), &mut writer)?;

            let end_offset =
                u32::try_from(writer.seek(SeekFrom::Current(0))?).expect("File size overflow");
            entry.offset_or_parent = start_offset;
            entry.size_or_next = end_offset - start_offset;
        }

        // Go back and fill in the header
        writer.seek(SeekFrom::Start(0))?;
        let mut buf = BufWriter::new(&mut writer);
        header.write_to(&mut buf)?;
        fst.write_to(&mut buf)?;
        buf.flush()?;
        Ok(())
    }
}

impl Default for ArchiveBuilder<'_> {
    fn default() -> Self {
        Self::new()
    }
}
