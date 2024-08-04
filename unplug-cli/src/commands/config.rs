use crate::args::config::*;

use crate::config::{Config, Settings};
use crate::context::Context;
use anyhow::{bail, Result};
use log::info;
use std::fmt::Display;
use std::mem;

/// The `config clear` CLI command.
fn command_clear() -> Result<()> {
    let mut config = Config::get();
    let path = mem::take(&mut config.path);
    *config = Config::with_path(path);
    config.save()?;
    info!("Configuration cleared");
    Ok(())
}

/// The `config path` CLI command.
fn command_path() -> Result<()> {
    let config = Config::get();
    if config.path.as_os_str().is_empty() {
        bail!("No config file is loaded");
    }
    if let Ok(path) = config.path.canonicalize() {
        if cfg!(target_os = "windows") {
            // canonicalize() includes the extended-length prefix on Windows, so strip that out
            let path_str = path.to_string_lossy();
            println!("{}", path_str.strip_prefix("\\\\?\\").unwrap_or(&*path_str));
        } else {
            println!("{}", path.display());
        }
    } else {
        println!("{}", config.path.display());
    }
    Ok(())
}

/// The `config get` CLI command.
fn command_get(setting: GetSetting) -> Result<()> {
    let settings = &Config::get().settings;
    match setting {
        GetSetting::DefaultIso => println!("{}", settings.default_iso),
        GetSetting::DolphinPath => println!("{}", settings.dolphin_path),
    }
    Ok(())
}

/// The `config set` CLI command.
fn command_set(setting: SetSetting) -> Result<()> {
    match setting {
        SetSetting::DefaultIso { path } => set("Default ISO", path, |s| &mut s.default_iso),
        SetSetting::DolphinPath { path } => set("Dolphin path", path, |s| &mut s.dolphin_path),
    }
}

/// Generic implementation of `config set`.
fn set<T, F>(name: &str, value: Option<T>, get_mut: F) -> Result<()>
where
    T: Clone + Default + Display,
    F: FnOnce(&mut Settings) -> &mut T,
{
    let mut config = Config::get();
    let cleared = value.is_none();
    let value = value.unwrap_or_default();
    *get_mut(&mut config.settings) = value.clone();
    config.save()?;
    if cleared {
        info!("{} cleared", name);
    } else {
        info!("{} set to {}", name, value);
    }
    Ok(())
}

/// The `config` CLI command.
pub fn command(_ctx: Context, command: Subcommand) -> Result<()> {
    if !Config::get().is_loaded() {
        bail!("The `config` command requires a config file to be loaded");
    }
    match command {
        Subcommand::Clear => command_clear(),
        Subcommand::Path => command_path(),
        Subcommand::Get(setting) => command_get(setting),
        Subcommand::Set(setting) => command_set(setting),
    }
}
