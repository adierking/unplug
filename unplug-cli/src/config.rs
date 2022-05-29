use crate::common::IString;
use crate::context::Context;
use anyhow::{anyhow, bail, Result};
use dirs::config_dir;
use lazy_static::lazy_static;
use log::{debug, error, trace};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};
use std::fs;
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

    /// Project definitions.
    pub projects: BTreeMap<IString, Project>,
}

impl Config {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_path(path: PathBuf) -> Self {
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

    /// Finds a project by name (case-insensitive).
    pub fn find_project(&self, name: &str) -> Result<(&str, &Project)> {
        self.projects
            .get_key_value(&name.into())
            .map(|(name, project)| (name.as_str(), project))
            .ok_or_else(|| anyhow!("Unknown project \"{}\"", name))
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

    /// The path to the Dolphin executable (or macOS app bundle) to run projects with.
    pub dolphin_path: String,

    /// The currently-open project.
    pub project: String,
}

/// A type of project.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectKind {
    /// The project is a .iso file.
    Iso,
}

impl Default for ProjectKind {
    fn default() -> Self {
        Self::Iso
    }
}

impl Display for ProjectKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Iso => f.write_str("ISO"),
        }
    }
}

/// A named link to a data source to run commands within.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Project {
    /// The project kind.
    pub kind: ProjectKind,
    /// The path to the project file(s).
    pub path: String,
}

/// Attempts to load the `Context` for a project, returning `Ok(None)` if no project is open.
pub fn load_project(config: &Config, name: Option<&str>) -> Result<Option<Context>> {
    let project_name = name.unwrap_or(&config.settings.project);
    if project_name.is_empty() {
        return Ok(None);
    }
    match config.find_project(project_name) {
        Ok((name, project)) => Ok(Some(Context::ProjectIso {
            name: name.to_owned(),
            path: Path::new(&project.path).to_owned(),
        })),
        Err(e) if name.is_some() => Err(e),
        _ => {
            error!("Project \"{}\" is open but has a missing config entry!", project_name);
            error!("To fix this, add it back or use `project close`.");
            Ok(Some(Context::Local))
        }
    }
}
