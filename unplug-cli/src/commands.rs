use crate::context::{Context, FileId, OpenContext};
use crate::id::IdString;
use crate::io::OutputRedirect;
use crate::opt::*;
use anyhow::{bail, Result};
use log::{debug, info};
use std::convert::TryFrom;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::Instant;
use unicase::UniCase;
use unplug::common::io::{copy_buffered, BUFFER_SIZE};
use unplug::common::ReadSeek;
use unplug::data::atc::ATCS;
use unplug::data::item::{ItemFlags, ITEMS};
use unplug::data::object::Object;
use unplug::data::stage::{StageDefinition, STAGES};
use unplug::dvd::{ArchiveReader, DiscStream, Entry, FileEntry, FileTree};
use unplug::event::{Block, Script};
use unplug::globals::Libs;
use unplug::stage::Stage;

const UNKNOWN_ID_PREFIX: &str = "unk";

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

fn find_stage_file<T: ReadSeek>(ctx: &mut OpenContext<T>, name: &str) -> Result<FileId> {
    match ctx.explicit_file_at(name)? {
        Some(id) => Ok(id),
        None => match StageDefinition::find(name) {
            Some(def) => ctx.qp_file_at(def.path()),
            None => bail!("Unrecognized stage \"{}\"", name),
        },
    }
}

pub fn list_archive(ctx: Context, opt: ListArchiveOpt) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    info!("Reading {}", opt.path);
    let file = ctx.open_file_at(&opt.path)?;
    let archive = ArchiveReader::open(file)?;
    list_files(&archive.files, opt.settings)
}

pub fn list_iso(_ctx: Context, opt: ListIsoOpt) -> Result<()> {
    let file = File::open(opt.path)?;
    let iso = DiscStream::open(file)?;
    println!("Game ID: {}", iso.game_id());
    list_files(&iso.files, opt.settings)
}

fn sort_ids<I: IdString + Ord>(ids: &mut [I], settings: &ListIdsOpt) {
    if settings.by_id {
        ids.sort_unstable();
    } else {
        ids.sort_unstable_by_key(|i| i.to_id());
    }
    if settings.reverse {
        ids.reverse();
    }
}

pub fn list_items(_ctx: Context, opt: ListItemsOpt) -> Result<()> {
    let mut items: Vec<_> = if opt.show_unknown {
        ITEMS.iter().map(|i| i.id).collect()
    } else {
        ITEMS.iter().filter(|i| !i.flags.contains(ItemFlags::UNUSED)).map(|i| i.id).collect()
    };
    sort_ids(&mut items, &opt.settings);
    for item in items {
        println!("[{:>3}] {}", i16::from(item), item.to_id());
    }
    Ok(())
}

pub fn list_equipment(_ctx: Context, opt: ListEquipmentOpt) -> Result<()> {
    let mut atcs: Vec<_> = ATCS.iter().map(|a| a.id).collect();
    sort_ids(&mut atcs, &opt.settings);
    for atc in atcs {
        let name = atc.to_id();
        if opt.show_unknown || !name.starts_with(UNKNOWN_ID_PREFIX) {
            println!("[{:>1}] {}", i16::from(atc), atc.to_id());
        }
    }
    Ok(())
}

pub fn list_stages(_ctx: Context, opt: ListStagesOpt) -> Result<()> {
    let mut stages: Vec<_> = STAGES.iter().map(|s| s.id).collect();
    sort_ids(&mut stages, &opt.settings);
    for stage in stages {
        let name = stage.to_id();
        println!("[{:>3}] {}", i32::from(stage), name);
    }
    Ok(())
}

fn extract_files(
    mut reader: impl ReadSeek,
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

pub fn extract_archive(ctx: Context, opt: ExtractArchiveOpt) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    info!("Reading {}", opt.path);
    let file = ctx.open_file_at(&opt.path)?;
    let mut qp = ArchiveReader::open(file)?;

    let mut buf = vec![0u8; BUFFER_SIZE].into_boxed_slice();
    let start_time = Instant::now();
    extract_files(&mut qp.reader, &qp.files, &opt.output, &mut buf)?;

    debug!("Extraction finished in {:?}", start_time.elapsed());
    Ok(())
}

pub fn extract_iso(_ctx: Context, opt: ExtractIsoOpt) -> Result<()> {
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

pub fn dump_stage(ctx: Context, opt: DumpStageOpt) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    let out = BufWriter::new(OutputRedirect::new(opt.output)?);
    let file = find_stage_file(&mut ctx, &opt.stage.name)?;
    info!("Reading script globals");
    let libs = ctx.read_globals()?.read_libs()?;
    info!("Dumping {}", ctx.query_file(&file)?.name);
    let stage = ctx.read_stage_file(&libs, &file)?;
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

pub fn dump_libs(ctx: Context, opt: DumpLibsOpt) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    let out = BufWriter::new(OutputRedirect::new(opt.output)?);
    info!("Dumping script globals");
    let libs = ctx.read_globals()?.read_libs()?;
    do_dump_libs(&libs, out)
}

pub fn dump_all_stages(ctx: Context, opt: DumpAllStagesOpt) -> Result<()> {
    let start_time = Instant::now();
    let mut ctx = ctx.open_read()?;

    info!("Dumping script globals");
    fs::create_dir_all(&opt.output)?;
    let libs = ctx.read_globals()?.read_libs()?;
    let libs_out = File::create(Path::join(&opt.output, "globals.txt"))?;
    do_dump_libs(&libs, BufWriter::new(libs_out))?;

    for stage_def in STAGES {
        info!("Dumping {}.bin", stage_def.name);
        let stage = ctx.read_stage(&libs, stage_def.id)?;
        let stage_out = File::create(Path::join(&opt.output, format!("{}.txt", stage_def.name)))?;
        do_dump_stage(&stage, &opt.flags, BufWriter::new(stage_out))?;
    }

    info!("Dumping finished in {:?}", start_time.elapsed());
    Ok(())
}

pub fn dump_colliders(ctx: Context, opt: DumpCollidersOpt) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    let mut out = BufWriter::new(OutputRedirect::new(opt.output)?);
    info!("Dumping collider globals");
    let colliders = ctx.read_globals()?.read_colliders()?;
    for (obj, list) in colliders.objects.iter().enumerate() {
        writeln!(out, "Object {:?} ({}):", Object::try_from(obj as i32)?, obj)?;
        for (i, collider) in list.iter().enumerate() {
            writeln!(out, "{:>2} {:?}", i, collider)?;
        }
        writeln!(out)?;
    }
    Ok(())
}
