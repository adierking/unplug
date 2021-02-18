use anyhow::Result;
use log::info;
use serial_test::serial;
use std::io::{Read, Seek, SeekFrom};
use tempfile::tempfile;
use unplug::common::Text;
use unplug::data::object::ObjectId;
use unplug::dvd::{ArchiveReader, OpenFile};
use unplug::event::command::PrintFArgs;
use unplug::event::{Block, CodeBlock, Command};
use unplug::globals::{Collider, GlobalsBuilder, GlobalsReader, Material, Shape};
use unplug_test as common;

#[test]
#[serial]
fn test_rebuild_globals_copy_all() -> Result<()> {
    common::init_logging();

    let mut iso = common::open_iso()?;
    let mut qp = ArchiveReader::open(iso.open_file_at(common::QP_PATH)?)?;

    info!("Opening {}", common::QP_GLOBALS_PATH);
    let mut original = GlobalsReader::open(qp.open_file_at(common::QP_GLOBALS_PATH)?)?;

    let mut temp = tempfile()?;
    info!("Building new globals");
    GlobalsBuilder::new().base(&mut original).write_to(&mut temp)?;

    info!("Comparing files");

    let mut original = original.into_inner();
    original.seek(SeekFrom::Start(0))?;
    let mut original_bytes = vec![];
    original.read_to_end(&mut original_bytes)?;

    temp.seek(SeekFrom::Start(0))?;
    let mut rebuilt_bytes = vec![];
    temp.read_to_end(&mut rebuilt_bytes)?;
    assert!(original_bytes == rebuilt_bytes);

    Ok(())
}

#[test]
#[serial]
fn test_rebuild_globals_metadata() -> Result<()> {
    common::init_logging();

    let mut iso = common::open_iso()?;
    let mut qp = ArchiveReader::open(iso.open_file_at(common::QP_PATH)?)?;

    info!("Reading {}", common::QP_GLOBALS_PATH);
    let mut original = GlobalsReader::open(qp.open_file_at(common::QP_GLOBALS_PATH)?)?;
    let mut original_metadata = original.read_metadata()?;

    // Change an item name so we know it isn't just copied
    info!("Building new globals with altered metadata");
    original_metadata.items[0].name = Text::encode("test").unwrap();

    let mut temp = tempfile()?;
    GlobalsBuilder::new().base(&mut original).metadata(&original_metadata).write_to(&mut temp)?;

    info!("Reading new metadata");
    let mut rebuilt = GlobalsReader::open(temp)?;
    let rebuilt_metadata = rebuilt.read_metadata()?;
    assert_eq!(original_metadata, rebuilt_metadata);

    info!("Comparing other partitions");
    assert!(common::compare_streams(
        &mut original.open_colliders()?,
        &mut rebuilt.open_colliders()?
    )?);
    assert!(common::compare_streams(&mut original.open_libs()?, &mut rebuilt.open_libs()?)?);

    Ok(())
}

#[test]
#[serial]
fn test_rebuild_globals_colliders() -> Result<()> {
    common::init_logging();

    let mut iso = common::open_iso()?;
    let mut qp = ArchiveReader::open(iso.open_file_at(common::QP_PATH)?)?;

    info!("Reading {}", common::QP_GLOBALS_PATH);
    let mut original = GlobalsReader::open(qp.open_file_at(common::QP_GLOBALS_PATH)?)?;
    let mut original_colliders = original.read_colliders()?;

    // Add an extra collider so that we know it isn't just copied
    info!("Building new globals with altered colliders");
    original_colliders[ObjectId::CbRobo].push(Collider {
        shape: Shape::Sphere,
        material: Material::Water,
        x: 1,
        y: 2,
        z: 3,
        rotate_y: 4,
        scale_x: 5,
        scale_y: 6,
        scale_z: 7,
    });

    let mut temp = tempfile()?;
    GlobalsBuilder::new().base(&mut original).colliders(&original_colliders).write_to(&mut temp)?;

    info!("Reading new colliders");
    let mut rebuilt = GlobalsReader::open(temp)?;
    let rebuilt_colliders = rebuilt.read_colliders()?;
    for (original, rebuilt) in original_colliders.objects.iter().zip(&*rebuilt_colliders.objects) {
        assert_eq!(original, rebuilt);
    }

    info!("Comparing other partitions");
    assert!(common::compare_streams(
        &mut original.open_metadata()?,
        &mut rebuilt.open_metadata()?
    )?);
    assert!(common::compare_streams(&mut original.open_libs()?, &mut rebuilt.open_libs()?)?);

    Ok(())
}

#[test]
#[serial]
fn test_rebuild_globals_libs() -> Result<()> {
    common::init_logging();

    let mut iso = common::open_iso()?;
    let mut qp = ArchiveReader::open(iso.open_file_at(common::QP_PATH)?)?;

    info!("Reading {}", common::QP_GLOBALS_PATH);
    let mut original = GlobalsReader::open(qp.open_file_at(common::QP_GLOBALS_PATH)?)?;
    let mut original_libs = original.read_libs()?;

    // Change one of the blocks so we know it isn't just copied
    info!("Building new globals with altered libs");
    let new_block = Block::Code(CodeBlock {
        commands: vec![
            Command::PrintF(PrintFArgs(Text::encode("bunger")?).into()),
            Command::Return,
        ],
        next_block: None,
        else_block: None,
    });
    *original_libs.script.block_mut(original_libs.entry_points[361]) = new_block.clone();

    let mut temp = tempfile()?;
    GlobalsBuilder::new().base(&mut original).libs(&original_libs).write_to(&mut temp)?;

    info!("Reading new libs");
    let mut rebuilt = GlobalsReader::open(temp)?;
    let rebuilt_libs = rebuilt.read_libs()?;
    assert_eq!(original_libs.script.len(), rebuilt_libs.script.len());
    assert_eq!(
        new_block.commands(),
        rebuilt_libs.script.block(rebuilt_libs.entry_points[361]).commands()
    );

    info!("Comparing other partitions");
    assert!(common::compare_streams(
        &mut original.open_metadata()?,
        &mut rebuilt.open_metadata()?
    )?);
    assert!(common::compare_streams(
        &mut original.open_colliders()?,
        &mut rebuilt.open_colliders()?
    )?);

    Ok(())
}
