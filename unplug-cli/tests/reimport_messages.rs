use anyhow::Result;
use log::info;
use std::fs::File;
use tempfile::NamedTempFile;
use unplug::data::stage::STAGES;
use unplug::dvd::{ArchiveReader, DiscStream, OpenFile};
use unplug::event::msg::MsgArgs;
use unplug::event::Script;
use unplug::globals::GlobalsReader;
use unplug::stage::Stage;
use unplug_cli::context::Context;
use unplug_cli::msg;
use unplug_cli::msg::common::{iter_messages, MessageId, MessageSource};
use unplug_cli::opt::{MessagesExportOpt, MessagesImportOpt};
use unplug_test as common;

fn collect_messages(source: MessageSource, script: &Script) -> Vec<(MessageId, &MsgArgs)> {
    let mut messages: Vec<_> = iter_messages(source, script).collect();
    messages.sort_unstable_by_key(|&(id, _)| id);
    messages
}

fn compare_messages(source: MessageSource, original: &Script, rebuilt: &Script) {
    let original_messages = collect_messages(source, original);
    let rebuilt_messages = collect_messages(source, rebuilt);
    for ((id1, msg1), (id2, msg2)) in original_messages.iter().zip(&rebuilt_messages) {
        assert_eq!(id1, id2);
        assert_eq!(msg1.commands, msg2.commands);
    }
}

#[test]
fn test_reimport_messages() -> Result<()> {
    common::init_logging();

    let copy_path = common::copy_iso()?;
    let ctx = Context::Iso(copy_path.to_path_buf());
    let xml_path = NamedTempFile::new()?.into_temp_path();
    msg::command_export(ctx.clone(), MessagesExportOpt { output: xml_path.to_owned() })?;
    msg::command_import(ctx, MessagesImportOpt { input: xml_path.to_owned() })?;

    info!("Opening original ISO");
    let mut original_iso = common::open_iso()?;
    info!("Opening rebuilt ISO");
    let mut rebuilt_iso = DiscStream::open(File::open(&copy_path)?)?;
    info!("Opening original qp.bin");
    let mut original_qp = ArchiveReader::open(original_iso.open_file_at(common::QP_PATH)?)?;
    info!("Opening rebuilt qp.bin");
    let mut rebuilt_qp = ArchiveReader::open(rebuilt_iso.open_file_at(common::QP_PATH)?)?;

    let original_libs = {
        info!("Reading original globals.bin");
        let file = original_qp.open_file_at(common::QP_GLOBALS_PATH)?;
        let mut globals = GlobalsReader::open(file)?;
        globals.read_libs()?
    };
    let rebuilt_libs = {
        info!("Reading rebuilt globals.bin");
        let file = rebuilt_qp.open_file_at(common::QP_GLOBALS_PATH)?;
        let mut globals = GlobalsReader::open(file)?;
        globals.read_libs()?
    };
    info!("Comparing globals messages");
    compare_messages(MessageSource::Globals, &original_libs.script, &rebuilt_libs.script);

    for stage_def in STAGES {
        let name = stage_def.name;
        let path = stage_def.path();
        info!("Reading original {}", name);
        let mut original_reader = original_qp.open_file_at(&path)?;
        let original_stage = Stage::read_from(&mut original_reader, &original_libs)?;
        info!("Reading rebuilt {}", name);
        let mut rebuilt_reader = rebuilt_qp.open_file_at(&path)?;
        let rebuilt_stage = Stage::read_from(&mut rebuilt_reader, &rebuilt_libs)?;
        info!("Comparing {} messages", name);
        compare_messages(
            MessageSource::Stage(stage_def.id),
            &original_stage.script,
            &rebuilt_stage.script,
        );
    }

    Ok(())
}
