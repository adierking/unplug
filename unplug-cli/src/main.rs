use anyhow::Result;
use log::error;
use std::process;
use structopt::StructOpt;
use unplug_cli::context::Context;
use unplug_cli::opt::{ContextOpt, Opt, Subcommand};
use unplug_cli::{audio, commands, globals, msg, shop, terminal};

#[cfg(feature = "trace")]
fn init_tracing(path: &std::path::Path) -> Result<impl Drop> {
    use log::debug;
    use std::fs::File;
    use std::io::BufWriter;
    use tracing_flame::FlameLayer;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::registry::Registry;

    let file = BufWriter::new(File::create(path)?);
    let (writer, _guard) = tracing_appender::non_blocking(file);
    let flame = FlameLayer::new(writer);
    let subscriber = Registry::default().with(flame);
    tracing::subscriber::set_global_default(subscriber).expect("failed to set global subscriber");
    debug!("inferno-flamegraph trace data will be written to {}", path.display());
    Ok(_guard)
}

fn get_context(opt: ContextOpt) -> Result<Context> {
    match opt.iso {
        Some(path) => Ok(Context::Iso(path)),
        None => Ok(Context::Local),
    }
}

fn run_app() -> Result<()> {
    let opt = Opt::from_args();
    terminal::init_logging(opt.verbose);

    #[cfg(feature = "trace")]
    let mut _trace_guard = None;
    #[cfg(feature = "trace")]
    if let Some(path) = &opt.trace {
        _trace_guard = Some(init_tracing(path)?);
    }

    let ctx = get_context(opt.context)?;
    match opt.command {
        Subcommand::ListArchive(opt) => commands::list_archive(ctx, opt),
        Subcommand::ListIso(opt) => commands::list_iso(ctx, opt),
        Subcommand::ListItems(opt) => commands::list_items(ctx, opt),
        Subcommand::ListEquipment(opt) => commands::list_equipment(ctx, opt),
        Subcommand::ListStages(opt) => commands::list_stages(ctx, opt),
        Subcommand::ExtractArchive(opt) => commands::extract_archive(ctx, opt),
        Subcommand::ExtractIso(opt) => commands::extract_iso(ctx, opt),
        Subcommand::DumpStage(opt) => commands::dump_stage(ctx, opt),
        Subcommand::DumpLibs(opt) => commands::dump_libs(ctx, opt),
        Subcommand::DumpAllStages(opt) => commands::dump_all_stages(ctx, opt),
        Subcommand::DumpColliders(opt) => commands::dump_colliders(ctx, opt),
        Subcommand::ExportMessages(opt) => msg::export_messages(ctx, opt),
        Subcommand::ImportMessages(opt) => msg::import_messages(ctx, opt),
        Subcommand::ExportGlobals(opt) => globals::export_globals(ctx, opt),
        Subcommand::ImportGlobals(opt) => globals::import_globals(ctx, opt),
        Subcommand::ExportShop(opt) => shop::export_shop(ctx, opt),
        Subcommand::ImportShop(opt) => shop::import_shop(ctx, opt),
        Subcommand::ExportMusic(opt) => audio::export_music(ctx, opt),
        Subcommand::ImportMusic(opt) => audio::import_music(ctx, opt),
        Subcommand::PlayMusic(opt) => audio::play_music(ctx, opt),
        Subcommand::ExportSounds(opt) => audio::export_sounds(ctx, opt),
        Subcommand::ImportSound(opt) => audio::import_sound(ctx, opt),
        Subcommand::PlaySound(opt) => audio::play_sound(ctx, opt),
    }
}

fn main() {
    process::exit(match run_app() {
        Ok(_) => 0,
        Err(err) => {
            error!("Fatal: {:#}", err);
            1
        }
    });
}
