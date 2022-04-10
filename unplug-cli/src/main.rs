use anyhow::Result;
use log::error;
use std::process;
use structopt::StructOpt;
use unplug_cli::opt::{Opt, Subcommand};
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

fn run_app() -> Result<()> {
    let opt = Opt::from_args();
    terminal::init_logging(opt.verbose);

    #[cfg(feature = "trace")]
    let mut _trace_guard = None;
    #[cfg(feature = "trace")]
    if let Some(path) = &opt.trace {
        _trace_guard = Some(init_tracing(path)?);
    }

    match opt.command {
        Subcommand::ListArchive(opt) => commands::list_archive(opt),
        Subcommand::ListIso(opt) => commands::list_iso(opt),
        Subcommand::ListItems(opt) => commands::list_items(opt),
        Subcommand::ListEquipment(opt) => commands::list_equipment(opt),
        Subcommand::ListStages(opt) => commands::list_stages(opt),
        Subcommand::ExtractArchive(opt) => commands::extract_archive(opt),
        Subcommand::ExtractIso(opt) => commands::extract_iso(opt),
        Subcommand::DumpStage(opt) => commands::dump_stage(opt),
        Subcommand::DumpLibs(opt) => commands::dump_libs(opt),
        Subcommand::DumpAllStages(opt) => commands::dump_all_stages(opt),
        Subcommand::DumpColliders(opt) => commands::dump_colliders(opt),
        Subcommand::ExportMessages(opt) => msg::export_messages(opt),
        Subcommand::ImportMessages(opt) => msg::import_messages(opt),
        Subcommand::ExportGlobals(opt) => globals::export_globals(opt),
        Subcommand::ImportGlobals(opt) => globals::import_globals(opt),
        Subcommand::ExportShop(opt) => shop::export_shop(opt),
        Subcommand::ImportShop(opt) => shop::import_shop(opt),
        Subcommand::ExportMusic(opt) => audio::export_music(opt),
        Subcommand::ImportMusic(opt) => audio::import_music(opt),
        Subcommand::PlayMusic(opt) => audio::play_music(opt),
        Subcommand::ExportSounds(opt) => audio::export_sounds(opt),
        Subcommand::ImportSound(opt) => audio::import_sound(opt),
        Subcommand::PlaySound(opt) => audio::play_sound(opt),
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
