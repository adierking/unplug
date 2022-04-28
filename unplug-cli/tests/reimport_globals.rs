use anyhow::Result;
use log::info;
use std::fs::File;
use tempfile::NamedTempFile;
use unplug::dvd::{ArchiveReader, DiscStream, OpenFile};
use unplug::globals::GlobalsReader;
use unplug_cli::context::Context;
use unplug_cli::globals;
use unplug_cli::opt::{ExportGlobalsOpt, ImportGlobalsOpt};
use unplug_test as common;

#[test]
fn test_reimport_metadata() -> Result<()> {
    common::init_logging();

    let copy_path = common::copy_iso()?;
    let ctx = Context::Iso(copy_path.to_path_buf());
    let json_path = NamedTempFile::new()?.into_temp_path();
    globals::export_globals(
        ctx.clone(),
        ExportGlobalsOpt { compact: false, output: Some(json_path.to_owned()) },
    )?;
    globals::import_globals(ctx, ImportGlobalsOpt { input: json_path.to_owned() })?;

    info!("Opening original ISO");
    let mut original_iso = common::open_iso()?;
    info!("Opening rebuilt ISO");
    let mut rebuilt_iso = DiscStream::open(File::open(&copy_path)?)?;
    info!("Opening original qp.bin");
    let mut original_qp = ArchiveReader::open(original_iso.open_file_at(common::QP_PATH)?)?;
    info!("Opening rebuilt qp.bin");
    let mut rebuilt_qp = ArchiveReader::open(rebuilt_iso.open_file_at(common::QP_PATH)?)?;

    let original = {
        info!("Reading original globals.bin");
        let file = original_qp.open_file_at(common::QP_GLOBALS_PATH)?;
        let mut globals = GlobalsReader::open(file)?;
        globals.read_metadata()?
    };
    let rebuilt = {
        info!("Reading rebuilt globals.bin");
        let file = rebuilt_qp.open_file_at(common::QP_GLOBALS_PATH)?;
        let mut globals = GlobalsReader::open(file)?;
        globals.read_metadata()?
    };

    info!("Comparing metadata");
    assert_eq!(original.battery_globals, rebuilt.battery_globals);
    assert_eq!(original.popper_globals, rebuilt.popper_globals);
    assert_eq!(original.copter_globals, rebuilt.copter_globals);
    assert_eq!(original.radar_globals, rebuilt.radar_globals);
    assert_eq!(original.time_limit, rebuilt.time_limit);
    assert_eq!(original.player_globals, rebuilt.player_globals);
    assert_eq!(original.default_atcs, rebuilt.default_atcs);
    assert_eq!(original.coin_values, rebuilt.coin_values);
    assert_eq!(original.pickup_sounds, rebuilt.pickup_sounds);
    assert_eq!(original.collect_sounds, rebuilt.collect_sounds);
    assert_eq!(original.items, rebuilt.items);
    assert_eq!(original.actors, rebuilt.actors);
    assert_eq!(original.atcs, rebuilt.atcs);
    assert_eq!(original.suits, rebuilt.suits);
    assert_eq!(original.stages, rebuilt.stages);
    assert_eq!(original.letickers, rebuilt.letickers);
    assert_eq!(original.stickers, rebuilt.stickers);
    assert_eq!(original.stats, rebuilt.stats);

    Ok(())
}
