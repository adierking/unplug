use crate::context::Context;
use crate::opt::ConfigCommand;
use anyhow::{bail, Result};
use dirs::config_dir;
use lazy_static::lazy_static;
use log::{debug, info, trace};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::mem;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// The default subdirectory to place the config file in.
const CONFIG_DIR: &str = "unplug";
/// The default name of the config file.
const CONFIG_NAME: &str = "config.toml";

lazy_static! {
    /// The global config singleton.
    static ref CONFIG: Mutex<Config> = Mutex::new(Config::new());
}

/// Gets the default path for the user's config file.
///
/// See the `dirs` crate documentation for exact details, but in general this will be in:
/// - Windows: `AppData/Roaming`
/// - macOS: `Library/Application Support`
/// - Linux: `$XDG_CONFIG_DIR` or `~/.config`
fn user_config_path() -> PathBuf {
    let mut dir = config_dir().unwrap_or_else(|| Path::new(".").to_owned());
    dir.push(CONFIG_DIR);
    dir.push(CONFIG_NAME);
    dir
}

/// The global Unplug configuration.
#[non_exhaustive]
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Config {
    /// The path to save the configuration back to. This is set on load and not stored in the file.
    #[serde(skip)]
    pub path: PathBuf,

    /// Settings which affect program behavior.
    pub settings: Settings,
}

impl Config {
    fn new() -> Self {
        Self::default()
    }

    fn with_path(path: PathBuf) -> Self {
        let mut config = Self::new();
        config.path = path;
        config
    }

    /// Locks the mutex for the global configuration and return it.
    pub fn get() -> MutexGuard<'static, Config> {
        CONFIG.lock().unwrap()
    }

    /// Loads the configuration from the default path.
    pub fn load(&mut self) -> Result<()> {
        self.load_from(user_config_path())
    }

    /// Loads the configuration from `path`.
    pub fn load_from(&mut self, path: PathBuf) -> Result<()> {
        if path.exists() {
            let contents = fs::read_to_string(&path)?;
            *self = toml::from_str(&contents)?;
            self.path = path;
            debug!("Configuration loaded from {}", self.path.display());
            trace!("{:?}", self);
        } else {
            debug!("Config file does not exist, creating a new one");
            *self = Self::with_path(path);
            self.save()?;
        }
        Ok(())
    }

    /// Saves the configuration back to the path it was loaded from.
    pub fn save(&self) -> Result<()> {
        let contents = toml::to_string_pretty(self)?;
        if self.path.as_os_str().is_empty() {
            bail!("No config file is loaded");
        }
        let dir = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(dir)?;
        fs::write(&self.path, contents)?;
        debug!("Configuration saved to {}", self.path.display());
        Ok(())
    }
}

/// Settings which affect program behavior.
#[non_exhaustive]
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Settings {
    /// A path to an ISO to load if none is specified. As a safety measure, Unplug will never let
    /// you edit this ISO.
    pub default_iso: String,
}

/// The `config` CLI command.
pub fn command(_ctx: Context, opt: ConfigCommand) -> Result<()> {
    match opt {
        ConfigCommand::Clear => command_clear(),
        ConfigCommand::Path => command_path(),
        ConfigCommand::DefaultIso { value } => command_default_iso(value),
    }
}

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

/// The `config default-iso` CLI command.
fn command_default_iso(value: Option<String>) -> Result<()> {
    if let Some(path) = value {
        if let Err(e) = File::open(&path) {
            bail!("Invalid ISO path: {:#}", e);
        }
        let mut config = Config::get();
        config.settings.default_iso = path;
        config.save()?;
        info!("Default ISO set to {}", config.settings.default_iso);
    } else {
        println!("{}", Config::get().settings.default_iso);
    }
    Ok(())
}
