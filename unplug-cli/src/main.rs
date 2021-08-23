use anyhow::Result;
use log::error;
use simplelog::{Color, ConfigBuilder, Level, LevelFilter, TermLogger, TerminalMode};
use std::process;
use structopt::StructOpt;
use unplug_cli::opt::{Opt, Subcommand};
use unplug_cli::{commands, globals, msg, shop};

fn init_logging(opt: &Opt) {
    let filter = if opt.verbose >= 2 {
        // Note: trace logs are compiled out in release builds
        LevelFilter::Trace
    } else if opt.verbose == 1 {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };
    let config = ConfigBuilder::new()
        .set_thread_level(LevelFilter::Off)
        .set_target_level(LevelFilter::Trace)
        .set_time_format_str("%T%.3f")
        .set_level_color(Level::Info, Color::Green)
        .build();
    TermLogger::init(filter, config, TerminalMode::Stderr).unwrap();
}

fn run_app() -> Result<()> {
    let opt = Opt::from_args();
    init_logging(&opt);
    match opt.command {
        Subcommand::ListArchive(opt) => commands::list_archive(opt),
        Subcommand::ListIso(opt) => commands::list_iso(opt),
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
