use crate::common::output_dir_and_name;
use crate::context::{Context, FileId, OpenContext};
use crate::id::IdString;
use crate::io::OutputRedirect;
use crate::opt::*;
use anyhow::{bail, Result};
use humansize::{file_size_opts, FileSize};
use log::{debug, info};
use std::convert::TryFrom;
use std::fs::{self, File};
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::Instant;
use tempfile::NamedTempFile;
use unicase::UniCase;
use unplug::common::io::{copy_buffered, BUFFER_SIZE};
use unplug::common::ReadSeek;
use unplug::data::atc::ATCS;
use unplug::data::item::{ItemFlags, ITEMS};
use unplug::data::object::Object;
use unplug::data::stage::{StageDefinition, STAGES};
use unplug::dvd::{
    ArchiveBuilder, ArchiveReader, DiscStream, Entry, EntryId, FileTree, Glob, GlobMode, OpenFile,
};
use unplug::event::{Block, Script};
use unplug::globals::Libs;
use unplug::stage::Stage;

const UNKNOWN_ID_PREFIX: &str = "unk";

fn list_files(tree: &FileTree, opt: &ListOpt, glob: &Glob) -> Result<()> {
    let get_file = |(p, e)| tree[e].file().map(|f| (p, f));
    let mut files = glob.find(tree).filter_map(get_file).collect::<Vec<_>>();
    if files.is_empty() {
        bail!("No files found");
    }
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

/// The `list` CLI command.
pub fn command_list(_ctx: Context, opt: ListCommand) -> Result<()> {
    match opt {
        ListCommand::Items(opt) => command_list_items(opt),
        ListCommand::Equipment(opt) => command_list_equipment(opt),
        ListCommand::Stages(opt) => command_list_stages(opt),
    }
}

/// The `list items` CLI command.
fn command_list_items(opt: ListItemsOpt) -> Result<()> {
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

/// The `list equipment` CLI command.
fn command_list_equipment(opt: ListEquipmentOpt) -> Result<()> {
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

/// The `list stages` CLI command.
fn command_list_stages(opt: ListStagesOpt) -> Result<()> {
    let mut stages: Vec<_> = STAGES.iter().map(|s| s.id).collect();
    sort_ids(&mut stages, &opt.settings);
    for stage in stages {
        let name = stage.to_id();
        println!("[{:>3}] {}", i32::from(stage), name);
    }
    Ok(())
}

fn dump_script(script: &Script, flags: &DumpStageFlags, mut out: impl Write) -> Result<()> {
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

    dump_script(&stage.script, flags, out)?;
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

fn do_dump_libs(libs: &Libs, flags: &DumpStageFlags, mut out: impl Write) -> Result<()> {
    for (i, id) in libs.entry_points.iter().enumerate() {
        writeln!(out, "lib[{}]: {:?}", i, id)?;
    }
    dump_script(&libs.script, flags, out)?;
    Ok(())
}

pub fn dump_libs(ctx: Context, opt: DumpLibsOpt) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    let out = BufWriter::new(OutputRedirect::new(opt.output)?);
    info!("Dumping script globals");
    let libs = ctx.read_globals()?.read_libs()?;
    do_dump_libs(&libs, &opt.flags, out)
}

pub fn dump_all_stages(ctx: Context, opt: DumpAllStagesOpt) -> Result<()> {
    let start_time = Instant::now();
    let mut ctx = ctx.open_read()?;

    info!("Dumping script globals");
    fs::create_dir_all(&opt.output)?;
    let libs = ctx.read_globals()?.read_libs()?;
    let libs_out = File::create(Path::join(&opt.output, "globals.txt"))?;
    do_dump_libs(&libs, &opt.flags, BufWriter::new(libs_out))?;

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

/// The `iso` CLI command.
pub fn command_iso(ctx: Context, opt: IsoCommand) -> Result<()> {
    match opt {
        IsoCommand::Info => command_iso_info(ctx),
        IsoCommand::List(opt) => command_iso_list(ctx, opt),
        IsoCommand::Extract(opt) => command_iso_extract(ctx, opt),
        IsoCommand::ExtractAll(opt) => command_iso_extract_all(ctx, opt),
        IsoCommand::Replace(opt) => command_iso_replace(ctx, opt),
    }
}

/// The `iso info` CLI command.
fn command_iso_info(ctx: Context) -> Result<()> {
    let path = ctx.into_iso_path()?;
    let mut disc = DiscStream::open(File::open(&path)?)?;
    let banner = disc.read_banner()?;
    let name = path.file_name().unwrap().to_string_lossy();
    println!("{}: [{}] {}", name, disc.game_id(), disc.game_name());

    let info = &banner.languages[0];
    println!("Name: {}", info.name_long.decode().unwrap());
    println!("Maker: {}", info.maker_long.decode().unwrap());

    let description = info.description.decode().unwrap();
    let mut lines = description.split('\n');
    println!("Description: {}", lines.next().unwrap_or(""));
    for line in lines {
        println!("             {}", line);
    }

    let used = disc.used_size() as u64;
    let total = disc.total_size() as u64;
    println!(
        "Disc Usage: {} / {} ({}%)",
        used.file_size(file_size_opts::CONVENTIONAL).unwrap(),
        total.file_size(file_size_opts::CONVENTIONAL).unwrap(),
        used * 100 / total
    );

    println!("File Entries: {}", disc.files.len());
    // TODO: Other useful info?
    Ok(())
}

/// The `iso list` CLI command.
fn command_iso_list(ctx: Context, opt: IsoListOpt) -> Result<()> {
    let path = ctx.into_iso_path()?;
    let disc = DiscStream::open(File::open(&path)?)?;
    list_files(&disc.files, &opt.settings, &Glob::new(GlobMode::Prefix, opt.paths))
}

/// The `iso extract` CLI command.
fn command_iso_extract(ctx: Context, opt: IsoExtractOpt) -> Result<()> {
    let path = ctx.into_iso_path()?;
    let mut disc = DiscStream::open(File::open(&path)?)?;
    let files = Glob::new(GlobMode::Exact, opt.paths).find(&disc.files).collect::<Vec<_>>();
    if files.is_empty() {
        bail!("Nothing to extract");
    }
    let (out_dir, out_name) = output_dir_and_name(opt.output.as_deref(), files.len() > 1);
    fs::create_dir_all(&out_dir)?;
    let mut io_buf = vec![0u8; BUFFER_SIZE].into_boxed_slice();
    for (path, entry) in files {
        extract_file(&mut disc, entry, &path, out_dir, out_name.as_deref(), &mut io_buf)?;
    }
    Ok(())
}

/// The `iso extract-all` CLI command.
fn command_iso_extract_all(ctx: Context, opt: IsoExtractAllOpt) -> Result<()> {
    let path = ctx.into_iso_path()?;
    let mut disc = DiscStream::open(File::open(&path)?)?;
    let (out_dir, out_name) = output_dir_and_name(opt.output.as_deref(), false);
    fs::create_dir_all(&out_dir)?;
    let mut io_buf = vec![0u8; BUFFER_SIZE].into_boxed_slice();
    let root = disc.files.root();
    extract_file(&mut disc, root, "/", out_dir, out_name.as_deref(), &mut io_buf)?;
    Ok(())
}

fn extract_file(
    source: &mut dyn OpenFile,
    entry: EntryId,
    entry_path: &str,
    out_dir: &Path,
    out_name: Option<&str>,
    io_buf: &mut [u8],
) -> Result<()> {
    let file = source.query_file(entry);
    let name = out_name.unwrap_or_else(|| file.name());
    let out_path = if name.is_empty() { out_dir.to_owned() } else { out_dir.join(name) };
    match file {
        Entry::File(_) => {
            info!("Extracting {}", entry_path);
            let mut writer = File::create(&out_path)?;
            let mut reader = source.open_file(entry)?;
            copy_buffered(&mut reader, &mut writer, io_buf)?;
        }
        Entry::Directory(dir) => {
            fs::create_dir_all(&out_path)?;
            for child in dir.children.clone() {
                let child_file = source.query_file(child);
                let child_path =
                    format!("{}/{}", entry_path.trim_end_matches('/'), child_file.name());
                extract_file(source, child, &child_path, &out_path, None, io_buf)?;
            }
        }
    }
    Ok(())
}

/// The `iso replace` CLI command.
fn command_iso_replace(ctx: Context, opt: IsoReplaceOpt) -> Result<()> {
    let mut ctx = ctx.open_read_write()?;
    let file = ctx.disc_file_at(&opt.dest_path)?;
    let info = ctx.query_file(&file)?;
    let reader = File::open(&opt.src_path)?;
    info!("Writing {}", info.name);
    ctx.begin_update().write_file(&file, reader).commit()?;
    Ok(())
}

/// The `archive` CLI command.
pub fn command_archive(ctx: Context, opt: ArchiveCommand) -> Result<()> {
    match opt {
        ArchiveCommand::Info(opt) => command_archive_info(ctx, opt),
        ArchiveCommand::List(opt) => command_archive_list(ctx, opt),
        ArchiveCommand::Extract(opt) => command_archive_extract(ctx, opt),
        ArchiveCommand::ExtractAll(opt) => command_archive_extract_all(ctx, opt),
        ArchiveCommand::Replace(opt) => command_archive_replace(ctx, opt),
    }
}

/// The `archive info` CLI command.
fn command_archive_info(ctx: Context, opt: ArchiveInfoOpt) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    let file = ctx.file_at(&opt.archive)?;
    let info = ctx.query_file(&file)?;
    let archive = ArchiveReader::open(ctx.open_file(&file)?)?;
    println!("{}: U8 archive", &info.name);
    println!("Size: {}", info.size.file_size(file_size_opts::CONVENTIONAL).unwrap());
    println!("File Entries: {}", archive.files.len());
    Ok(())
}

/// The `archive list` CLI command.
fn command_archive_list(ctx: Context, opt: ArchiveListOpt) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    let archive = ArchiveReader::open(ctx.open_file_at(&opt.archive)?)?;
    list_files(&archive.files, &opt.settings, &Glob::new(GlobMode::Prefix, opt.paths))?;
    Ok(())
}

/// The `archive extract` CLI command.
fn command_archive_extract(ctx: Context, opt: ArchiveExtractOpt) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    let mut archive = ArchiveReader::open(ctx.open_file_at(&opt.archive)?)?;
    let files = Glob::new(GlobMode::Exact, opt.paths).find(&archive.files).collect::<Vec<_>>();
    if files.is_empty() {
        bail!("Nothing to extract");
    }
    let (out_dir, out_name) = output_dir_and_name(opt.output.as_deref(), files.len() > 1);
    fs::create_dir_all(&out_dir)?;
    let mut io_buf = vec![0u8; BUFFER_SIZE].into_boxed_slice();
    for (path, entry) in files {
        extract_file(&mut archive, entry, &path, out_dir, out_name.as_deref(), &mut io_buf)?;
    }
    Ok(())
}

/// The `archive extract-all` CLI command.
fn command_archive_extract_all(ctx: Context, opt: ArchiveExtractAllOpt) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    let mut archive = ArchiveReader::open(ctx.open_file_at(&opt.archive)?)?;
    let (out_dir, out_name) = output_dir_and_name(opt.output.as_deref(), false);
    fs::create_dir_all(&out_dir)?;
    let mut io_buf = vec![0u8; BUFFER_SIZE].into_boxed_slice();
    let root = archive.files.root();
    extract_file(&mut archive, root, "/", out_dir, out_name.as_deref(), &mut io_buf)?;
    Ok(())
}

/// The `archive replace` CLI command.
fn command_archive_replace(ctx: Context, opt: ArchiveReplaceOpt) -> Result<()> {
    let mut ctx = ctx.open_read_write()?;
    let file = ctx.file_at(&opt.archive)?;
    let info = ctx.query_file(&file)?;
    let mut archive = ArchiveReader::open(ctx.open_file(&file)?)?;
    let entry = archive.files.at(&opt.dest_path)?;
    if archive.files[entry].is_dir() {
        bail!("{} is a directory", archive.files[entry].name());
    }

    let reader = File::open(&opt.src_path)?;
    info!("Rebuilding archive data");
    let mut temp = NamedTempFile::new()?;
    debug!("Writing new archive to {}", temp.path().to_string_lossy());
    let mut builder = ArchiveBuilder::with_archive(&mut archive);
    builder.replace(entry, || reader).write_to(&mut temp)?;
    temp.seek(SeekFrom::Start(0))?;
    drop(builder);
    drop(archive);

    info!("Writing new {}", info.name);
    ctx.begin_update().write_file(&file, temp).commit()?;
    Ok(())
}
