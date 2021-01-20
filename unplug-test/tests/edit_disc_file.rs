use anyhow::Result;
use log::info;
use serial_test::serial;
use std::cmp;
use std::fs::{File, OpenOptions};
use std::io::{Cursor, Read, SeekFrom};
use unplug::dvd::{DiscStream, EditFile, OpenFile};
use unplug_test as common;

#[test]
#[serial]
fn test_move_disc_file() -> Result<()> {
    common::init_logging();

    let copy_path = common::copy_iso()?;
    info!("Opening {}", copy_path.to_str().unwrap());
    let file = OpenOptions::new().read(true).write(true).open(&copy_path)?;
    let mut iso = DiscStream::open(file)?;
    common::check_iso(&iso)?;

    info!("Moving qp.bin");
    let qp = iso.files.at("qp.bin")?;
    assert_eq!(iso.files.file(qp)?.offset, 0x3dd6e348);
    iso.move_file(qp, 0x280000)?;
    assert_eq!(iso.files.file(qp)?.offset, 0x280000);
    drop(iso);

    info!("Comparing ISOs");
    let mut old_iso = common::open_iso()?;
    let mut new_iso = DiscStream::open(File::open(&copy_path)?)?;
    let old_qp = old_iso.files.file_at("qp.bin")?;
    let new_qp = new_iso.files.file_at("qp.bin")?;
    assert_eq!(old_qp.offset, 0x3dd6e348);
    assert_eq!(new_qp.offset, 0x280000);
    let old_reader = old_iso.open_file_at("qp.bin")?;
    let new_reader = new_iso.open_file_at("qp.bin")?;
    assert!(common::compare_streams(old_reader, new_reader)?);
    Ok(())
}

fn do_resize_qp_test(new_size: u32, expected_offset: u32) -> Result<()> {
    common::init_logging();

    let copy_path = common::copy_iso()?;
    info!("Opening {}", copy_path.to_str().unwrap());
    let file = OpenOptions::new().read(true).write(true).open(&copy_path)?;
    let mut iso = DiscStream::open(file)?;
    common::check_iso(&iso)?;

    info!("Resizing qp.bin");
    let qp = iso.files.at("qp.bin")?;
    assert_eq!(iso.files.file(qp)?.offset, 0x3dd6e348);
    assert_eq!(iso.files.file(qp)?.size, 0x46b288b);
    iso.resize_file(qp, new_size)?;
    assert_eq!(iso.files.file(qp)?.offset, expected_offset);
    assert_eq!(iso.files.file(qp)?.size, new_size);
    drop(iso);

    info!("Comparing ISOs");
    let mut old_iso = common::open_iso()?;
    let mut new_iso = DiscStream::open(File::open(&copy_path)?)?;
    let old_qp = old_iso.files.file_at("qp.bin")?;
    let new_qp = new_iso.files.file_at("qp.bin")?;
    assert_eq!(old_qp.offset, 0x3dd6e348);
    assert_eq!(new_qp.offset, expected_offset);
    assert_eq!(new_qp.size, new_size);

    let compare_size = cmp::min(old_qp.size, new_size) as u64;
    let old_reader = old_iso.open_file_at("qp.bin")?;
    let new_reader = new_iso.open_file_at("qp.bin")?;
    assert!(common::compare_streams(old_reader.take(compare_size), new_reader.take(compare_size))?);
    Ok(())
}

#[test]
#[serial]
fn test_resize_disc_file_smaller() -> Result<()> {
    do_resize_qp_test(0x46b288a, 0x3dd6e348)
}

#[test]
#[serial]
fn test_resize_disc_file_larger_no_move() -> Result<()> {
    do_resize_qp_test(0x46b288c, 0x3dd6e348)
}

#[test]
#[serial]
fn test_resize_disc_file_larger_move() -> Result<()> {
    do_resize_qp_test(0x46b288d, 0x280000)
}

#[test]
#[serial]
fn test_edit_disc_file() -> Result<()> {
    common::init_logging();

    let copy_path = common::copy_iso()?;
    info!("Opening {}", copy_path.to_str().unwrap());
    let file = OpenOptions::new().read(true).write(true).open(&copy_path)?;
    let mut iso = DiscStream::open(file)?;
    common::check_iso(&iso)?;

    info!("Editing qp.bin");
    let qp = iso.files.at("qp.bin")?;
    let mut stream = iso.edit_file(qp)?;
    assert_eq!(stream.seek(SeekFrom::End(0))?, 0x46b288b);
    assert_eq!(stream.write(&[0xaa, 0xbb])?, 1);
    assert_eq!(stream.write(&[0xaa, 0xbb])?, 0);
    Ok(())
}

#[test]
#[serial]
fn test_replace_disc_file() -> Result<()> {
    common::init_logging();

    let copy_path = common::copy_iso()?;
    info!("Opening {}", copy_path.to_str().unwrap());
    let file = OpenOptions::new().read(true).write(true).open(&copy_path)?;
    let mut iso = DiscStream::open(file)?;
    common::check_iso(&iso)?;

    info!("Replacing qp.bin");
    iso.replace_file_at("qp.bin", Cursor::new(b"Hello, world!"))?;
    drop(iso);

    info!("Checking updated file");
    let mut iso = DiscStream::open(File::open(&copy_path)?)?;
    let reader = iso.open_file_at("qp.bin")?;
    assert!(common::compare_streams(reader, Cursor::new(b"Hello, world!"))?);
    Ok(())
}
