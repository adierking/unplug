use crate::common::find_stage_file;
use crate::context::Context;
use crate::io::OutputRedirect;
use crate::opt::{
    ScriptAssembleOpt, ScriptCommand, ScriptDisassembleAllOpt, ScriptDisassembleOpt,
    ScriptDumpAllOpt, ScriptDumpFlags, ScriptDumpOpt,
};
use anyhow::{anyhow, bail, Result};
use log::error;
use log::info;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::Instant;
use unplug::data::{Resource, Stage as StageId};
use unplug::event::{Block, Script};
use unplug::globals::{GlobalsBuilder, Libs};
use unplug::stage::Stage;
use unplug_asm as asm;
use unplug_asm::assembler::ProgramAssembler;
use unplug_asm::lexer::{Logos, Token};
use unplug_asm::parser::{Ast, Parser, Stream};
use unplug_asm::program::Target;

fn do_dump_libs(libs: &Libs, flags: &ScriptDumpFlags, mut out: impl Write) -> Result<()> {
    for (i, id) in libs.entry_points.iter().enumerate() {
        writeln!(out, "lib[{}]: {:?}", i, id)?;
    }
    dump_script(&libs.script, flags, out)?;
    Ok(())
}

fn do_dump_stage(stage: &Stage, flags: &ScriptDumpFlags, mut out: impl Write) -> Result<()> {
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

fn dump_script(script: &Script, flags: &ScriptDumpFlags, mut out: impl Write) -> Result<()> {
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

/// The `script dump` CLI command.
pub fn command_dump(ctx: Context, opt: ScriptDumpOpt) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    let out = BufWriter::new(OutputRedirect::new(opt.output)?);
    let file = find_stage_file(&mut ctx, &opt.stage)?;
    info!("Reading script globals");
    let libs = ctx.read_globals()?.read_libs()?;
    info!("Dumping {}", ctx.query_file(&file)?.name);
    let stage = ctx.read_stage_file(&libs, &file)?;
    do_dump_stage(&stage, &opt.flags, out)?;
    Ok(())
}

/// The `script dump globals` CLI command.
pub fn command_dump_globals(ctx: Context, opt: ScriptDumpOpt) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    let out = BufWriter::new(OutputRedirect::new(opt.output)?);
    info!("Dumping script globals");
    let libs = ctx.read_globals()?.read_libs()?;
    do_dump_libs(&libs, &opt.flags, out)
}

/// The `script dump-all` CLI command.
pub fn command_dump_all(ctx: Context, opt: ScriptDumpAllOpt) -> Result<()> {
    let start_time = Instant::now();
    let mut ctx = ctx.open_read()?;

    info!("Dumping script globals");
    fs::create_dir_all(&opt.output)?;
    let libs = ctx.read_globals()?.read_libs()?;
    let libs_out = File::create(Path::join(&opt.output, "globals.txt"))?;
    do_dump_libs(&libs, &opt.flags, BufWriter::new(libs_out))?;

    for id in StageId::iter() {
        info!("Dumping {}", id.file_name());
        let stage = ctx.read_stage(&libs, id)?;
        let stage_out = File::create(Path::join(&opt.output, format!("{}.txt", id.name())))?;
        do_dump_stage(&stage, &opt.flags, BufWriter::new(stage_out))?;
    }

    info!("Dumping finished in {:?}", start_time.elapsed());
    Ok(())
}

fn command_disassemble(ctx: Context, opt: ScriptDisassembleOpt) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    let out = BufWriter::new(File::create(opt.output)?);
    let file = find_stage_file(&mut ctx, &opt.stage)?;
    let info = ctx.query_file(&file)?;

    info!("Reading script globals");
    let libs = ctx.read_globals()?.read_libs()?;

    info!("Disassembling {}", ctx.query_file(&file)?.name);
    let stage = ctx.read_stage_file(&libs, &file)?;
    let name = info.name.rsplit_once('.').unwrap_or((&info.name, "")).0;
    let program = asm::disassemble_stage(&stage, name)?;
    asm::write_program(&program, out)?;
    Ok(())
}

pub fn command_disassemble_all(ctx: Context, opt: ScriptDisassembleAllOpt) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    fs::create_dir_all(&opt.output)?;

    info!("Disassembling script globals");
    let libs = ctx.read_globals()?.read_libs()?;
    let libs_out = Path::join(&opt.output, "globals.us");
    let libs_writer = BufWriter::new(File::create(libs_out)?);
    let libs_program = asm::disassemble_globals(&libs)?;
    asm::write_program(&libs_program, libs_writer)?;

    for id in StageId::iter() {
        info!("Disassembling {}", id.file_name());
        let stage = ctx.read_stage(&libs, id)?;
        let out_path = Path::join(&opt.output, format!("{}.us", id.name()));
        let writer = BufWriter::new(File::create(out_path)?);
        let program = asm::disassemble_stage(&stage, id.name())?;
        asm::write_program(&program, writer)?;
    }
    Ok(())
}

/// The `script assemble` CLI command.
fn command_assemble(ctx: Context, opt: ScriptAssembleOpt) -> Result<()> {
    let mut ctx = ctx.open_read_write()?;

    let name = opt.path.file_name().unwrap_or_default().to_string_lossy();
    info!("Parsing {}", name);
    let source = fs::read_to_string(&opt.path)?;
    let len = source.len();
    let lexer = Token::lexer(&source);
    let stream = Stream::from_iter(len..len + 1, lexer.spanned());
    let ast = match Ast::parser().parse(stream) {
        Ok(ast) => ast,
        Err(errors) => {
            // TODO: Make this suck less
            for error in &errors {
                error!("{}: {}", name, error);
            }
            return match errors.len() {
                1 => Err(anyhow!("1 syntax error found")),
                e => Err(anyhow!("{} syntax errors found", e)),
            };
        }
    };

    info!("Assembling script");
    let program = ProgramAssembler::new(&ast).assemble()?;
    let compiled = asm::compile(&program)?;
    let update = match &compiled.target {
        Some(Target::Globals) => {
            let libs = compiled.into_libs()?;
            let mut globals = ctx.read_globals()?;
            ctx.begin_update()
                .write_globals(GlobalsBuilder::new().base(&mut globals).libs(&libs))?
        }
        Some(Target::Stage(stage_name)) => {
            let stage_id = StageId::find(stage_name)
                .ok_or_else(|| anyhow!("Unknown stage \"{stage_name}\""))?;
            let libs = ctx.read_globals()?.read_libs()?;
            let mut stage = ctx.read_stage(&libs, stage_id)?;
            stage = compiled.into_stage(stage)?;
            ctx.begin_update().write_stage(stage_id, &stage)?
        }
        None => {
            bail!("The script does not have a .globals or .stage directive");
        }
    };

    info!("Updating game files");
    update.commit()?;
    Ok(())
}

/// The `script` CLI command.
pub fn command(ctx: Context, opt: ScriptCommand) -> Result<()> {
    match opt {
        ScriptCommand::Dump(opt) if opt.stage == "globals" => command_dump_globals(ctx, opt),
        ScriptCommand::Dump(opt) => command_dump(ctx, opt),
        ScriptCommand::DumpAll(opt) => command_dump_all(ctx, opt),
        ScriptCommand::Disassemble(opt) => command_disassemble(ctx, opt),
        ScriptCommand::DisassembleAll(opt) => command_disassemble_all(ctx, opt),
        ScriptCommand::Assemble(opt) => command_assemble(ctx, opt),
    }
}
