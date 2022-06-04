use anyhow::Result;
use log::info;
use std::fs::File;
use tempfile::TempDir;
use unplug::data::stage::STAGES;
use unplug::dvd::{ArchiveReader, DiscStream, OpenFile};
use unplug::globals::GlobalsReader;
use unplug::stage::Stage;
use unplug_cli::commands::stage;
use unplug_cli::context::Context;
use unplug_cli::opt::{StageExportAllOpt, StageImportAllOpt};
use unplug_test as common;

#[test]
fn test_reimport_stages() -> Result<()> {
    common::init_logging();

    let copy_path = common::copy_iso()?;
    let ctx = Context::Iso(copy_path.to_path_buf());

    let json_dir = TempDir::new()?;
    stage::command_export_all(
        ctx.clone(),
        StageExportAllOpt { output: json_dir.path().to_owned() },
    )?;
    stage::command_import_all(
        ctx,
        StageImportAllOpt { input: json_dir.path().to_owned(), force: true },
    )?;
    json_dir.close()?;

    info!("Opening original ISO");
    let mut original_iso = common::open_iso()?;
    info!("Opening rebuilt ISO");
    let mut rebuilt_iso = DiscStream::open(File::open(&copy_path)?)?;
    info!("Opening original qp.bin");
    let mut original_qp = ArchiveReader::open(original_iso.open_file_at(common::QP_PATH)?)?;
    info!("Opening rebuilt qp.bin");
    let mut rebuilt_qp = ArchiveReader::open(rebuilt_iso.open_file_at(common::QP_PATH)?)?;

    let libs = {
        info!("Reading original globals.bin");
        let file = original_qp.open_file_at(common::QP_GLOBALS_PATH)?;
        GlobalsReader::open(file)?.read_libs()?
    };

    info!("Comparing stage data");
    for stage_def in STAGES {
        let name = stage_def.name;
        let path = stage_def.path();
        info!("Reading original {}", name);
        let mut original_reader = original_qp.open_file_at(&path)?;
        let original_stage = Stage::read_from(&mut original_reader, &libs)?;
        info!("Reading rebuilt {}", name);
        let mut rebuilt_reader = rebuilt_qp.open_file_at(&path)?;
        let rebuilt_stage = Stage::read_from(&mut rebuilt_reader, &libs)?;

        info!("Comparing stages");
        assert_eq!(original_stage.objects.len(), rebuilt_stage.objects.len());
        for (i, (original, rebuilt)) in
            original_stage.objects.iter().zip(&rebuilt_stage.objects).enumerate()
        {
            assert_eq!(original, rebuilt, "i = {}", i);
        }
    }
    Ok(())
}
