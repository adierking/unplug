use crate::common::*;
use crate::io::OutputRedirect;
use crate::opt::*;
use anyhow::Result;
use log::{debug, info};
use std::convert::TryFrom;
use std::fs::{self, File};
use std::io::{BufWriter, Read, Seek, Write};
use std::path::Path;
use std::time::Instant;
use unicase::UniCase;
use unplug::common::io::{copy_buffered, BUFFER_SIZE};
use unplug::data::object::ObjectId;
use unplug::data::stage::{StageDefinition, STAGES};
use unplug::dvd::{ArchiveReader, DiscStream, Entry, FileEntry, FileTree, OpenFile};
use unplug::event::{Block, Script};
use unplug::globals::Libs;
use unplug::stage::Stage;

fn list_files(tree: &FileTree, opt: ListOpt) -> Result<()> {
    let mut files: Vec<(String, &FileEntry)> =
        tree.recurse().filter_map(|(p, e)| tree[e].file().map(|f| (p, f))).collect();
    if opt.by_offset {
        files.sort_unstable_by_key(|(_, f)| f.offset);
    } else if opt.by_size {
        files.sort_unstable_by_key(|(_, f)| f.size);
    } else {
        files.sort_unstable_by(|(p1, _), (p2, _)| UniCase::new(p1).cmp(&UniCase::new(p2)));
    }
    if opt.reverse {
        files.reverse();
    }
    for (path, file) in files {
        if opt.long {
            println!("{:<8x} {:<8x} {}", file.offset, file.size, path);
        } else {
            println!("{}", path);
        }
    }
    Ok(())
}

pub fn list_archive(opt: ListArchiveOpt) -> Result<()> {
    let file = File::open(opt.path)?;
    let archive = ArchiveReader::open(file)?;
    list_files(&archive.files, opt.settings)
}

pub fn list_iso(opt: ListIsoOpt) -> Result<()> {
    let file = File::open(opt.path)?;
    let iso = DiscStream::open(file)?;
    println!("Game ID: {}", iso.game_id());
    list_files(&iso.files, opt.settings)
}

fn extract_files(
    mut reader: (impl Read + Seek),
    tree: &FileTree,
    output: &Path,
    iobuf: &mut [u8],
) -> Result<()> {
    fs::create_dir_all(&output)?;
    for (path, id) in tree.recurse() {
        let out_path = Path::new(output).join(&path);
        match &tree[id] {
            Entry::File(file) => {
                info!("Extracting {}", path);
                let mut file_writer = File::create(out_path)?;
                let mut file_reader = file.open(&mut reader)?;
                copy_buffered(&mut file_reader, &mut file_writer, iobuf)?;
            }
            Entry::Directory(_) => {
                fs::create_dir_all(out_path)?;
            }
        }
    }
    Ok(())
}

pub fn extract_archive(opt: ExtractArchiveOpt) -> Result<()> {
    let mut iso = open_iso_optional(opt.iso.as_ref())?;
    let mut qp = ArchiveReader::open(match &mut iso {
        Some(iso) => iso.open_file_at(opt.path.to_str().unwrap())?,
        None => Box::new(File::open(opt.path)?),
    })?;

    let mut buf = vec![0u8; BUFFER_SIZE].into_boxed_slice();
    let start_time = Instant::now();
    extract_files(&mut qp.reader, &qp.files, &opt.output, &mut buf)?;

    debug!("Extraction finished in {:?}", start_time.elapsed());
    Ok(())
}

pub fn extract_iso(opt: ExtractIsoOpt) -> Result<()> {
    let file = File::open(opt.path)?;
    let mut iso = DiscStream::open(file)?;

    let mut buf = vec![0u8; BUFFER_SIZE].into_boxed_slice();
    let start_time = Instant::now();
    extract_files(&mut iso.stream, &iso.files, &opt.output, &mut buf)?;

    info!("Extracting main.dol");
    let dol_path = Path::new(&opt.output).join("main.dol");
    let mut dol_writer = File::create(dol_path)?;
    let (_, mut dol_reader) = iso.open_dol()?;
    copy_buffered(&mut dol_reader, &mut dol_writer, &mut buf)?;

    debug!("Extraction finished in {:?}", start_time.elapsed());
    Ok(())
}

fn dump_script(mut out: impl Write, script: &Script) -> Result<()> {
    info!("Dumping script");

    write!(out, "\nDATA\n\n")?;
    writeln!(out, "off   id   value")?;
    for (location, block) in script.blocks_ordered() {
        if let Block::Data(data) = block {
            writeln!(out, "{:<5x} {:<4} {:?}", location.offset, location.id.index(), data)?;
        }
    }

    write!(out, "\nCODE\n\n")?;
    writeln!(out, "off   id   command")?;
    for (location, command) in script.commands_ordered() {
        let block = location.block;
        writeln!(out, "{:<5x} {:<4} {:?}", block.offset, block.id.index(), command)?;
    }
    Ok(())
}

fn do_dump_stage(stage: &Stage, flags: &DumpStageFlags, mut out: impl Write) -> Result<()> {
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

    dump_script(out, &stage.script)?;
    Ok(())
}

pub fn dump_stage(opt: DumpStageOpt) -> Result<()> {
    let out = BufWriter::new(OutputRedirect::new(opt.output)?);

    let mut iso = open_iso_optional(opt.container.iso.as_ref())?;
    let mut qp = open_qp_optional(iso.as_mut(), &opt.container)?;

    info!("Reading script globals");
    let libs = {
        let mut globals = read_globals_qp_or_file(qp.as_mut(), opt.globals.path)?;
        globals.read_libs()?
    };

    info!("Reading stage file");
    let stage = read_stage_qp_or_file(qp.as_mut(), opt.stage.name, &libs)?;
    do_dump_stage(&stage, &opt.flags, out)?;
    Ok(())
}

fn do_dump_libs(libs: &Libs, mut out: impl Write) -> Result<()> {
    for (i, id) in libs.entry_points.iter().enumerate() {
        writeln!(out, "lib[{}]: {:?}", i, id)?;
    }
    dump_script(out, &libs.script)?;
    Ok(())
}

pub fn dump_libs(opt: DumpLibsOpt) -> Result<()> {
    let out = BufWriter::new(OutputRedirect::new(opt.output)?);

    let mut iso = open_iso_optional(opt.container.iso.as_ref())?;
    let mut qp = open_qp_optional(iso.as_mut(), &opt.container)?;

    info!("Reading script globals");
    let mut globals = read_globals_qp_or_file(qp.as_mut(), opt.globals.path)?;
    let libs = globals.read_libs()?;
    do_dump_libs(&libs, out)
}

pub fn dump_all_stages(opt: DumpAllStagesOpt) -> Result<()> {
    let mut iso = open_iso_optional(opt.container.iso.as_ref())?;
    let mut qp = open_qp_required(iso.as_mut(), &opt.container)?;

    let start_time = Instant::now();
    info!("Reading script globals");
    let libs = {
        let mut globals = read_globals_qp(&mut qp)?;
        globals.read_libs()?
    };
    fs::create_dir_all(&opt.output)?;
    let libs_out = File::create(Path::join(&opt.output, "globals.txt"))?;
    do_dump_libs(&libs, BufWriter::new(libs_out))?;

    for &stage_id in STAGES {
        let stage_def = StageDefinition::get(stage_id);
        info!("Reading {}.bin", stage_def.name);
        let stage = read_stage_qp(&mut qp, stage_def.name, &libs)?;
        let stage_out = File::create(Path::join(&opt.output, format!("{}.txt", stage_def.name)))?;
        do_dump_stage(&stage, &opt.flags, BufWriter::new(stage_out))?;
    }

    info!("Dumping finished in {:?}", start_time.elapsed());
    Ok(())
}

pub fn dump_colliders(opt: DumpCollidersOpt) -> Result<()> {
    let mut out = BufWriter::new(OutputRedirect::new(opt.output)?);

    let mut iso = open_iso_optional(opt.container.iso.as_ref())?;
    let mut qp = open_qp_optional(iso.as_mut(), &opt.container)?;

    info!("Reading collider globals");
    let colliders = {
        let mut globals = read_globals_qp_or_file(qp.as_mut(), opt.globals.path)?;
        globals.read_colliders()?
    };

    info!("Dumping colliders");
    for (obj, list) in colliders.objects.iter().enumerate() {
        writeln!(out, "Object {:?} ({}):", ObjectId::try_from(obj as i32)?, obj)?;
        for (i, collider) in list.iter().enumerate() {
            writeln!(out, "{:>2} {:?}", i, collider)?;
        }
        writeln!(out)?;
    }

    Ok(())
}
