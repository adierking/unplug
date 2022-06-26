use crate::io::{copy_into_memory, MemoryCursor};
use anyhow::{anyhow, Error, Result};
use lazy_static::lazy_static;
use log::{debug, error, info, warn};
use regex::Regex;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Cursor, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::NamedTempFile;
use unplug::audio::metadata::SfxPlaylist;
use unplug::audio::transport::{HpsReader, SfxBank};
use unplug::common::io::{copy_buffered, BUFFER_SIZE};
use unplug::common::{ReadFrom, ReadSeek, ReadWriteSeek, WriteTo};
use unplug::data::{Music, Sfx, SfxGroup, Stage as StageId};
use unplug::dvd::{ArchiveBuilder, ArchiveReader, DiscStream, EntryId, FileTree, OpenFile};
use unplug::globals::{GlobalsBuilder, GlobalsReader, Libs};
use unplug::stage::Stage;

/// Path to qp.bin within the disc.
const QP_PATH: &str = "qp.bin";

lazy_static! {
    static ref DVD_PATH_REGEX: Regex = Regex::new(r"^(?i)dvd:(.*)$").unwrap();
    static ref FILE_PATH_REGEX: Regex = Regex::new(r"^(?i)file:(.*)$").unwrap();
    static ref QP_PATH_REGEX: Regex = Regex::new(r"^(?i)qp:(.*)$").unwrap();
}

fn no_disc_error() -> Error {
    anyhow!("No disc is loaded. Open a project or use --iso.")
}

fn dir_error(path: &str) -> Error {
    anyhow!("{} is a directory", path)
}

/// A path relative to a resource in the current context.
///
/// Path prefixes:
/// - `dvd:` - Relative to the disc
/// - `file:` - Absolute or relative local file path
/// - `qp:` - Relative to qp.bin
enum ContextPath<'a> {
    Dvd(&'a str),
    File(&'a str),
    Qp(&'a str),
    Other(&'a str),
}

impl<'a> ContextPath<'a> {
    /// Parses a path in a string.
    fn parse(path: &'a str) -> Self {
        if let Some(captures) = DVD_PATH_REGEX.captures(path) {
            ContextPath::Dvd(captures.get(1).unwrap().as_str())
        } else if let Some(captures) = FILE_PATH_REGEX.captures(path) {
            ContextPath::File(captures.get(1).unwrap().as_str())
        } else if let Some(captures) = QP_PATH_REGEX.captures(path) {
            ContextPath::Qp(captures.get(1).unwrap().as_str())
        } else {
            ContextPath::Other(path)
        }
    }
}

/// Opens the iso at `path` for read access.
fn open_iso_read(path: &Path) -> Result<DiscSource<Box<dyn ReadSeek>>> {
    let file: Box<dyn ReadSeek> = Box::from(File::open(path)?);
    Ok(DiscSource::Iso(DiscStream::open(file)?.into()))
}

/// Opens the iso at `path` for read and write access.
fn open_iso_read_write(path: &Path) -> Result<DiscSource<Box<dyn ReadWriteSeek>>> {
    let file: Box<dyn ReadWriteSeek> =
        Box::from(OpenOptions::new().read(true).write(true).open(path)?);
    Ok(DiscSource::Iso(DiscStream::open(file)?.into()))
}

/// The context which a command is run in.
#[non_exhaustive]
#[derive(Clone)]
pub enum Context {
    /// The command only has access to the local filesystem.
    Local,
    /// The command has access to files inside a .iso.
    Iso(PathBuf),
    /// The command has access to files inside the default ISO, but it is read-only.
    DefaultIso(PathBuf),
    /// The command has access to files inside a project ISO.
    ProjectIso { name: String, path: PathBuf },
}

impl Context {
    /// Opens the context files for reading.
    pub fn open_read(self) -> Result<OpenContext<Box<dyn ReadSeek>>> {
        let disc = match self {
            Self::Local => DiscSource::None,
            Self::Iso(path) => {
                info!("Opening ISO: {}", path.display());
                open_iso_read(&path)?
            }
            Self::DefaultIso(path) => {
                info!("Opening default ISO: {}", path.display());
                match open_iso_read(&path) {
                    Ok(disc) => disc,
                    Err(e) => {
                        error!("Could not load the default ISO: {:#}", e);
                        DiscSource::None
                    }
                }
            }
            Self::ProjectIso { name, path } => {
                info!("Opening ISO: {} ({})", name, path.display());
                open_iso_read(&path)?
            }
        };
        Ok(OpenContext::new(disc))
    }

    /// Opens the context files for reading and writing.
    pub fn open_read_write(self) -> Result<OpenContext<Box<dyn ReadWriteSeek>>> {
        let disc = match self {
            Self::Local => DiscSource::None,
            Self::Iso(path) => {
                info!("Opening ISO: {}", path.display());
                open_iso_read_write(&path)?
            }
            Self::DefaultIso(_) => {
                warn!("Editing commands do not load the default ISO, as a precaution");
                DiscSource::None
            }
            Self::ProjectIso { name, path } => {
                info!("Opening ISO: {} ({})", name, path.display());
                open_iso_read_write(&path)?
            }
        };
        Ok(OpenContext::new(disc))
    }

    /// Requires the context to be an ISO and returns its path.
    pub fn into_iso_path(self) -> Result<PathBuf> {
        match self {
            Context::Local => Err(no_disc_error()),
            Context::Iso(path) => {
                info!("Using ISO: {}", path.display());
                Ok(path)
            }
            Context::DefaultIso(path) => {
                info!("Using default ISO: {}", path.display());
                Ok(path)
            }
            Context::ProjectIso { name, path } => {
                info!("Using ISO: {} ({})", name, path.display());
                Ok(path)
            }
        }
    }
}

/// A source for game disc data.
enum DiscSource<T: ReadSeek> {
    /// No disc is available.
    None,
    /// Disc data is stored in a .iso.
    Iso(Box<DiscStream<T>>),
}

impl<T: ReadSeek> DiscSource<T> {
    /// Gets the `FileId` of the disc file at `path`.
    fn get(&self, path: &str) -> Result<FileId> {
        let files = match self {
            Self::None => return Err(no_disc_error()),
            Self::Iso(disc) => &disc.files,
        };
        let entry = files.at(path)?;
        if files[entry].is_dir() {
            return Err(dir_error(path));
        }
        Ok(FileId::Iso(entry))
    }

    /// Queries information about a disc file.
    fn query(&self, file: &FileId) -> Result<FileInfo> {
        if let FileId::Iso(entry) = *file {
            let info = match self {
                Self::None => return Err(no_disc_error()),
                Self::Iso(disc) => disc.files[entry].file().unwrap(),
            };
            Ok(FileInfo { name: info.name.clone(), size: info.size.into() })
        } else {
            panic!("cannot query a non-disc file");
        }
    }

    /// Opens a disc file for reading.
    fn open(&mut self, file: &FileId) -> Result<Box<dyn ReadSeek + '_>> {
        if let FileId::Iso(entry) = *file {
            match self {
                Self::None => Err(no_disc_error()),
                Self::Iso(disc) => Ok(disc.open_file(entry)?),
            }
        } else {
            panic!("cannot open a non-disc file");
        }
    }
}

impl<T: ReadWriteSeek> DiscSource<T> {
    /// Overwrites a disc file with data read from `reader`.
    fn write(&mut self, file: &FileId, reader: &mut dyn ReadSeek) -> Result<()> {
        if let FileId::Iso(entry) = *file {
            match self {
                Self::None => Err(no_disc_error()),
                Self::Iso(disc) => Ok(disc.replace_file(entry, reader)?),
            }
        } else {
            panic!("cannot write a non-disc file");
        }
    }
}

/// An identifier for a file which resides in a .iso, qp.bin, or the local filesystem.
#[non_exhaustive]
#[derive(Clone, Hash, PartialEq, Eq)]
pub enum FileId {
    Iso(EntryId),
    Qp(EntryId),
    File(Arc<PathBuf>),
}

/// File metadata.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfo {
    /// The name of the file including its extension.
    pub name: String,
    /// The size of the file in bytes.
    pub size: u64,
}

/// Cached qp.bin data.
struct QpCache {
    /// The disc file that qp.bin was read from.
    file: FileId,
    /// The archive file tree.
    tree: FileTree,
}

/// An opened context which can read and write game files.
pub struct OpenContext<T: ReadSeek> {
    disc: DiscSource<T>,
    qp: Option<QpCache>,
}

impl<T: ReadSeek> OpenContext<T> {
    fn new(disc: DiscSource<T>) -> Self {
        Self { disc, qp: None }
    }

    /// Gets the ID of a file located using a context path.
    pub fn file_at(&mut self, path: impl AsRef<str>) -> Result<FileId> {
        self.get_file_impl(ContextPath::parse(path.as_ref()))
    }

    /// Gets the ID of a file located using a context path, but returns `None` for paths which do
    /// not explicitly name a file.
    pub fn explicit_file_at(&mut self, path: impl AsRef<str>) -> Result<Option<FileId>> {
        match ContextPath::parse(path.as_ref()) {
            ContextPath::Other(_) => Ok(None),
            path => Ok(Some(self.get_file_impl(path)?)),
        }
    }

    /// Gets the ID of a file on the disc.
    pub fn disc_file_at(&mut self, path: impl AsRef<str>) -> Result<FileId> {
        self.get_file_impl(ContextPath::Dvd(path.as_ref()))
    }

    /// Gets the ID of a file in qp.bin.
    pub fn qp_file_at(&mut self, path: impl AsRef<str>) -> Result<FileId> {
        self.get_file_impl(ContextPath::Qp(path.as_ref()))
    }

    fn get_file_impl(&mut self, path: ContextPath<'_>) -> Result<FileId> {
        match path {
            ContextPath::Dvd(path) => self.disc.get(path),
            ContextPath::Qp(path) => {
                let qp = self.load_qp()?;
                let entry = qp.tree.at(path)?;
                if qp.tree[entry].is_dir() {
                    return Err(dir_error(path));
                }
                Ok(FileId::Qp(entry))
            }
            ContextPath::File(path) | ContextPath::Other(path) => {
                let canonical = fs::canonicalize(path)?;
                if canonical.is_dir() {
                    return Err(dir_error(path));
                }
                Ok(FileId::File(canonical.into()))
            }
        }
    }

    /// Queries information about a file without opening it.
    pub fn query_file(&mut self, file: &FileId) -> Result<FileInfo> {
        match file {
            FileId::Iso(_) => self.disc.query(file),
            &FileId::Qp(entry) => {
                let info = self.load_qp()?.tree[entry].file().unwrap();
                Ok(FileInfo { name: info.name.clone(), size: info.size.into() })
            }
            FileId::File(path) => {
                let name = path
                    .file_name()
                    .unwrap_or_else(|| OsStr::new(""))
                    .to_string_lossy()
                    .into_owned();
                let info = fs::metadata(path.as_path())?;
                Ok(FileInfo { name, size: info.len() })
            }
        }
    }

    /// Opens a file for reading.
    pub fn open_file(&mut self, file: &FileId) -> Result<Box<dyn ReadSeek + '_>> {
        match file {
            FileId::Iso(_) => self.disc.open(file),
            &FileId::Qp(entry) => {
                let file = self.load_qp()?.file.clone();
                let reader = self.disc.open(&file)?;
                let entry = self.qp.as_ref().unwrap().tree[entry].file().unwrap();
                Ok(entry.open(reader)?)
            }
            FileId::File(path) => Ok(Box::from(File::open(path.as_ref())?)),
        }
    }

    /// Opens a file for reading, located using a context path.
    pub fn open_file_at(&mut self, path: impl AsRef<str>) -> Result<Box<dyn ReadSeek + '_>> {
        let file = self.file_at(path)?;
        self.open_file(&file)
    }

    /// Opens a disc file for reading.
    pub fn open_disc_file_at(&mut self, path: impl AsRef<str>) -> Result<Box<dyn ReadSeek + '_>> {
        let file = self.disc_file_at(path)?;
        self.open_file(&file)
    }

    /// Opens a file in qp.bin for reading.
    pub fn open_qp_file_at(&mut self, path: impl AsRef<str>) -> Result<Box<dyn ReadSeek + '_>> {
        let file = self.qp_file_at(path)?;
        self.open_file(&file)
    }

    /// Opens a `GlobalsReader` on globals.bin.
    pub fn read_globals(&mut self) -> Result<GlobalsReader<MemoryCursor>> {
        let reader = self.open_qp_file_at(StageId::QP_GLOBALS_PATH)?;
        let cursor = copy_into_memory(reader)?;
        Ok(GlobalsReader::open(cursor)?)
    }

    /// Opens the stage file corresponding to `id`.
    pub fn read_stage(&mut self, libs: &Libs, id: StageId) -> Result<Stage> {
        let file = self.qp_file_at(id.path())?;
        self.read_stage_file(libs, &file)
    }

    /// Reads `file` as a stage file.
    pub fn read_stage_file(&mut self, libs: &Libs, file: &FileId) -> Result<Stage> {
        let reader = self.open_file(file)?;
        let mut cursor = copy_into_memory(reader)?;
        Ok(Stage::read_from(&mut cursor, libs)?)
    }

    /// Opens the music file corresponding to `id`.
    pub fn open_music(&mut self, id: Music) -> Result<HpsReader<'_>> {
        let path = id.path().ok_or_else(|| anyhow!("{:?} does not have an associated file", id))?;
        let file = self.disc_file_at(path)?;
        self.open_music_file(&file)
    }

    /// Opens `file` as a music file.
    pub fn open_music_file(&mut self, file: &FileId) -> Result<HpsReader<'_>> {
        let info = self.query_file(file)?;
        let reader = BufReader::new(self.open_file(file)?);
        Ok(HpsReader::new(reader, info.name)?)
    }

    /// Opens sfx_sample.sem.
    pub fn read_playlist(&mut self) -> Result<SfxPlaylist> {
        let mut reader = self.open_disc_file_at(Sfx::DISC_PLAYLIST_PATH)?;
        Ok(SfxPlaylist::read_from(&mut reader)?)
    }

    /// Reads the sample bank corresponding to `bank`.
    pub fn read_bank(&mut self, bank: SfxGroup) -> Result<SfxBank> {
        let file = self.disc_file_at(bank.path())?;
        self.read_bank_file(&file)
    }

    /// Reads `file` as a sample bank file.
    pub fn read_bank_file(&mut self, file: &FileId) -> Result<SfxBank> {
        let info = self.query_file(file)?;
        let reader = self.open_file(file)?;
        let mut cursor = copy_into_memory(reader)?;
        Ok(SfxBank::open(&mut cursor, info.name)?)
    }

    /// Loads and caches qp.bin if it is not available.
    fn load_qp(&mut self) -> Result<&QpCache> {
        if self.qp.is_none() {
            debug!("Reading qp.bin");
            let id = self.disc.get(QP_PATH)?;
            let files = ArchiveReader::open(self.disc.open(&id)?)?.files;
            self.qp = Some(QpCache { file: id, tree: files });
        }
        Ok(self.qp.as_ref().unwrap())
    }
}

impl<T: ReadWriteSeek> OpenContext<T> {
    /// Begins a series of file updates. The returned queue must be committed with `commit()` or
    /// else no writes will take place.
    pub fn begin_update<'r>(&mut self) -> UpdateQueue<'_, 'r, T> {
        UpdateQueue::new(self)
    }
}

/// A queue of pending file updates.
pub struct UpdateQueue<'c, 'r, T: ReadWriteSeek> {
    ctx: &'c mut OpenContext<T>,
    qp_files: Vec<(EntryId, Box<dyn ReadSeek + 'r>)>,
    iso_files: Vec<(EntryId, Box<dyn ReadSeek + 'r>)>,
    fs_files: Vec<(Arc<PathBuf>, Box<dyn ReadSeek + 'r>)>,
}

impl<'c, 'r, T: ReadWriteSeek> UpdateQueue<'c, 'r, T> {
    fn new(ctx: &'c mut OpenContext<T>) -> Self {
        Self { ctx, qp_files: vec![], iso_files: vec![], fs_files: vec![] }
    }

    /// Enqueues `file` to be written from data in `reader`.
    pub fn write_file(self, file: &FileId, reader: impl ReadSeek + 'r) -> Self {
        self.write_impl(file, Box::from(reader))
    }

    /// Enqueues the file at `path` to be written from data in `reader`.
    pub fn write_file_at(self, path: &str, reader: impl ReadSeek + 'r) -> Result<Self> {
        let file = self.ctx.file_at(path)?;
        Ok(self.write_file(&file, reader))
    }

    /// Enqueues the disc file at `path` to be written from data in `reader`.
    pub fn write_disc_file_at(self, path: &str, reader: impl ReadSeek + 'r) -> Result<Self> {
        let file = self.ctx.disc_file_at(path)?;
        Ok(self.write_file(&file, reader))
    }

    /// Enqueues the file at `path` in qp.bin to be written from data in `reader`.
    pub fn write_qp_file_at(self, path: &str, reader: impl ReadSeek + 'r) -> Result<Self> {
        let file = self.ctx.qp_file_at(path)?;
        Ok(self.write_file(&file, reader))
    }

    /// Enqueues globals.bin to be built from `builder`.
    pub fn write_globals(self, builder: &mut GlobalsBuilder<'_>) -> Result<Self> {
        let mut writer = Cursor::new(vec![]);
        builder.write_to(&mut writer)?;
        writer.seek(SeekFrom::Start(0))?;
        self.write_qp_file_at(StageId::QP_GLOBALS_PATH, writer)
    }

    /// Enqueues the stage file corresponding to `id` to be written from `stage`.
    pub fn write_stage(self, id: StageId, stage: &Stage) -> Result<Self> {
        let file = self.ctx.qp_file_at(id.path())?;
        self.write_stage_file(&file, stage)
    }

    /// Enqueues the stage file at `file` to be written from `stage`.
    pub fn write_stage_file(self, file: &FileId, stage: &Stage) -> Result<Self> {
        let mut writer = Cursor::new(vec![]);
        stage.write_to(&mut writer)?;
        writer.seek(SeekFrom::Start(0))?;
        Ok(self.write_file(file, writer))
    }

    fn write_impl(mut self, file: &FileId, reader: Box<dyn ReadSeek + 'r>) -> Self {
        match file {
            &FileId::Iso(entry) => self.iso_files.push((entry, reader)),
            &FileId::Qp(entry) => self.qp_files.push((entry, reader)),
            FileId::File(path) => self.fs_files.push((path.clone(), reader)),
        }
        self
    }

    /// Commits all pending file updates.
    pub fn commit(mut self) -> Result<()> {
        // qp.bin is updated first since it's part of the disc
        if !self.qp_files.is_empty() {
            self.commit_qp()?;
        }
        // The disc is updated next because it's part of the local filesystem
        if !self.iso_files.is_empty() {
            self.commit_iso()?;
        }
        // Finally, update local files
        if !self.fs_files.is_empty() {
            self.commit_fs()?;
        }
        Ok(())
    }

    fn commit_qp(&mut self) -> Result<()> {
        self.ctx.load_qp()?;
        debug!("Rebuilding qp.bin");

        // Take the qp.bin cache because it will end up being invalid anyway
        let qp = self.ctx.qp.take().unwrap();
        let mut archive = ArchiveReader::new(self.ctx.open_file(&qp.file)?, qp.tree);
        let mut builder = ArchiveBuilder::with_archive(&mut archive);
        for (entry, reader) in self.qp_files.drain(..) {
            builder.replace(entry, || reader);
        }

        let mut temp = NamedTempFile::new()?;
        debug!("Writing new qp.bin to {}", temp.path().to_string_lossy());
        builder.write_to(&mut temp)?;
        drop(builder);
        drop(archive);

        temp.seek(SeekFrom::Start(0))?;
        self.ctx.disc.write(&qp.file, &mut temp)?;
        Ok(())
    }

    fn commit_iso(&mut self) -> Result<()> {
        for (entry, mut reader) in self.iso_files.drain(..) {
            self.ctx.disc.write(&FileId::Iso(entry), &mut *reader)?;
        }
        Ok(())
    }

    fn commit_fs(&mut self) -> Result<()> {
        let mut buf = [0u8; BUFFER_SIZE];
        for (path, mut reader) in self.fs_files.drain(..) {
            let mut out = File::create(&*path)?;
            copy_buffered(&mut *reader, &mut out, &mut buf)?;
            out.flush()?;
        }
        Ok(())
    }
}
