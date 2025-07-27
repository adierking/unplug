use crate::args::debug::*;

use crate::common::find_stage_file;
use crate::context::Context;
use crate::io::OutputRedirect;

use anyhow::Result;
use log::info;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::Instant;
use unplug::data::{self, Resource};
use unplug::event::{Block, Script};
use unplug::globals::{GlobalsBuilder, Libs};
use unplug::stage::Stage;

/// The `debug rebuild-scripts` CLI command.
fn command_rebuild_scripts(ctx: Context) -> Result<()> {
    let mut ctx = ctx.open_read_write()?;

    info!("Reading script globals");
    let mut globals = ctx.read_globals()?;
    let libs = globals.read_libs()?;

    let mut stages = vec![];
    for id in data::Stage::iter() {
        info!("Reading {}", id.file_name());
        let stage = ctx.read_stage(&libs, id)?;
        stages.push((id, stage));
    }

    info!("Rebuilding script globals");
    let mut update = ctx.begin_update();
    update = update.write_globals(GlobalsBuilder::new().base(&mut globals).libs(&libs))?;
    for (id, stage) in stages {
        info!("Rebuilding {}", id.file_name());
        update = update.write_stage(id, &stage)?;
    }

    info!("Updating game files");
    update.commit()?;
    Ok(())
}

fn do_dump_libs(libs: &Libs, flags: &DumpFlags, mut out: impl Write) -> Result<()> {
    for (i, id) in libs.entry_points.iter().enumerate() {
        writeln!(out, "lib[{}]: {:?}", i, id)?;
    }
    dump_script(&libs.script, flags, out)?;
    Ok(())
}

fn do_dump_stage(stage: &Stage, flags: &DumpFlags, mut out: impl Write) -> Result<()> {
    for (i, object) in stage.objects.iter().enumerate() {
        writeln!(out, "obj[{}]: {:?}", i, object)?;
    }
    writeln!(out)?;

    if flags.dump_unknown {
        writeln!(out, "settings: {:?}", stage.settings)?;
        writeln!(out)?;
        for (i, unk) in stage.unk_28.iter().enumerate() {
            writeln!(out, "unk28[{}]: {:?}", i, unk)?;
        }
        writeln!(out)?;
        for (i, unk) in stage.unk_2c.iter().enumerate() {
            writeln!(out, "unk2C[{}]: {:?}", i, unk)?;
        }
        writeln!(out)?;
        for (i, unk) in stage.unk_30.iter().enumerate() {
            writeln!(out, "unk30[{}]: {:?}", i, unk)?;
        }
        writeln!(out)?;
    }

    writeln!(out, "on_prologue: {:?}", stage.on_prologue)?;
    writeln!(out, "on_startup: {:?}", stage.on_startup)?;
    writeln!(out, "on_dead: {:?}", stage.on_dead)?;
    writeln!(out, "on_pose: {:?}", stage.on_pose)?;
    writeln!(out, "on_time_cycle: {:?}", stage.on_time_cycle)?;
    writeln!(out, "on_time_up: {:?}", stage.on_time_up)?;

    dump_script(&stage.script, flags, out)?;
    Ok(())
}

fn dump_script(script: &Script, flags: &DumpFlags, mut out: impl Write) -> Result<()> {
    write!(out, "\nDATA\n\n")?;
    if flags.no_offsets {
        writeln!(out, "id   value")?;
    } else {
        writeln!(out, "off   id   value")?;
    }
    for (location, block) in script.blocks_ordered() {
        if let Block::Data(data) = block {
            if flags.no_offsets {
                writeln!(out, "{:<4} {:?}", location.id.index(), data)?;
            } else {
                writeln!(out, "{:<5x} {:<4} {:?}", location.offset, location.id.index(), data)?;
            }
        }
    }

    write!(out, "\nCODE\n\n")?;
    if flags.no_offsets {
        writeln!(out, "id   command")?;
    } else {
        writeln!(out, "off   id   command")?;
    }
    for (location, command) in script.commands_ordered() {
        let block = location.block;
        if flags.no_offsets {
            writeln!(out, "{:<4} {:?}", block.id.index(), command)?;
        } else {
            writeln!(out, "{:<5x} {:<4} {:?}", block.offset, block.id.index(), command)?;
        }
    }
    Ok(())
}

/// The `debug dump-script` CLI command.
pub fn command_dump_script(ctx: Context, args: DumpArgs) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    let out = BufWriter::new(OutputRedirect::new(args.output)?);
    let file = find_stage_file(&mut ctx, &args.stage)?;
    info!("Reading script globals");
    let libs = ctx.read_globals()?.read_libs()?;
    info!("Dumping {}", ctx.query_file(&file)?.name);
    let stage = ctx.read_stage_file(&libs, &file)?;
    do_dump_stage(&stage, &args.flags, out)?;
    Ok(())
}

/// The `debug dump-script globals` CLI command.
pub fn command_dump_script_globals(ctx: Context, args: DumpArgs) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    let out = BufWriter::new(OutputRedirect::new(args.output)?);
    info!("Dumping script globals");
    let libs = ctx.read_globals()?.read_libs()?;
    do_dump_libs(&libs, &args.flags, out)
}

/// The `debug dump-all-scripts` CLI command.
pub fn command_dump_all_scripts(ctx: Context, args: DumpAllArgs) -> Result<()> {
    let start_time = Instant::now();
    let mut ctx = ctx.open_read()?;

    info!("Dumping script globals");
    fs::create_dir_all(&args.output)?;
    let libs = ctx.read_globals()?.read_libs()?;
    let libs_out = File::create(Path::join(&args.output, "globals.txt"))?;
    do_dump_libs(&libs, &args.flags, BufWriter::new(libs_out))?;

    for id in data::Stage::iter() {
        info!("Dumping {}", id.file_name());
        let stage = ctx.read_stage(&libs, id)?;
        let stage_out = File::create(Path::join(&args.output, format!("{}.txt", id.name())))?;
        do_dump_stage(&stage, &args.flags, BufWriter::new(stage_out))?;
    }

    info!("Dumping finished in {:?}", start_time.elapsed());
    Ok(())
}

/// The `debug` CLI command.
pub fn command(ctx: Context, command: Subcommand) -> Result<()> {
    match command {
        Subcommand::RebuildScripts => command_rebuild_scripts(ctx),
        Subcommand::DumpScript(args) if args.stage == "globals" => {
            command_dump_script_globals(ctx, args)
        }
        Subcommand::DumpScript(args) => command_dump_script(ctx, args),
        Subcommand::DumpAllScripts(args) => command_dump_all_scripts(ctx, args),
    }
}
