use crate::context::Context;
use crate::msg::{self, MessageId, MessageReader, MessageSource, MessageWriter};
use crate::opt::{MessagesCommand, MessagesExportOpt, MessagesImportOpt};
use anyhow::Result;
use log::{info, warn};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, BufWriter, Cursor};
use unplug::common::WriteTo;
use unplug::data::stage::{StageDefinition, GLOBALS_PATH, STAGES};
use unplug::event::msg::MsgArgs;
use unplug::event::Script;
use unplug::globals::GlobalsBuilder;

/// Replaces messages in `script` using `messages`. After a message is replaced, it is removed from
/// the map.
fn apply_messages(
    source: MessageSource,
    script: &mut Script,
    messages: &mut HashMap<MessageId, MsgArgs>,
) {
    for (id, old_message) in msg::iter_messages_mut(source, script) {
        if let Some(new_message) = messages.remove(&id) {
            *old_message = new_message;
        }
    }
}

/// The `messages export` CLI command.
pub fn command_export(ctx: Context, opt: MessagesExportOpt) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    info!("Reading script globals");
    let libs = ctx.read_globals()?.read_libs()?;

    let out_file = BufWriter::new(File::create(opt.output)?);
    let mut writer = MessageWriter::new(out_file);
    writer.start()?;
    writer.write_script(MessageSource::Globals, &libs.script)?;

    for def in STAGES {
        info!("Reading {}.bin", def.name);
        let stage = ctx.read_stage(&libs, def.id)?;
        writer.write_script(MessageSource::Stage(def.id), &stage.script)?;
    }

    writer.finish()?;
    Ok(())
}

/// The `messages import` CLI command.
pub fn command_import(ctx: Context, opt: MessagesImportOpt) -> Result<()> {
    let mut ctx = ctx.open_read_write()?;
    info!("Reading messages from {}", opt.input.to_str().unwrap());
    let file = BufReader::new(File::open(opt.input)?);
    let mut reader = MessageReader::new(file);
    reader.read_header()?;
    let mut messages = HashMap::new();
    let mut sources = HashSet::new();
    while let Some((id, mut msg)) = reader.read_message()? {
        sources.insert(id.source);
        msg.extra_data = msg::encode_block_offset(id.block_offset).to_vec();
        messages.insert(id, msg);
    }
    reader.read_footer()?;
    if messages.is_empty() {
        info!("No messages read - stopping");
        return Ok(());
    }
    let mut sources: Vec<_> = sources.into_iter().collect();
    sources.sort_unstable();

    info!("Reading script globals");
    let mut globals = ctx.read_globals()?;
    let mut rebuilt_files = vec![];
    let mut libs = globals.read_libs()?;
    if sources[0] == MessageSource::Globals {
        info!("Rebuilding globals.bin");
        apply_messages(MessageSource::Globals, &mut libs.script, &mut messages);
        let mut writer = Cursor::new(vec![]);
        GlobalsBuilder::new().base(&mut globals).libs(&libs).write_to(&mut writer)?;
        let bytes = writer.into_inner().into_boxed_slice();
        rebuilt_files.push((GLOBALS_PATH.to_owned(), bytes));
    }
    drop(globals);

    for source in sources {
        let stage_id = match source {
            MessageSource::Globals => continue,
            MessageSource::Stage(id) => id,
        };
        let stage_def = StageDefinition::get(stage_id);
        info!("Rebuilding {}.bin", stage_def.name);
        let mut stage = ctx.read_stage(&libs, stage_id)?;
        apply_messages(source, &mut stage.script, &mut messages);
        let mut writer = Cursor::new(vec![]);
        stage.write_to(&mut writer)?;
        let bytes = writer.into_inner().into_boxed_slice();
        rebuilt_files.push((stage_def.path(), bytes));
    }

    if !messages.is_empty() {
        let mut unused_ids: Vec<_> = messages.into_iter().map(|(k, _)| k).collect();
        unused_ids.sort_unstable();
        for id in unused_ids {
            warn!("Message was not found: {}", id.to_string());
        }
    }

    info!("Updating game files");
    let mut writer = ctx.begin_update();
    for (path, bytes) in rebuilt_files {
        writer = writer.write_qp_file_at(&path, Cursor::new(bytes))?;
    }
    writer.commit()?;
    Ok(())
}

/// The `messages` CLI command.
pub fn command(ctx: Context, opt: MessagesCommand) -> Result<()> {
    match opt {
        MessagesCommand::Export(opt) => command_export(ctx, opt),
        MessagesCommand::Import(opt) => command_import(ctx, opt),
    }
}
