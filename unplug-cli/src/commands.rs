use crate::context::{Context, FileId, OpenContext};
use crate::id::IdString;
use crate::io::OutputRedirect;
use crate::opt::*;
use anyhow::{bail, Result};
use humansize::{file_size_opts, FileSize};
use log::{debug, info};
use regex::RegexSet;
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

/// Characters which need to be escaped if they appear in a glob.
const SPECIAL_REGEX_CHARS: &str = r".+()|[]{}^$";

/// Converts a glob string into a regex that can match paths.
/// Supports the typical `*`, `**`, and `?` wildcards.
fn glob_to_regex(glob: &str) -> String {
    let mut regex = "(?i)^".to_owned(); // Case-insensitive
    let mut chars = glob.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '*' {
            if let Some('*') = chars.peek().copied() {
                // `**` - match any characters including slashes
                regex.push_str(r".*");
                chars.next();
                // Discard separators after `**`
                while let Some('\\') | Some('/') = chars.peek().copied() {
                    chars.next();
                }
            } else {
                // `*` - match any characters except slashes
                regex.push_str(r"[^/]*");
            }
        } else if ch == '?' {
            // Wildcard, match any single character except slashes
            regex.push_str(r"[^/]");
        } else if ch == '\\' || ch == '/' {
            // Normalize path separators
            regex.push('/');
            while let Some('\\') | Some('/') = chars.peek().copied() {
                chars.next();
            }
        } else if SPECIAL_REGEX_CHARS.contains(ch) {
            // Escape special characters
            regex.push('\\');
            regex.push(ch);
        } else {
            regex.push(ch);
        }
    }
    // End on separator boundary
    regex.push_str(r"(/|$)");
    regex
}

/// Compiles a set of glob expressions into a single `RegexSet`.
fn compile_globs<I, S>(globs: I) -> RegexSet
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    RegexSet::new(globs.into_iter().map(|g| glob_to_regex(g.as_ref()))).unwrap()
}

fn list_files(tree: &FileTree, opt: &ListOpt, filter: Option<RegexSet>) -> Result<()> {
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
        if let Some(filter) = &filter {
            if !filter.is_match(&path) {
                continue;
            }
        }
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
    list_files(&archive.files, &opt.settings, None)
}

pub fn list_iso(_ctx: Context, opt: ListIsoOpt) -> Result<()> {
    let file = File::open(opt.path)?;
    let iso = DiscStream::open(file)?;
    println!("Game ID: {}", iso.game_id());
    list_files(&iso.files, &opt.settings, None)
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
        IsoCommand::Extract(_) => todo!(),
        IsoCommand::ExtractAll(_) => todo!(),
        IsoCommand::Replace(_) => todo!(),
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
    let filter = if !opt.paths.is_empty() { Some(compile_globs(&opt.paths)) } else { None };
    list_files(&disc.files, &opt.settings, filter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_globbing() {
        let paths = &[
            "qp.bin",
            "qp/sfx_army.ssm",
            "qp/sfx_bb.ssm",
            "qp/streaming/bgm.hps",
            "qp/streaming/menu.hps",
        ];

        let check = |glob: &str, expected: &[&str]| {
            let filter = compile_globs(&[glob]);
            let actual = paths.iter().copied().filter(|p| filter.is_match(p)).collect::<Vec<_>>();
            assert_eq!(&actual, expected, "glob: {:?}", glob);
        };

        check("", &[]);
        check("q", &[]);
        check(
            "qp",
            &["qp/sfx_army.ssm", "qp/sfx_bb.ssm", "qp/streaming/bgm.hps", "qp/streaming/menu.hps"],
        );

        check("qp?", &[]);
        check("qp????", &["qp.bin"]);
        check("qp.bin", &["qp.bin"]);
        check("QP.bin", &["qp.bin"]);

        check("qp/sfx_army.ssm", &["qp/sfx_army.ssm"]);
        check("qp\\sfx_army.ssm", &["qp/sfx_army.ssm"]);
        check("qp/\\/sfx_army.ssm", &["qp/sfx_army.ssm"]);

        check("*in", &["qp.bin"]);
        check("*.in", &[]);
        check("*.bin", &["qp.bin"]);

        check("*.hps", &[]);
        check("**.hps", &["qp/streaming/bgm.hps", "qp/streaming/menu.hps"]);
        check("**/*.hps", &["qp/streaming/bgm.hps", "qp/streaming/menu.hps"]);
        check("**/\\/*.hps", &["qp/streaming/bgm.hps", "qp/streaming/menu.hps"]);
        check("*/*/*", &["qp/streaming/bgm.hps", "qp/streaming/menu.hps"]);
        check("qp/streaming/*.hps", &["qp/streaming/bgm.hps", "qp/streaming/menu.hps"]);

        check("**/*.bin", &["qp.bin"]);
        check("**/**/*.bin", &["qp.bin"]);

        check("*", paths);
        check("**", paths);
        check("**/*", paths);
        check("**/**", paths);
    }
}
