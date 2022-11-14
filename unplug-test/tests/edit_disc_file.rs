use anyhow::Result;
use log::info;
use std::cmp;
use std::fs::{File, OpenOptions};
use std::io::{Cursor, Read, SeekFrom};
use std::path::Path;
use unplug::dvd::{DiscStream, EditFile, OpenFile};
use unplug_test as common;

const OLD_QP_OFFSET: u32 = 0x3dd6e348;
const NEW_QP_OFFSET: u32 = 0x280000;

const OLD_QP_SIZE: u32 = 0x46b288b;

#[test]
fn test_edit_disc_file() -> Result<()> {
    common::init_logging();
    let copy_path = common::copy_iso()?;

    check_resize_qp(&copy_path, OLD_QP_SIZE + 1, OLD_QP_OFFSET)?; // Larger (no move)
    check_resize_qp(&copy_path, OLD_QP_SIZE + 2, NEW_QP_OFFSET)?; // Larger (move)
    check_resize_qp(&copy_path, OLD_QP_SIZE, NEW_QP_OFFSET)?; // Smaller (no move)

    check_move_file(&copy_path, 0x10000000)?;
    check_move_file(&copy_path, OLD_QP_OFFSET)?;

    check_edit_file(&copy_path)?;
    check_replace_file(&copy_path)?;

    Ok(())
}

fn check_resize_qp(copy_path: &Path, new_size: u32, expected_offset: u32) -> Result<()> {
    info!("Opening {}", copy_path.display());
    let file = OpenOptions::new().read(true).write(true).open(copy_path)?;
    let mut iso = DiscStream::open(file)?;
    common::check_iso(&iso)?;

    info!("Resizing qp.bin");
    let qp = iso.files.at("qp.bin")?;
    iso.resize_file(qp, new_size)?;
    assert_eq!(iso.files.file(qp)?.offset, expected_offset);
    assert_eq!(iso.files.file(qp)?.size, new_size);
    drop(iso);

    info!("Comparing ISOs");
    let mut old_iso = common::open_iso()?;
    let mut new_iso = DiscStream::open(File::open(copy_path)?)?;
    let old_qp = old_iso.files.file_at("qp.bin")?;
    let new_qp = new_iso.files.file_at("qp.bin")?;
    assert_eq!(old_qp.offset, OLD_QP_OFFSET);
    assert_eq!(new_qp.offset, expected_offset);
    assert_eq!(new_qp.size, new_size);

    let compare_size = cmp::min(old_qp.size, new_size) as u64;
    let old_reader = old_iso.open_file_at("qp.bin")?;
    let new_reader = new_iso.open_file_at("qp.bin")?;
    assert!(common::compare_streams(old_reader.take(compare_size), new_reader.take(compare_size))?);
    Ok(())
}

fn check_move_file(copy_path: &Path, new_offset: u32) -> Result<()> {
    info!("Opening {}", copy_path.display());
    let file = OpenOptions::new().read(true).write(true).open(copy_path)?;
    let mut iso = DiscStream::open(file)?;
    common::check_iso(&iso)?;

    info!("Moving qp.bin");
    let qp = iso.files.at("qp.bin")?;
    iso.move_file(qp, new_offset)?;
    assert_eq!(iso.files.file(qp)?.offset, new_offset);
    drop(iso);

    info!("Comparing ISOs");
    let mut old_iso = common::open_iso()?;
    let mut new_iso = DiscStream::open(File::open(copy_path)?)?;
    let old_qp = old_iso.files.file_at("qp.bin")?;
    let new_qp = new_iso.files.file_at("qp.bin")?;
    assert_eq!(old_qp.offset, OLD_QP_OFFSET);
    assert_eq!(new_qp.offset, new_offset);
    let old_reader = old_iso.open_file_at("qp.bin")?;
    let new_reader = new_iso.open_file_at("qp.bin")?;
    assert!(common::compare_streams(old_reader, new_reader)?);
    Ok(())
}

fn check_edit_file(copy_path: &Path) -> Result<()> {
    info!("Opening {}", copy_path.display());
    let file = OpenOptions::new().read(true).write(true).open(copy_path)?;
    let mut iso = DiscStream::open(file)?;
    common::check_iso(&iso)?;

    info!("Editing qp.bin");
    let qp = iso.files.at("qp.bin")?;
    let mut stream = iso.edit_file(qp)?;
    assert_eq!(stream.seek(SeekFrom::End(0))?, OLD_QP_SIZE.into());
    assert_eq!(stream.write(&[0xaa, 0xbb])?, 1);
    assert_eq!(stream.write(&[0xaa, 0xbb])?, 0);
    Ok(())
}

fn check_replace_file(copy_path: &Path) -> Result<()> {
    info!("Opening {}", copy_path.display());
    let file = OpenOptions::new().read(true).write(true).open(copy_path)?;
    let mut iso = DiscStream::open(file)?;
    common::check_iso(&iso)?;

    info!("Replacing qp.bin");
    iso.replace_file_at("qp.bin", Cursor::new(b"Hello, world!"))?;
    drop(iso);

    info!("Checking updated file");
    let mut iso = DiscStream::open(File::open(copy_path)?)?;
    let reader = iso.open_file_at("qp.bin")?;
    assert!(common::compare_streams(reader, Cursor::new(b"Hello, world!"))?);
    Ok(())
}
