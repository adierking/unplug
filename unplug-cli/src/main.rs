use anyhow::Result;
use clap::Parser;
use log::error;
use std::path::Path;
use std::process;
use unplug_cli::args::{CliArgs, GlobalConfigArgs, GlobalContextArgs};
use unplug_cli::config::{self, Config};
use unplug_cli::context::Context;
use unplug_cli::{commands, terminal};

#[cfg(feature = "trace")]
fn init_tracing(path: &Path) -> Result<impl Drop> {
    use log::debug;
    use std::fs::File;
    use std::io::BufWriter;
    use tracing_flame::FlameLayer;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::registry::Registry;

    let file = BufWriter::new(File::create(path)?);
    let (writer, guard) = tracing_appender::non_blocking(file);
    let flame = FlameLayer::new(writer);
    let subscriber = Registry::default().with(flame);
    tracing::subscriber::set_global_default(subscriber).expect("failed to set global subscriber");
    debug!("inferno-flamegraph trace data will be written to {}", path.display());
    Ok(guard)
}

fn load_config(args: GlobalConfigArgs) {
    if args.no_config {
        return;
    }
    let result = match args.config {
        Some(path) => Config::get().load_from(path),
        None => Config::get().load(),
    };
    if let Err(e) = result {
        error!("Failed to load the config file: {:#}", e);
    }
}

fn get_context(args: GlobalContextArgs) -> Result<Context> {
    // Command-line args take precedence
    if let Some(path) = args.iso {
        return Ok(Context::Iso(path));
    }

    // Try loading a project
    let config = Config::get();
    if !args.default_iso {
        if let Some(context) = config::load_project(&config, args.project.as_deref())? {
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
    let args = CliArgs::parse();
    terminal::init_logging(args.verbose);
    load_config(args.config);

    #[cfg(feature = "trace")]
    let mut _trace_guard = None;
    #[cfg(feature = "trace")]
    if let Some(path) = &args.trace {
        _trace_guard = Some(init_tracing(path)?);
    }

    let ctx = get_context(args.context)?;
    commands::execute(ctx, args.command)
}

fn main() {
    process::exit(match run_app() {
        Ok(()) => 0,
        Err(err) => {
            error!("Fatal: {:#}", err);
            1
        }
    });
}
