use anyhow::Result;
use log::info;
use serial_test::serial;
use std::io::Cursor;
use tempfile::tempfile;
use unplug::common::ReadSeek;
use unplug::dvd::{ArchiveBuilder, ArchiveReader, Entry, OpenFile};
use unplug_test as common;

#[test]
#[serial]
fn test_rebuild_archive_copy_all() -> Result<()> {
    common::init_logging();

    let mut iso = common::open_iso()?;
    info!("Opening {}", common::QP_PATH);
    let mut original = ArchiveReader::open(iso.open_file_at(common::QP_PATH)?)?;

    info!("Rebuilding archive");
    let mut temp = tempfile()?;
    ArchiveBuilder::with_archive(&mut original).write_to(&mut temp)?;

    info!("Comparing files");
    let mut rebuilt = ArchiveReader::open(temp)?;
    assert_eq!(rebuilt.files.entries.len(), original.files.entries.len());
    for (path, rebuilt_id) in rebuilt.files.recurse() {
        let file = match &rebuilt.files[rebuilt_id] {
            Entry::File(f) => f,
            _ => continue,
        };
        let original_id = original.files.at(&path)?;
        let mut original_reader = original.open_file(original_id)?;
        let mut rebuilt_reader = file.open(&mut rebuilt.reader)?;
        assert!(common::compare_streams(&mut original_reader, &mut rebuilt_reader)?);
    }
    Ok(())
}

#[test]
#[serial]
fn test_rebuild_archive_replace_files() -> Result<()> {
    common::init_logging();

    let mut iso = common::open_iso()?;
    info!("Opening {}", common::QP_PATH);
    let mut original = ArchiveReader::open(iso.open_file_at(common::QP_PATH)?)?;

    info!("Rebuilding archive");
    let mut temp = tempfile()?;
    ArchiveBuilder::with_archive(&mut original)
        .replace_at("bin/e/stage01.bin", || Cursor::new(b"stage01"))?
        .replace_at("bin/e/stage02.bin", || Cursor::new(b"stage02"))?
        .replace_at("bin/e/stage03.bin", || Cursor::new(b"stage03"))?
        .replace_at("bin/e/stage04.bin", || Cursor::new(b"stage04"))?
        .replace_at("bin/e/stage05.bin", || Cursor::new(b"stage05"))?
        .write_to(&mut temp)?;

    info!("Comparing files");
    let mut rebuilt = ArchiveReader::open(temp)?;
    assert_eq!(rebuilt.files.entries.len(), original.files.entries.len());
    for (path, rebuilt_id) in rebuilt.files.recurse() {
        let file = match &rebuilt.files[rebuilt_id] {
            Entry::File(f) => f,
            _ => continue,
        };
        let mut rebuilt_reader = file.open(&mut rebuilt.reader)?;
        let mut expected: Box<dyn ReadSeek> = match path.as_ref() {
            "bin/e/stage01.bin" => Box::new(Cursor::new(b"stage01")),
            "bin/e/stage02.bin" => Box::new(Cursor::new(b"stage02")),
            "bin/e/stage03.bin" => Box::new(Cursor::new(b"stage03")),
            "bin/e/stage04.bin" => Box::new(Cursor::new(b"stage04")),
            "bin/e/stage05.bin" => Box::new(Cursor::new(b"stage05")),
            _ => {
                let original_id = original.files.at(&path)?;
                original.open_file(original_id)?
            }
        };
        assert!(common::compare_streams(&mut rebuilt_reader, &mut expected)?);
    }
    Ok(())
}
