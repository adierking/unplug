use anyhow::Result;
use clap::Parser;
use log::error;
use std::path::Path;
use std::process;
use unplug_cli::config::{self, Config};
use unplug_cli::context::Context;
use unplug_cli::opt::{ConfigOpt, ContextOpt, Opt, Subcommand};
use unplug_cli::{audio, commands, dolphin, globals, msg, project, shop, terminal};

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

fn load_config(opt: ConfigOpt) {
    if opt.no_config {
        return;
    }
    let result = match opt.config {
        Some(path) => Config::get().load_from(path),
        None => Config::get().load(),
    };
    if let Err(e) = result {
        error!("Failed to load the config file: {:#}", e);
    }
}

fn get_context(opt: ContextOpt) -> Result<Context> {
    // Command-line args take precedence
    if let Some(path) = opt.iso {
        return Ok(Context::Iso(path));
    }

    // Try loading a project
    let config = Config::get();
    if !opt.no_project {
        if let Some(context) = project::try_get_context(&config, opt.project.as_deref())? {
            return Ok(context);
        }
    }

    // Try loading the default ISO if nothing else is available
    let default_iso = &config.settings.default_iso;
    if !default_iso.is_empty() {
        Ok(Context::DefaultIso(Path::new(default_iso).to_owned()))
    } else {
        Ok(Context::Local)
    }
}

fn run_app() -> Result<()> {
    let opt = Opt::parse();
    terminal::init_logging(opt.verbose);
    load_config(opt.config);

    #[cfg(feature = "trace")]
    let mut _trace_guard = None;
    #[cfg(feature = "trace")]
    if let Some(path) = &opt.trace {
        _trace_guard = Some(init_tracing(path)?);
    }

    let ctx = get_context(opt.context)?;
    match opt.command {
        Subcommand::Config(opt) => config::command(ctx, opt),
        Subcommand::Project(opt) => project::command(ctx, opt),
        Subcommand::Archive(opt) => commands::command_archive(ctx, opt),
        Subcommand::Audio(opt) => audio::command(ctx, opt),
        Subcommand::Iso(opt) => commands::command_iso(ctx, opt),
        Subcommand::List(opt) => commands::command_list(ctx, opt),
        Subcommand::Qp(opt) => commands::command_qp(ctx, opt),
        Subcommand::Script(opt) => commands::command_script(ctx, opt),
        Subcommand::DumpColliders(opt) => commands::dump_colliders(ctx, opt),
        Subcommand::ExportMessages(opt) => msg::export_messages(ctx, opt),
        Subcommand::ImportMessages(opt) => msg::import_messages(ctx, opt),
        Subcommand::ExportGlobals(opt) => globals::export_globals(ctx, opt),
        Subcommand::ImportGlobals(opt) => globals::import_globals(ctx, opt),
        Subcommand::ExportShop(opt) => shop::export_shop(ctx, opt),
        Subcommand::ImportShop(opt) => shop::import_shop(ctx, opt),
        Subcommand::Dolphin(opt) => dolphin::command(ctx, opt),
        #[cfg(feature = "debug")]
        Subcommand::Debug(opt) => unplug_cli::debug::command(ctx, opt),
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
