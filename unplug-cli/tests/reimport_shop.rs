use anyhow::Result;
use log::info;
use std::fs::File;
use std::io::BufReader;
use tempfile::NamedTempFile;
use unplug::data::Stage as StageId;
use unplug::dvd::{ArchiveReader, DiscStream, OpenFile};
use unplug::globals::GlobalsReader;
use unplug::shop::Shop;
use unplug::stage::Stage;
use unplug_cli::commands::shop;
use unplug_cli::context::Context;
use unplug_cli::opt::{ShopExportOpt, ShopImportOpt};
use unplug_test as common;

#[test]
fn test_reimport_shop() -> Result<()> {
    common::init_logging();

    let copy_path = common::copy_iso()?;
    let ctx = Context::Iso(copy_path.to_path_buf());
    let json_path = NamedTempFile::new()?.into_temp_path();
    shop::command_export(
        ctx.clone(),
        ShopExportOpt { compact: false, output: Some(json_path.to_owned()) },
    )?;
    shop::command_import(ctx, ShopImportOpt { input: json_path.to_owned() })?;

    info!("Opening original ISO");
    let mut original_iso = common::open_iso()?;
    info!("Opening rebuilt ISO");
    let mut rebuilt_iso = DiscStream::open(File::open(&copy_path)?)?;
    info!("Opening original qp.bin");
    let mut original_qp = ArchiveReader::open(original_iso.open_file_at(common::QP_PATH)?)?;
    info!("Opening rebuilt qp.bin");
    let mut rebuilt_qp = ArchiveReader::open(rebuilt_iso.open_file_at(common::QP_PATH)?)?;

    info!("Reading original globals.bin");
    let file = original_qp.open_file_at(common::QP_GLOBALS_PATH)?;
    let mut original_globals = GlobalsReader::open(file)?;
    let original_metadata = original_globals.read_metadata()?;
    let original_libs = original_globals.read_libs()?;
    drop(original_globals);

    info!("Reading rebuilt globals.bin");
    let file = rebuilt_qp.open_file_at(common::QP_GLOBALS_PATH)?;
    let mut rebuilt_globals = GlobalsReader::open(file)?;
    let rebuilt_metadata = rebuilt_globals.read_metadata()?;
    let rebuilt_libs = rebuilt_globals.read_libs()?;
    drop(rebuilt_globals);

    let chibi_house = StageId::ChibiHouse;
    info!("Reading original {}", chibi_house.file_name());
    let file = original_qp.open_file_at(&chibi_house.qp_path())?;
    let original_stage = Stage::read_from(&mut BufReader::new(file), &original_libs)?;

    info!("Reading rebuilt {}", chibi_house.file_name());
    let file = rebuilt_qp.open_file_at(&chibi_house.qp_path())?;
    let rebuilt_stage = Stage::read_from(&mut BufReader::new(file), &rebuilt_libs)?;

    info!("Parsing original shop code");
    let original = Shop::parse(&original_stage.script)?;
    info!("Parsing rebuilt shop code");
    let rebuilt = Shop::parse(&rebuilt_stage.script)?;

    info!("Comparing shops");
    assert_eq!(original.slots().len(), rebuilt.slots().len());
    for (i, (actual, expected)) in rebuilt.slots().iter().zip(original.slots()).enumerate() {
        assert_eq!(actual, expected, "slot {}", i);
    }

    info!("Comparing prices");
    for (actual, expected) in rebuilt_metadata.items.iter().zip(&*original_metadata.items) {
        assert_eq!(actual.price, expected.price);
    }

    Ok(())
}
