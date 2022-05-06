use crate::config::Config;
use crate::context::Context;
use crate::opt::DolphinOpt;
use anyhow::{bail, Result};
use cfg_if::cfg_if;
use log::{debug, info};
use std::fs;
use std::process::{Command, Stdio};

/// Dolphin option to disable the UI
const DOLPHIN_OPT_NO_UI: &str = "-b";
/// Dolphin option to select a game to execute
const DOLPHIN_OPT_EXEC: &str = "-e";
/// Dolphin option to set a config variable
const DOLPHIN_OPT_CONFIG: &str = "-C";

/// This makes Dolphin not show a confirmation dialog when you close the window. When we aren't
/// showing the full UI this dialog doesn't seem necessary.
const DOLPHIN_CONFIG_NO_CONFIRM_STOP: &str = "Dolphin.Interface.ConfirmStop=False";

cfg_if! {
    if #[cfg(target_os = "macos")] {
        /// Path to Info.plist within an app bundle.
        const INFO_PLIST_PATH: &str = "Contents/Info.plist";
        /// Path to the `open` executable to run app bundles with
        const OPEN_PATH: &str = "/usr/bin/open";
        /// open option to select an app bundle
        const OPEN_OPT_APP: &str = "-a";
        /// open option to always start a new instance
        const OPEN_OPT_NEW_INSTANCE: &str = "-n";
        /// open option to wait for completion
        const OPEN_OPT_WAIT: &str = "-W";
        /// open option to set app arguments
        const OPEN_OPT_ARGS: &str = "--args";
    }
}

fn dolphin_command() -> Result<Command> {
    let config = Config::get();
    let path = &config.settings.dolphin_path;
    if path.is_empty() {
        bail!("No Dolphin path is configured. Use `config set dolphin-path <PATH>`.");
    }

    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(e) => {
            bail!("Invalid Dolphin path: {}: {:#}", path, e);
        }
    };
    if metadata.is_file() {
        return Ok(Command::new(path));
    }

    #[cfg(target_os = "macos")]
    if metadata.is_dir() {
        let plist = std::path::Path::new(path).join(INFO_PLIST_PATH);
        if plist.is_file() {
            debug!("Detected Dolphin app bundle (plist at {})", plist.display());
            let mut command = Command::new(OPEN_PATH);
            command.arg(OPEN_OPT_APP).arg(path);
            command.arg(OPEN_OPT_NEW_INSTANCE);
            command.arg(OPEN_OPT_WAIT);
            command.arg(OPEN_OPT_ARGS);
            return Ok(command);
        }
    }

    bail!("Invalid Dolphin path: {}", path);
}

/// The `dolphin` CLI command.
pub fn command(ctx: Context, opt: DolphinOpt) -> Result<()> {
    let mut command = dolphin_command()?;
    command.arg(DOLPHIN_OPT_EXEC).arg(ctx.into_iso_path()?);
    if !opt.ui {
        command.arg(DOLPHIN_OPT_NO_UI);
        command.arg(DOLPHIN_OPT_CONFIG).arg(DOLPHIN_CONFIG_NO_CONFIRM_STOP);
    }
    if opt.no_capture {
        command.stdin(Stdio::inherit());
        command.stdout(Stdio::inherit());
        command.stderr(Stdio::inherit());
    } else {
        command.stdin(Stdio::null());
        command.stdout(Stdio::null());
        command.stderr(Stdio::null());
    }

    info!("Starting Dolphin");
    debug!("Command: {:?}", command);
    let mut child = command.spawn()?;
    if !opt.no_wait {
        let status = child.wait()?;
        if !status.success() {
            bail!("Dolphin exited abnormally");
        }
    }
    Ok(())
}
