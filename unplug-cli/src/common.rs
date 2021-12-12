use crate::io::copy_into_memory;
use crate::opt::{OptionalContainerOpt, RequiredContainerOpt};
use anyhow::{bail, Result};
use log::{debug, info};
use std::fs::{File, OpenOptions};
use std::io::Cursor;
use std::path::Path;
use unplug::common::ReadSeek;
use unplug::data::stage::{StageDefinition, GLOBALS_PATH};
use unplug::dvd::{ArchiveReader, DiscStream, OpenFile};
use unplug::globals::{GlobalsReader, Libs};
use unplug::stage::Stage;

pub const QP_PATH: &str = "qp.bin";

/// A cursor around a byte slice.
pub type MemoryCursor = Cursor<Box<[u8]>>;

/// Optionally opens an ISO file for reading.
pub fn open_iso_optional(path: Option<impl AsRef<Path>>) -> Result<Option<DiscStream<File>>> {
    if let Some(path) = path {
        info!("Opening ISO: {}", path.as_ref().to_str().unwrap());
        let file = File::open(path)?;
        Ok(Some(DiscStream::open(file)?))
    } else {
        Ok(None)
    }
}

/// Optionally opens an ISO file for reading and writing.
pub fn edit_iso_optional(path: Option<impl AsRef<Path>>) -> Result<Option<DiscStream<File>>> {
    if let Some(path) = path {
        info!("Opening ISO: {}", path.as_ref().to_str().unwrap());
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        Ok(Some(DiscStream::open(file)?))
    } else {
        Ok(None)
    }
}

/// Opens a file from either an ISO or the local filesystem.
pub fn open_iso_entry_or_file<'a>(
    iso: Option<&'a mut DiscStream<File>>,
    path: impl AsRef<Path>,
) -> Result<Box<dyn ReadSeek + 'a>> {
    if let Some(iso) = iso {
        info!("Opening {} from ISO", path.as_ref().display());
        Ok(iso.open_file_at(path.as_ref().to_str().unwrap())?)
    } else {
        info!("Opening file: {}", path.as_ref().display());
        Ok(Box::new(File::open(path)?))
    }
}

fn open_qp<'a>(
    iso: Option<&'a mut DiscStream<File>>,
    path: Option<impl AsRef<Path>>,
) -> Result<Option<ArchiveReader<Box<dyn ReadSeek + 'a>>>> {
    if let Some(iso) = iso {
        info!("Opening {} from ISO", QP_PATH);
        let file = iso.open_file_at(QP_PATH)?;
        Ok(Some(ArchiveReader::open(file)?))
    } else if let Some(path) = path {
        info!("Opening archive: {}", path.as_ref().display());
        let file: Box<dyn ReadSeek> = Box::new(File::open(path)?);
        Ok(Some(ArchiveReader::open(file)?))
    } else {
        Ok(None)
    }
}

/// Opens a qp.bin file from either an ISO or the local filesystem if a path is provided.
pub fn open_qp_optional<'a>(
    iso: Option<&'a mut DiscStream<File>>,
    container: &OptionalContainerOpt,
) -> Result<Option<ArchiveReader<Box<dyn ReadSeek + 'a>>>> {
    open_qp(iso, container.qp.as_ref())
}

/// Opens a qp.bin file from either an ISO or the local filesystem.
pub fn open_qp_required<'a>(
    iso: Option<&'a mut DiscStream<File>>,
    container: &RequiredContainerOpt,
) -> Result<ArchiveReader<Box<dyn ReadSeek + 'a>>> {
    open_qp(iso, container.qp.as_ref()).transpose().expect("No container provided")
}

/// Copies an archive entry into memory.
fn read_entry(
    archive: &mut ArchiveReader<Box<dyn ReadSeek + '_>>,
    path: &str,
) -> Result<MemoryCursor> {
    debug!("Opening archive entry: {}", path);
    Ok(copy_into_memory(archive.open_file_at(path)?)?)
}

/// Copies a file into memory.
fn read_file(path: impl AsRef<Path>) -> Result<MemoryCursor> {
    debug!("Opening file: {}", path.as_ref().display());
    Ok(copy_into_memory(File::open(path)?)?)
}

/// Reads globals.bin from an archive.
pub fn read_globals_qp(
    qp: &mut ArchiveReader<Box<dyn ReadSeek + '_>>,
) -> Result<GlobalsReader<MemoryCursor>> {
    let reader = read_entry(qp, GLOBALS_PATH)?;
    Ok(GlobalsReader::open(reader)?)
}

/// Reads globals.bin from either an archive or the local filesystem.
pub fn read_globals_qp_or_file(
    qp: Option<&mut ArchiveReader<Box<dyn ReadSeek + '_>>>,
    path: Option<impl AsRef<Path>>,
) -> Result<GlobalsReader<MemoryCursor>> {
    match qp {
        Some(qp) => read_globals_qp(qp),
        None => {
            let reader = read_file(path.expect("Missing globals.bin path"))?;
            Ok(GlobalsReader::open(reader)?)
        }
    }
}

/// Reads a stage by name from an archive.
pub fn read_stage_qp(
    qp: &mut ArchiveReader<Box<dyn ReadSeek + '_>>,
    name: &str,
    libs: &Libs,
) -> Result<Stage> {
    let def = match StageDefinition::find(name) {
        Some(def) => def,
        None => bail!("Unknown stage \"{}\"", name),
    };
    Ok(Stage::read_from(&mut read_entry(qp, &def.path())?, libs)?)
}

/// Reads a stage by name from either an archive or the local filesystem.
pub fn read_stage_qp_or_file(
    qp: Option<&mut ArchiveReader<Box<dyn ReadSeek + '_>>>,
    name: impl AsRef<Path>,
    libs: &Libs,
) -> Result<Stage> {
    match qp {
        Some(qp) => read_stage_qp(qp, name.as_ref().to_str().unwrap(), libs),
        None => Ok(Stage::read_from(&mut read_file(name)?, libs)?),
    }
}
