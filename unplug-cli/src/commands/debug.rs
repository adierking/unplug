use crate::context::Context;
use crate::opt::DebugCommand;
use anyhow::Result;
use log::info;
use unplug::data::Stage;
use unplug::globals::GlobalsBuilder;

/// The `debug rebuild-scripts` CLI command.
fn command_rebuild_scripts(ctx: Context) -> Result<()> {
    let mut ctx = ctx.open_read_write()?;

    info!("Reading script globals");
    let mut globals = ctx.read_globals()?;
    let libs = globals.read_libs()?;

    let mut stages = vec![];
    for id in Stage::all() {
        info!("Reading {}.bin", id.name());
        let stage = ctx.read_stage(&libs, id)?;
        stages.push((id, stage));
    }

    info!("Rebuilding script globals");
    let mut update = ctx.begin_update();
    update = update.write_globals(GlobalsBuilder::new().base(&mut globals).libs(&libs))?;
    for (id, stage) in stages {
        info!("Rebuilding {}.bin", id.name());
        update = update.write_stage(id, &stage)?;
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
