use crate::common::{
    string_table, ReadFrom, ReadSeek, ReadWriteSeek, Region, StringTable, WriteTo,
};
use byteorder::{ReadBytesExt, WriteBytesExt, BE};
use encoding_rs::SHIFT_JIS;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use slotmap::{new_key_type, DenseSlotMap};
use std::convert::TryFrom;
use std::ffi::CString;
use std::io::{self, Read, Write};
use std::iter::FusedIterator;
use std::mem;
use std::ops::{Index, IndexMut};
use thiserror::Error;
use tracing::{debug, trace};

new_key_type! {
    /// A unique ID for a `FileTree` entry.
    pub struct EntryId;
}

const FST_ENTRY_SIZE: usize = 12;

/// The result type for FST operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for FST operations.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("entry name could not be encoded as SHIFT-JIS: {0:?}")]
    Encode(String),

    #[error("entry name could not be decoded with SHIFT-JIS at {0:#x}")]
    Decode(u32),

    #[error("entry name is unsafe: {0:?}")]
    UnsafeName(String),

    #[error("root entry is not a directory")]
    RootIsNotDirectory,

    #[error("unrecognized entry kind: {0}")]
    UnrecognizedKind(u8),

    #[error("invalid directory next index: {0}")]
    InvalidNextIndex(u32),

    #[error("entry {0:?} is not a file")]
    ExpectedFile(EntryId),

    #[error("entry at {0:?} is not a file")]
    ExpectedFileAt(String),

    #[error("entry {0:?} is not a directory")]
    ExpectedDirectory(EntryId),

    #[error("entry {0:?} was not found")]
    NotFound(String),

    #[error(transparent)]
    Io(Box<io::Error>),

    #[error(transparent)]
    StringTable(Box<string_table::Error>),
}

from_error_boxed!(Error::Io, io::Error);
from_error_boxed!(Error::StringTable, string_table::Error);

/// Returns true if a name is safe from directory traversal attacks.
fn is_name_safe(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(|c| c == '/' || c == '\\' || c == ':')
}

/// A file string table (FST) in a GameCube filesystem.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct FileStringTable {
    pub entries: Vec<FstEntry>,
    pub strings: StringTable,
}

impl FileStringTable {
    /// Constructs an empty `FileStringTable`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Decodes the entry name at `offset`.
    pub fn decode_name(&self, offset: u32) -> Result<String> {
        let raw = self.strings.at(offset)?;
        let name = SHIFT_JIS
            .decode_without_bom_handling_and_without_replacement(raw.to_bytes())
            .ok_or(Error::Decode(offset))?;
        if is_name_safe(&name) {
            Ok(name.into_owned())
        } else {
            Err(Error::UnsafeName(name.into_owned()))
        }
    }

    /// Encodes an entry name, appends it to the string table, and returns its offset.
    pub fn encode_name(&mut self, name: &str) -> Result<u32> {
        let (bytes, _, unmappable) = SHIFT_JIS.encode(name);
        if unmappable {
            Err(Error::Encode(name.to_owned()))
        } else {
            Ok(u32::try_from(self.strings.push(CString::new(bytes).unwrap()))
                .expect("String offset overflow"))
        }
    }

    /// Returns the number of bytes that the FST will occupy when it is written to disk.
    pub fn disk_size(&self) -> u32 {
        (self.entries.len() * FST_ENTRY_SIZE + self.strings.as_bytes().len()) as u32
    }
}

impl<R: Read + ?Sized> ReadFrom<R> for FileStringTable {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        // Read the root entry first to figure out how many entries are in the FST
        let root = FstEntry::read_from(reader)?;
        if root.kind != FstEntryKind::Directory {
            return Err(Error::RootIsNotDirectory);
        }

        let num_entries = root.size_or_next;
        let mut entries = Vec::with_capacity(num_entries as usize);
        entries.push(root);
        for _ in 1..num_entries {
            entries.push(FstEntry::read_from(reader)?);
        }
        debug!("Read {} FST entries", num_entries);

        // The string table follows the last entry and extends up to the end of the FST
        let mut string_bytes = vec![];
        reader.read_to_end(&mut string_bytes)?;
        Ok(Self { entries, strings: StringTable::from_bytes(string_bytes)? })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for FileStringTable {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        for entry in &self.entries {
            entry.write_to(writer)?;
        }
        writer.write_all(self.strings.as_bytes())?;
        Ok(())
    }
}

/// FST entry kinds.
#[derive(Debug, Copy, Clone, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum FstEntryKind {
    File = 0,
    Directory = 1,
}

/// An FST entry.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct FstEntry {
    /// The entry's kind.
    pub kind: FstEntryKind,
    /// The offset of the entry's name in the string table.
    pub name_offset: u32,
    /// For a file, the data offset, and for a directory, the parent directory index.
    pub offset_or_parent: u32,
    /// For a file, the data size, and for a directory, the index of the next entry in the parent.
    pub size_or_next: u32,
}

impl<R: Read + ?Sized> ReadFrom<R> for FstEntry {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let kind = reader.read_u8()?;
        Ok(Self {
            kind: FstEntryKind::try_from(kind).map_err(|_| Error::UnrecognizedKind(kind))?,
            name_offset: reader.read_u24::<BE>()?,
            offset_or_parent: reader.read_u32::<BE>()?,
            size_or_next: reader.read_u32::<BE>()?,
        })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for FstEntry {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_u8(self.kind.into())?;
        writer.write_u24::<BE>(self.name_offset)?;
        writer.write_u32::<BE>(self.offset_or_parent)?;
        writer.write_u32::<BE>(self.size_or_next)?;
        Ok(())
    }
}

/// An entry in a `FileTree`.
#[derive(Debug, Clone)]
pub enum Entry {
    File(FileEntry),
    Directory(DirectoryEntry),
}

impl Entry {
    /// Returns the entry's name.
    pub fn name(&self) -> &str {
        match self {
            Self::File(file) => &file.name,
            Self::Directory(dir) => &dir.name,
        }
    }

    /// If the entry is a file, returns a reference to its `FileEntry`.
    pub fn file(&self) -> Option<&FileEntry> {
        match self {
            Self::File(f) => Some(f),
            Self::Directory(_) => None,
        }
    }

    /// If the entry is a file, returns a mutable reference to its `FileEntry`.
    pub fn file_mut(&mut self) -> Option<&mut FileEntry> {
        match self {
            Self::File(f) => Some(f),
            Self::Directory(_) => None,
        }
    }

    /// If the entry is a directory, returns a reference to its `DirectoryEntry`.
    pub fn dir(&self) -> Option<&DirectoryEntry> {
        match self {
            Self::File(_) => None,
            Self::Directory(d) => Some(d),
        }
    }

    /// If the entry is a directory, returns a mutable reference to its `DirectoryEntry`.
    pub fn dir_mut(&mut self) -> Option<&mut DirectoryEntry> {
        match self {
            Self::File(_) => None,
            Self::Directory(d) => Some(d),
        }
    }

    /// Returns true if the entry is a file.
    pub fn is_file(&self) -> bool {
        self.file().is_some()
    }

    /// Returns true if the entry is a directory.
    pub fn is_dir(&self) -> bool {
        self.dir().is_some()
    }
}

impl From<FileEntry> for Entry {
    fn from(file: FileEntry) -> Self {
        Self::File(file)
    }
}

impl From<DirectoryEntry> for Entry {
    fn from(dir: DirectoryEntry) -> Self {
        Self::Directory(dir)
    }
}

/// A file in a `FileTree`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// The file's name.
    pub name: String,
    /// The offset of the file's data inside the containing storage.
    pub offset: u32,
    /// The size of the file's data.
    pub size: u32,
}

impl FileEntry {
    /// Constructs a new `FileEntry`.
    pub fn new(name: impl Into<String>, offset: u32, size: u32) -> Self {
        Self { name: name.into(), offset, size }
    }

    /// Returns a wrapper around a reader which reads the contents of this file.
    pub fn open<'a>(&self, reader: (impl ReadSeek + 'a)) -> Result<Box<dyn ReadSeek + 'a>> {
        trace!("Opening entry \"{}\" at {:#x} (size = {:#x})", self.name, self.offset, self.size);
        Ok(Box::new(Region::new(reader, self.offset as u64, self.size as u64)))
    }

    /// Returns a wrapper around a stream which can read and write the contents of this file
    /// in-place. The strean will be permitted to grow up to `max_size`. It is the caller's
    /// responsibility to change the file size after writing.
    pub fn edit<'a>(
        &self,
        stream: (impl ReadWriteSeek + 'a),
        max_size: u32,
    ) -> Box<dyn ReadWriteSeek + 'a> {
        trace!(
            "Opening entry \"{}\" at {:#x} (size = {:#x}, max size = {:#x})",
            self.name,
            self.offset,
            self.size,
            max_size,
        );
        Box::new(Region::with_max_len(
            stream,
            self.offset as u64,
            self.size as u64,
            max_size as u64,
        ))
    }

    /// Constructs a `FileEntry` from an FST entry.
    fn from_fst(entry: FstEntry, fst: &FileStringTable) -> Result<Self> {
        Ok(Self {
            name: fst.decode_name(entry.name_offset)?,
            offset: entry.offset_or_parent,
            size: entry.size_or_next,
        })
    }
}

/// A directory in a `FileTree`.
#[derive(Debug, Clone)]
pub struct DirectoryEntry {
    /// The directory's name.
    pub name: String,
    /// The IDs of entries inside the directory.
    pub children: Vec<EntryId>,
}

impl DirectoryEntry {
    /// Constructs an empty `DirectoryEntry`.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), children: vec![] }
    }

    /// Constructs a `DirectoryEntry` from an FST entry.
    fn from_fst(
        entry: FstEntry,
        index: usize,
        fst: &FileStringTable,
        entry_map: &mut DenseSlotMap<EntryId, Entry>,
    ) -> Result<Self> {
        let name = if index > 0 {
            fst.decode_name(entry.name_offset)?
        } else {
            // The root entry isn't guaranteed to have a valid string offset
            "".into()
        };

        // The entries following the directory up to the "next" entry are descendants of it.
        let start_index = index + 1;
        let end_index = entry.size_or_next as usize;
        if end_index > fst.entries.len() || start_index > end_index {
            return Err(Error::InvalidNextIndex(entry.size_or_next));
        }

        let num_children = end_index - index;
        let mut children = Vec::with_capacity(num_children);
        let mut i = start_index;
        while i < end_index {
            let entry = &fst.entries[i];
            let child = match entry.kind {
                FstEntryKind::File => {
                    i += 1;
                    entry_map.insert(FileEntry::from_fst(*entry, fst)?.into())
                }
                FstEntryKind::Directory => {
                    // Recurse into the directory and then jump past it
                    let index = i;
                    i = entry.size_or_next as usize;
                    let node = DirectoryEntry::from_fst(*entry, index, fst, entry_map)?;
                    entry_map.insert(node.into())
                }
            };
            children.push(child);
        }
        Ok(Self { name, children })
    }
}

/// An iterator which recurses through descendants of a directory.
pub struct TreeRecurse<'a> {
    files: &'a FileTree,
    /// The path of the current directory, not ending with a separator.
    path: String,
    /// The current iterator in the current directory.
    iter: std::slice::Iter<'a, EntryId>,
    /// The iterators for parent directories.
    stack: Vec<std::slice::Iter<'a, EntryId>>,
}

impl<'a> TreeRecurse<'a> {
    /// Constructs a new `FstRecurse` which recurses from `root`.
    fn new(files: &'a FileTree, root: &'a DirectoryEntry) -> Self {
        Self { files, path: "".into(), iter: root.children.iter(), stack: Vec::new() }
    }
}

impl Iterator for TreeRecurse<'_> {
    type Item = (String, EntryId);

    fn next(&mut self) -> Option<Self::Item> {
        // Check the next file in this directory
        let next = self.iter.next();
        match next {
            Some(&entry) => {
                let path = if self.path.is_empty() {
                    // Entry in the root directory
                    self.files[entry].name().into()
                } else {
                    format!("{}/{}", self.path, self.files[entry].name())
                };
                match &self.files[entry] {
                    Entry::File(_) => Some((path, entry)),
                    Entry::Directory(dir) => {
                        // Return the directory and descend into it
                        self.path = path;
                        let iter = mem::replace(&mut self.iter, dir.children.iter());
                        self.stack.push(iter);
                        Some((self.path.clone(), entry))
                    }
                }
            }
            None => {
                // End of directory
                if let Some(iter) = self.stack.pop() {
                    // Go up a level and try again
                    if let Some(index) = self.path.rfind('/') {
                        self.path.truncate(index);
                    } else {
                        self.path.clear();
                    }
                    self.iter = iter;
                    self.next()
                } else {
                    // End of file table
                    None
                }
            }
        }
    }
}

impl FusedIterator for TreeRecurse<'_> {}

/// A tree of files which corresponds to an FST.
#[derive(Clone)]
pub struct FileTree {
    /// The entries in the table.
    pub entries: DenseSlotMap<EntryId, Entry>,
    /// The ID of the root directory.
    root: EntryId,
}

impl FileTree {
    /// Constructs a new `FileTree`.
    pub fn new() -> Self {
        let mut entries = DenseSlotMap::with_key();
        let root = entries.insert(DirectoryEntry::new("").into());
        Self { entries, root }
    }

    /// Constructs a `FileTree` from the contents of `fst`.
    pub fn from_fst(fst: &FileStringTable) -> Result<Self> {
        let mut entry_map = DenseSlotMap::with_capacity_and_key(fst.entries.len());
        let root_node = DirectoryEntry::from_fst(fst.entries[0], 0, fst, &mut entry_map)?;
        let root_id = entry_map.insert(root_node.into());
        Ok(Self { entries: entry_map, root: root_id })
    }

    /// Returns the ID of the root directory.
    pub fn root(&self) -> EntryId {
        self.root
    }

    /// Returns an iterator that recurses through the tree.
    pub fn recurse(&self) -> TreeRecurse<'_> {
        TreeRecurse::new(self, self[self.root].dir().unwrap())
    }

    /// Returns a reference to the `FileEntry` corresponding to `id`.
    pub fn file(&self, id: EntryId) -> Result<&FileEntry> {
        match &self[id] {
            Entry::File(f) => Ok(f),
            Entry::Directory(_) => Err(Error::ExpectedFile(id)),
        }
    }

    /// Returns a mutable reference to the `FileEntry` corresponding to `id`.
    pub fn file_mut(&mut self, id: EntryId) -> Result<&mut FileEntry> {
        match &mut self[id] {
            Entry::File(f) => Ok(f),
            Entry::Directory(_) => Err(Error::ExpectedFile(id)),
        }
    }

    /// Returns a reference to the `FileEntry` at `path`.
    pub fn file_at(&self, path: &str) -> Result<&FileEntry> {
        match &self[self.at(path)?] {
            Entry::File(file) => Ok(file),
            Entry::Directory(_) => Err(Error::ExpectedFileAt(path.to_owned())),
        }
    }

    /// Returns the ID of the entry at `path`.
    pub fn at(&self, path: &str) -> Result<EntryId> {
        self.at_descendant(self.root, path)
    }

    /// Returns the ID of the entry at `path`, interpreted relative to the `entry` directory.
    pub fn at_descendant(&self, entry: EntryId, path: &str) -> Result<EntryId> {
        let mut cur_entry = entry;
        let mut cur_dir = match &self[cur_entry] {
            Entry::File(_) => return Err(Error::ExpectedDirectory(cur_entry)),
            Entry::Directory(d) => d,
        };
        let mut components = path.split(|c| c == '/' || c == '\\');
        let mut stack: Vec<EntryId> = Vec::new();
        loop {
            // Find the first non-empty and non-"." component. If this is the last component, return
            // the current directory.
            let component = match components.find(|&c| !c.is_empty() && c != ".") {
                Some(c) => c,
                None => return Ok(cur_entry),
            };

            // If this is "..", try to pop the directory stack
            if component == ".." {
                if let Some(entry) = stack.pop() {
                    cur_entry = entry;
                    cur_dir = self[entry].dir().unwrap();
                    continue;
                }
                break;
            }

            // We have a child name - search for it in the current directory
            let entry = cur_dir.children.iter().find(|&&e| self[e].name() == component);
            let entry = match entry {
                Some(&e) => e,
                None => break,
            };
            match &self[entry] {
                Entry::File(_) => {
                    // Only return the file if this is the last component
                    if components.next().is_none() {
                        return Ok(entry);
                    }
                    break;
                }
                Entry::Directory(d) => {
                    // Enter the directory
                    stack.push(cur_entry);
                    cur_entry = entry;
                    cur_dir = d;
                }
            }
        }
        Err(Error::NotFound(path.to_owned()))
    }

    /// Inserts `entry` as a child of the `parent_id` directory and returns its ID.
    pub fn insert(&mut self, parent_id: EntryId, entry: Entry) -> EntryId {
        let id = self.entries.insert(entry);
        let parent = match self.entries[parent_id].dir_mut() {
            Some(d) => d,
            _ => panic!("Entry {:?} is not a directory", parent_id),
        };
        parent.children.push(id);
        id
    }

    /// Builds a `FileStringTable` from this tree. Returns an `(fst, ids)` tuple, where `ids` is a
    /// list of the `EntryId` corresponding to each FST entry.
    pub fn to_fst(&self) -> Result<(FileStringTable, Vec<EntryId>)> {
        let mut fst = FileStringTable::new();
        let mut fst_ids = vec![];
        self.build_fst_entries(&mut fst, &mut fst_ids, self.root, 0)?;
        Ok((fst, fst_ids))
    }

    fn build_fst_entries(
        &self,
        fst: &mut FileStringTable,
        fst_ids: &mut Vec<EntryId>,
        entry_id: EntryId,
        parent: u32,
    ) -> Result<()> {
        let entry = &self[entry_id];
        let name_offset = if entry_id == self.root {
            0 // The root directory does not have a name
        } else {
            fst.encode_name(entry.name())?
        };
        match entry {
            Entry::File(file) => {
                fst.entries.push(FstEntry {
                    kind: FstEntryKind::File,
                    name_offset,
                    offset_or_parent: file.offset,
                    size_or_next: file.size,
                });
                fst_ids.push(entry_id);
            }
            Entry::Directory(dir) => {
                // The directory entry needs to store the offset of the next adjacent file and we
                // don't know that until we've added all of its children.
                let dir_index = u32::try_from(fst.entries.len()).expect("FST size overflow");
                fst.entries.push(FstEntry {
                    kind: FstEntryKind::Directory,
                    name_offset,
                    offset_or_parent: parent,
                    size_or_next: 0,
                });
                fst_ids.push(entry_id);
                for &id in &dir.children {
                    self.build_fst_entries(fst, fst_ids, id, dir_index)?;
                }
                fst.entries[dir_index as usize].size_or_next =
                    u32::try_from(fst.entries.len()).expect("FST size overflow");
            }
        }
        Ok(())
    }
}

impl Default for FileTree {
    fn default() -> Self {
        Self::new()
    }
}

impl Index<EntryId> for FileTree {
    type Output = Entry;

    fn index(&self, id: EntryId) -> &Self::Output {
        self.entries.get(id).unwrap()
    }
}

impl IndexMut<EntryId> for FileTree {
    fn index_mut(&mut self, id: EntryId) -> &mut Self::Output {
        self.entries.get_mut(id).unwrap()
    }
}

/// A trait for objects which can provide readable streams for file entries.
pub trait OpenFile {
    /// Returns a reader which reads the data inside the file with ID `id`.
    /// ***Panics*** if the ID is invalid.
    fn open_file(&mut self, id: EntryId) -> Result<Box<dyn ReadSeek + '_>>;

    /// Similar to `open_file()`, but consumes this stream.
    fn into_file<'s>(self, id: EntryId) -> Result<Box<dyn ReadSeek + 's>>
    where
        Self: 's;

    /// Returns a reader which reads the data inside the file at `path`.
    fn open_file_at(&mut self, path: &str) -> Result<Box<dyn ReadSeek + '_>>;

    /// Similar to `open_file_at()`, but consumes this stream.
    fn into_file_at<'s>(self, path: &str) -> Result<Box<dyn ReadSeek + 's>>
    where
        Self: 's;
}

impl<T: OpenFile> OpenFile for &mut T {
    fn open_file(&mut self, id: EntryId) -> Result<Box<dyn ReadSeek + '_>> {
        (**self).open_file(id)
    }

    fn into_file<'s>(self, id: EntryId) -> Result<Box<dyn ReadSeek + 's>>
    where
        Self: 's,
    {
        self.open_file(id)
    }

    fn open_file_at(&mut self, path: &str) -> Result<Box<dyn ReadSeek + '_>> {
        (**self).open_file_at(path)
    }

    fn into_file_at<'s>(self, path: &str) -> Result<Box<dyn ReadSeek + 's>>
    where
        Self: 's,
    {
        self.open_file_at(path)
    }
}

/// A trait for objects which can provide writable streams for file entries.
pub trait EditFile: OpenFile {
    /// Returns a stream which can read and write the data inside the file with ID `id`.
    /// ***Panics*** if the ID is invalid.
    fn edit_file(&mut self, id: EntryId) -> Result<Box<dyn ReadWriteSeek + '_>>;

    /// Returns a reader which can read and write the data inside the file at `path`.
    fn edit_file_at(&mut self, path: &str) -> Result<Box<dyn ReadWriteSeek + '_>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_write_and_read;
    use lazy_static::lazy_static;
    use std::io::Cursor;

    #[rustfmt::skip]
    static TEST_FST_BYTES: &[u8] = &[
        /* 0 */ 1, 0, 0, 0,    0, 0, 0, 0,   0, 0, 0, 9,  // /
        /* 1 */ 0, 0, 0, 0,    0, 0, 0, 1,   0, 0, 0, 2,  // /abc
        /* 2 */ 0, 0, 0, 4,    0, 0, 0, 1,   0, 0, 0, 2,  // /def
        /* 3 */ 1, 0, 0, 8,    0, 0, 0, 0,   0, 0, 0, 8,  // /ghi
        /* 4 */ 1, 0, 0, 12,   0, 0, 0, 3,   0, 0, 0, 7,  // /ghi/jkl
        /* 5 */ 1, 0, 0, 16,   0, 0, 0, 4,   0, 0, 0, 7,  // /ghi/jkl/mno
        /* 6 */ 1, 0, 0, 20,   0, 0, 0, 5,   0, 0, 0, 7,  // /ghi/jkl/mno/pqr
        /* 7 */ 0, 0, 0, 24,   0, 0, 0, 1,   0, 0, 0, 2,  // /ghi/stu
        /* 8 */ 0, 0, 0, 28,   0, 0, 0, 1,   0, 0, 0, 2,  // /vwx

        /*  0 */ b'a', b'b', b'c', 0,
        /*  4 */ b'd', b'e', b'f', 0,
        /*  8 */ b'g', b'h', b'i', 0,
        /* 12 */ b'j', b'k', b'l', 0,
        /* 16 */ b'm', b'n', b'o', 0,
        /* 20 */ b'p', b'q', b'r', 0,
        /* 24 */ b's', b't', b'u', 0,
        /* 28 */ b'v', b'w', b'x', 0,
    ];

    lazy_static! {
        static ref TEST_FST: FileStringTable = {
            let mut fst = FileStringTable::new();
            push_fst(&mut fst, FstEntryKind::Directory, "", 0, 9).unwrap();
            push_fst(&mut fst, FstEntryKind::File, "abc", 1, 2).unwrap();
            push_fst(&mut fst, FstEntryKind::File, "def", 1, 2).unwrap();
            push_fst(&mut fst, FstEntryKind::Directory, "ghi", 0, 8).unwrap();
            push_fst(&mut fst, FstEntryKind::Directory, "jkl", 3, 7).unwrap();
            push_fst(&mut fst, FstEntryKind::Directory, "mno", 4, 7).unwrap();
            push_fst(&mut fst, FstEntryKind::Directory, "pqr", 5, 7).unwrap();
            push_fst(&mut fst, FstEntryKind::File, "stu", 1, 2).unwrap();
            push_fst(&mut fst, FstEntryKind::File, "vwx", 1, 2).unwrap();
            fst
        };
        static ref TEST_TREE: FileTree = {
            let mut files = FileTree::new();
            files.insert(files.root(), file("abc"));
            files.insert(files.root(), file("def"));
            let ghi = files.insert(files.root(), dir("ghi"));
            let jkl = files.insert(ghi, dir("jkl"));
            let mno = files.insert(jkl, dir("mno"));
            files.insert(mno, dir("pqr"));
            files.insert(ghi, file("stu"));
            files.insert(files.root(), file("vwx"));
            files
        };
    }

    fn file(name: &str) -> Entry {
        FileEntry::new(name, 1, 2).into()
    }

    fn dir(name: &str) -> Entry {
        DirectoryEntry::new(name).into()
    }

    fn push_fst(
        fst: &mut FileStringTable,
        kind: FstEntryKind,
        name: &str,
        offset_or_parent: u32,
        size_or_next: u32,
    ) -> Result<()> {
        let name_offset = if fst.entries.is_empty() { 0 } else { fst.encode_name(name)? };
        fst.entries.push(FstEntry { kind, name_offset, offset_or_parent, size_or_next });
        Ok(())
    }

    #[test]
    fn test_write_and_read_raw_entry() {
        assert_write_and_read!(FstEntry {
            kind: FstEntryKind::File,
            name_offset: 1,
            offset_or_parent: 2,
            size_or_next: 3,
        });
    }

    #[test]
    fn test_disk_size() {
        assert_eq!(TEST_FST.disk_size(), TEST_FST_BYTES.len() as u32);
    }

    #[test]
    fn test_read_fst() -> Result<()> {
        let mut cursor = Cursor::new(TEST_FST_BYTES);
        let fst = FileStringTable::read_from(&mut cursor)?;
        assert!(fst == *TEST_FST);
        Ok(())
    }

    #[test]
    fn test_write_fst() -> Result<()> {
        let mut cursor = Cursor::new(Vec::<u8>::new());
        TEST_FST.write_to(&mut cursor)?;
        assert_eq!(cursor.into_inner(), TEST_FST_BYTES);
        Ok(())
    }

    #[test]
    fn test_from_fst() -> Result<()> {
        let files = FileTree::from_fst(&TEST_FST)?;
        let root = &files[files.root()].dir().unwrap();
        let abc = files[root.children[0]].file().unwrap();
        let def = files[root.children[1]].file().unwrap();
        let ghi = files[root.children[2]].dir().unwrap();
        let jkl = files[ghi.children[0]].dir().unwrap();
        let mno = files[jkl.children[0]].dir().unwrap();
        let pqr = files[mno.children[0]].dir().unwrap();
        let stu = files[ghi.children[1]].file().unwrap();
        let vwx = files[root.children[3]].file().unwrap();

        assert_eq!(root.name, "");
        assert_eq!(*abc, FileEntry::new("abc", 1, 2));
        assert_eq!(*def, FileEntry::new("def", 1, 2));
        assert_eq!(ghi.name, "ghi");
        assert_eq!(jkl.name, "jkl");
        assert_eq!(mno.name, "mno");
        assert_eq!(pqr.name, "pqr");
        assert_eq!(*stu, FileEntry::new("stu", 1, 2));
        assert_eq!(*vwx, FileEntry::new("vwx", 1, 2));
        Ok(())
    }

    #[test]
    fn test_to_fst() -> Result<()> {
        let (fst, ids) = TEST_TREE.to_fst()?;
        assert!(fst == *TEST_FST);
        assert_eq!(
            ids,
            [
                TEST_TREE.at("/")?,
                TEST_TREE.at("abc")?,
                TEST_TREE.at("def")?,
                TEST_TREE.at("ghi")?,
                TEST_TREE.at("ghi/jkl")?,
                TEST_TREE.at("ghi/jkl/mno")?,
                TEST_TREE.at("ghi/jkl/mno/pqr")?,
                TEST_TREE.at("ghi/stu")?,
                TEST_TREE.at("vwx")?,
            ]
        );
        Ok(())
    }

    #[test]
    fn test_recurse() -> Result<()> {
        let recurse: Vec<_> = TEST_TREE.recurse().collect();
        assert_eq!(
            recurse,
            [
                ("abc".to_owned(), TEST_TREE.at("abc")?),
                ("def".to_owned(), TEST_TREE.at("def")?),
                ("ghi".to_owned(), TEST_TREE.at("ghi")?),
                ("ghi/jkl".to_owned(), TEST_TREE.at("ghi/jkl")?),
                ("ghi/jkl/mno".to_owned(), TEST_TREE.at("ghi/jkl/mno")?),
                ("ghi/jkl/mno/pqr".to_owned(), TEST_TREE.at("ghi/jkl/mno/pqr")?),
                ("ghi/stu".to_owned(), TEST_TREE.at("ghi/stu")?),
                ("vwx".to_owned(), TEST_TREE.at("vwx")?),
            ]
        );
        Ok(())
    }

    #[test]
    fn test_at() -> Result<()> {
        let mut files = FileTree::new();
        let abc = files.insert(files.root(), file("abc"));
        let def = files.insert(files.root(), file("def"));
        let ghi = files.insert(files.root(), dir("ghi"));
        let jkl = files.insert(ghi, dir("jkl"));
        let mno = files.insert(jkl, dir("mno"));
        let pqr = files.insert(mno, dir("pqr"));
        let stu = files.insert(ghi, file("stu"));
        let vwx = files.insert(files.root(), file("vwx"));

        assert_eq!(files.at("abc")?, abc);
        assert_eq!(files.at("def")?, def);
        assert_eq!(files.at("ghi")?, ghi);
        assert_eq!(files.at("ghi/jkl")?, jkl);
        assert_eq!(files.at("ghi/jkl/mno")?, mno);
        assert_eq!(files.at("ghi/jkl/mno/pqr")?, pqr);
        assert_eq!(files.at("ghi/stu")?, stu);
        assert_eq!(files.at("vwx")?, vwx);

        assert!(files.at("ab").is_err());
        assert!(files.at("abcd").is_err());

        assert_eq!(files.at("ghi\\jkl/mno\\pqr")?, pqr);
        assert_eq!(files.at("//ghi/jkl/mno/pqr")?, pqr);
        assert_eq!(files.at("ghi////jkl////mno////pqr")?, pqr);
        assert_eq!(files.at("ghi////jkl////mno////pqr////")?, pqr);

        assert_eq!(files.at("/")?, files.root());
        assert_eq!(files.at("\\")?, files.root());
        assert_eq!(files.at("")?, files.root());
        assert_eq!(files.at(".")?, files.root());
        assert!(files.at("..").is_err());

        assert!(files.at("abc/").is_err());
        assert!(files.at("abc/../abc").is_err());
        assert_eq!(files.at("ghi/jkl/mno/pqr/.")?, pqr);
        assert_eq!(files.at("ghi/jkl/mno/pqr/..")?, mno);
        assert_eq!(files.at("ghi/./jkl/./mno/./pqr")?, pqr);
        assert_eq!(files.at("ghi/../ghi/jkl/mno/../../jkl/mno/pqr")?, pqr);
        assert!(files.at("ghi/../../ghi/jkl/mno/../../jkl/mno/pqr").is_err());
        Ok(())
    }

    #[test]
    fn test_is_name_safe() {
        assert!(!is_name_safe(""));
        assert!(!is_name_safe("."));
        assert!(!is_name_safe(".."));
        assert!(is_name_safe("..."));
        assert!(is_name_safe("foo"));
        assert!(!is_name_safe("foo\\"));
        assert!(!is_name_safe("foo/"));
        assert!(!is_name_safe("/"));
        assert!(!is_name_safe("C:"));
    }
}
