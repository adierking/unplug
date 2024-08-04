use crate::opt::archive::*;

use crate::common::output_dir_and_name;
use crate::context::Context;
use crate::fst::{extract_file, list_files};
use anyhow::{bail, Result};
use humansize::{FormatSize, BINARY};
use log::{debug, info};
use std::fs::{self, File};
use std::io::Seek;
use tempfile::NamedTempFile;
use unplug::common::io::BUFFER_SIZE;
use unplug::dvd::{ArchiveBuilder, ArchiveReader, Glob, GlobMode};

/// The path that `qp` passes to the `archive` commands.
const QP_ALIAS_PATH: &str = "dvd:qp.bin";

/// The `archive info` CLI command.
fn command_info(ctx: Context, path: &str) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    let file = ctx.file_at(path)?;
    let info = ctx.query_file(&file)?;
    let archive = ArchiveReader::open(ctx.open_file(&file)?)?;
    println!("{}: U8 archive", &info.name);
    println!("Size: {}", info.size.format_size(BINARY));
    println!("File Entries: {}", archive.files.len());
    Ok(())
}

/// The `archive list` CLI command.
fn command_list(ctx: Context, path: &str, opt: ListArgs) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    let archive = ArchiveReader::open(ctx.open_file_at(path)?)?;
    list_files(&archive.files, &opt.settings, &Glob::new(GlobMode::Prefix, opt.paths))?;
    Ok(())
}

/// The `archive extract` CLI command.
fn command_extract(ctx: Context, path: &str, opt: ExtractArgs) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    let mut archive = ArchiveReader::open(ctx.open_file_at(path)?)?;
    let files = Glob::new(GlobMode::Exact, opt.paths).find(&archive.files).collect::<Vec<_>>();
    if files.is_empty() {
        bail!("Nothing to extract");
    }
    let (out_dir, out_name) = output_dir_and_name(opt.output.as_deref(), files.len() > 1);
    fs::create_dir_all(out_dir)?;
    let mut io_buf = vec![0u8; BUFFER_SIZE].into_boxed_slice();
    for (path, entry) in files {
        extract_file(&mut archive, entry, &path, out_dir, out_name.as_deref(), &mut io_buf)?;
    }
    Ok(())
}

/// The `archive extract-all` CLI command.
fn command_extract_all(ctx: Context, path: &str, opt: ExtractAllArgs) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    let mut archive = ArchiveReader::open(ctx.open_file_at(path)?)?;
    let (out_dir, out_name) = output_dir_and_name(opt.output.as_deref(), false);
    fs::create_dir_all(out_dir)?;
    let mut io_buf = vec![0u8; BUFFER_SIZE].into_boxed_slice();
    let root = archive.files.root();
    extract_file(&mut archive, root, "/", out_dir, out_name.as_deref(), &mut io_buf)?;
    Ok(())
}

/// The `archive replace` CLI command.
fn command_replace(ctx: Context, path: &str, opt: ReplaceArgs) -> Result<()> {
    let mut ctx = ctx.open_read_write()?;
    let file = ctx.file_at(path)?;
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
    temp.rewind()?;
    drop(builder);
    drop(archive);

    info!("Writing new {}", info.name);
    ctx.begin_update().write_file(&file, temp).commit()?;
    Ok(())
}

/// The `archive` CLI command.
pub fn command(ctx: Context, opt: Subcommand) -> Result<()> {
    match opt {
        Subcommand::Info { path } => command_info(ctx, &path),
        Subcommand::List { path, opt } => command_list(ctx, &path, opt),
        Subcommand::Extract { path, opt } => command_extract(ctx, &path, opt),
        Subcommand::ExtractAll { path, opt } => command_extract_all(ctx, &path, opt),
        Subcommand::Replace { path, opt } => command_replace(ctx, &path, opt),
    }
}

/// The `qp` CLI command.
pub fn command_qp(ctx: Context, opt: QpSubcommand) -> Result<()> {
    match opt {
        QpSubcommand::Info => command_info(ctx, QP_ALIAS_PATH),
        QpSubcommand::List(opt) => command_list(ctx, QP_ALIAS_PATH, opt),
        QpSubcommand::Extract(opt) => command_extract(ctx, QP_ALIAS_PATH, opt),
        QpSubcommand::ExtractAll(opt) => command_extract_all(ctx, QP_ALIAS_PATH, opt),
        QpSubcommand::Replace(opt) => command_replace(ctx, QP_ALIAS_PATH, opt),
    }
}
