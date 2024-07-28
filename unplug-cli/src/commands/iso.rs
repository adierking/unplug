use crate::common::output_dir_and_name;
use crate::context::Context;
use crate::fst::{extract_file, list_files};
use crate::opt::{
    IsoCommand, IsoExtractAllOpt, IsoExtractOpt, IsoListOpt, IsoReplaceOpt, IsoSetCommand,
};
use anyhow::{bail, Result};
use humansize::{FormatSize, BINARY};
use log::info;
use std::fs::{self, File};
use unplug::common::io::BUFFER_SIZE;
use unplug::common::Text;
use unplug::dvd::{Banner, DiscStream, Glob, GlobMode};

const BANNER_PATH: &str = "opening.bnr";

/// The `iso info` CLI command.
fn command_info(ctx: Context) -> Result<()> {
    let path = ctx.into_iso_path()?;
    let mut disc = DiscStream::open(File::open(&path)?)?;
    let banner = disc.read_banner()?;
    let name = path.file_name().unwrap().to_string_lossy();
    println!("{}: [{}] {}", name, disc.game_id(), disc.game_name());

    let info = &banner.languages[0];
    println!("Name: {}", info.name_long.decode_replacing());
    println!("Maker: {}", info.maker_long.decode_replacing());

    let description = info.description.decode_replacing();
    let mut lines = description.split('\n');
    println!("Description: {}", lines.next().unwrap_or(""));
    for line in lines {
        println!("             {}", line);
    }

    let used = disc.used_size() as u64;
    let total = disc.total_size() as u64;
    println!(
        "Disc Usage: {} / {} ({}%)",
        used.format_size(BINARY),
        total.format_size(BINARY),
        used * 100 / total
    );

    println!("File Entries: {}", disc.files().len());
    // TODO: Other useful info?
    Ok(())
}

/// The `iso list` CLI command.
fn command_list(ctx: Context, opt: IsoListOpt) -> Result<()> {
    let path = ctx.into_iso_path()?;
    let disc = DiscStream::open(File::open(path)?)?;
    list_files(disc.files(), &opt.settings, &Glob::new(GlobMode::Prefix, opt.paths))
}

/// The `iso extract` CLI command.
fn command_extract(ctx: Context, opt: IsoExtractOpt) -> Result<()> {
    let path = ctx.into_iso_path()?;
    let mut disc = DiscStream::open(File::open(path)?)?;
    let files = Glob::new(GlobMode::Exact, opt.paths).find(disc.files()).collect::<Vec<_>>();
    if files.is_empty() {
        bail!("Nothing to extract");
    }
    let (out_dir, out_name) = output_dir_and_name(opt.output.as_deref(), files.len() > 1);
    fs::create_dir_all(out_dir)?;
    let mut io_buf = vec![0u8; BUFFER_SIZE].into_boxed_slice();
    for (path, entry) in files {
        extract_file(&mut disc, entry, &path, out_dir, out_name.as_deref(), &mut io_buf)?;
    }
    Ok(())
}

/// The `iso extract-all` CLI command.
fn command_extract_all(ctx: Context, opt: IsoExtractAllOpt) -> Result<()> {
    let path = ctx.into_iso_path()?;
    let mut disc = DiscStream::open(File::open(path)?)?;
    let (out_dir, out_name) = output_dir_and_name(opt.output.as_deref(), false);
    fs::create_dir_all(out_dir)?;
    let mut io_buf = vec![0u8; BUFFER_SIZE].into_boxed_slice();
    let root = disc.files().root();
    extract_file(&mut disc, root, "/", out_dir, out_name.as_deref(), &mut io_buf)?;
    Ok(())
}

/// The `iso replace` CLI command.
fn command_replace(ctx: Context, opt: IsoReplaceOpt) -> Result<()> {
    let mut ctx = ctx.open_read_write()?;
    let file = ctx.disc_file_at(&opt.dest_path)?;
    let info = ctx.query_file(&file)?;
    let reader = File::open(&opt.src_path)?;
    info!("Writing {}", info.name);
    ctx.begin_update().write_file(&file, reader).commit()?;
    Ok(())
}

/// Read, edit, and save opening.bnr.
fn edit_banner<F>(ctx: Context, f: F) -> Result<()>
where
    F: FnOnce(&mut Banner) -> Result<()>,
{
    let mut ctx = ctx.open_read_write()?;
    let file = ctx.disc_file_at(BANNER_PATH)?;
    let mut banner = ctx.deserialize_file(&file)?;
    f(&mut banner)?;
    info!("Writing {}", BANNER_PATH);
    ctx.begin_update().serialize_file(&file, &banner)?.commit()?;
    Ok(())
}

/// The `iso set maker` CLI command.
fn command_set_maker(ctx: Context, name: String) -> Result<()> {
    edit_banner(ctx, |banner| {
        let lang = &mut banner.languages[0];
        let short_len = name.len().min(lang.maker_short.max_len());
        lang.maker_short = Text::encode(&name[..short_len])?;
        lang.maker_long = Text::encode(&name)?;
        Ok(())
    })
}

/// The `iso set name` CLI command.
fn command_set_name(ctx: Context, name: String) -> Result<()> {
    edit_banner(ctx, |banner| {
        let lang = &mut banner.languages[0];
        let short_len = name.len().min(lang.name_short.max_len());
        lang.name_short = Text::encode(&name[..short_len])?;
        lang.name_long = Text::encode(&name)?;
        Ok(())
    })
}

/// The `iso` CLI command.
pub fn command(ctx: Context, opt: IsoCommand) -> Result<()> {
    match opt {
        IsoCommand::Info => command_info(ctx),
        IsoCommand::List(opt) => command_list(ctx, opt),
        IsoCommand::Extract(opt) => command_extract(ctx, opt),
        IsoCommand::ExtractAll(opt) => command_extract_all(ctx, opt),
        IsoCommand::Replace(opt) => command_replace(ctx, opt),
        IsoCommand::Set(IsoSetCommand::Maker { name }) => command_set_maker(ctx, name),
        IsoCommand::Set(IsoSetCommand::Name { name }) => command_set_name(ctx, name),
    }
}
