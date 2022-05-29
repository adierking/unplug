use crate::context::Context;
use crate::opt::DebugCommand;
use anyhow::Result;
use log::info;
use unplug::data::stage::{StageDefinition, STAGES};
use unplug::globals::GlobalsBuilder;

/// The `debug rebuild-scripts` CLI command.
fn command_rebuild_scripts(ctx: Context) -> Result<()> {
    let mut ctx = ctx.open_read_write()?;

    info!("Reading script globals");
    let mut globals = ctx.read_globals()?;
    let libs = globals.read_libs()?;

    let mut stages = vec![];
    for def in STAGES {
        info!("Reading {}.bin", def.name);
        let stage = ctx.read_stage(&libs, def.id)?;
        stages.push((def.id, stage));
    }

    info!("Rebuilding script globals");
    let mut update = ctx.begin_update();
    update = update.write_globals(GlobalsBuilder::new().base(&mut globals).libs(&libs))?;
    for (id, stage) in stages {
        info!("Rebuilding {}.bin", StageDefinition::get(id).name);
        update = update.write_stage(id, stage)?;
    }

    info!("Updating game files");
    update.commit()?;
    Ok(())
}

/// The `debug` CLI command.
pub fn command(ctx: Context, opt: DebugCommand) -> Result<()> {
    match opt {
        DebugCommand::RebuildScripts => command_rebuild_scripts(ctx),
    }
}
