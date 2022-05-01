use crate::config::{Config, Project, ProjectKind};
use crate::context::Context;
use crate::opt::ProjectCommand;
use anyhow::{anyhow, bail, Result};
use log::{error, info};
use std::path::{Path, PathBuf};
use std::{fs, mem};

/// Attempts to load the `Context` for a project, returning `Ok(None)` if no project is open.
pub fn try_get_context(config: &Config, name: Option<&str>) -> Result<Option<Context>> {
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

/// The `project` CLI command.
pub fn command(_ctx: Context, opt: ProjectCommand) -> Result<()> {
    match opt {
        ProjectCommand::Info { name } => command_info(name),
        ProjectCommand::List => command_list(),
        ProjectCommand::Add { path, name } => command_add(path, name),
        ProjectCommand::Forget { name } => command_forget(name),
        ProjectCommand::Open { name } => command_open(name),
        ProjectCommand::Close => command_close(),
    }
}

/// The `project info` CLI command.
fn command_info(name: Option<String>) -> Result<()> {
    let config = Config::get();
    let (name, project) = match &name {
        Some(name) => config.find_project(name)?,
        None => {
            if !config.settings.project.is_empty() {
                config.find_project(&config.settings.project)?
            } else {
                info!("No project is open");
                return Ok(());
            }
        }
    };
    println!("{} ({} at {})", name, project.kind, project.path);
    Ok(())
}

/// The `project list` CLI command.
fn command_list() -> Result<()> {
    for (name, project) in &Config::get().projects {
        println!("{} ({})", name, project.path);
    }
    Ok(())
}

/// The `project add` CLI command.
fn command_add(path: PathBuf, name: Option<String>) -> Result<()> {
    let filename = path
        .file_name()
        .ok_or_else(|| anyhow!("Invalid project path: {}", path.display()))?
        .to_string_lossy()
        .to_lowercase();

    // If a project name was not passed in, use the part of the filename before the extension
    let (name, ext) = match &name {
        Some(name) => (name.as_str(), filename.rsplit('.').next().unwrap_or("")),
        None => filename.rsplit_once('.').unwrap_or((&filename, "")),
    };
    let info = fs::metadata(&path)?;
    // This is lazy, but checking the extension should be fine here
    if !info.is_file() || ext != "iso" {
        bail!("Projects must point to a .iso file");
    }

    let mut config = Config::get();
    let key = name.into();
    if config.projects.contains_key(&key) {
        bail!("Project \"{}\" is already defined", name);
    }
    let project = Project { kind: ProjectKind::Iso, path: path.to_string_lossy().into_owned() };
    config.projects.insert(key, project);
    config.save()?;
    info!("Added project: {}", name);

    Ok(())
}

/// The `project forget` CLI command.
fn command_forget(name: String) -> Result<()> {
    let mut config = Config::get();
    let name = match config.projects.remove_entry(&name.clone().into()) {
        Some((name, _)) => name,
        None => bail!("Unknown project \"{}\"", name),
    };
    if name.matches(&config.settings.project) {
        config.settings.project = String::new();
    }
    config.save()?;
    info!("Forgot project: {}", name);
    Ok(())
}

/// The `project open` CLI command.
fn command_open(name: String) -> Result<()> {
    let mut config = Config::get();
    let (name, project) = config.find_project(&name)?;
    let (name, path) = (name.to_owned(), project.path.clone());
    config.settings.project = name;
    config.save()?;
    info!("Opened project: {} ({})", config.settings.project, path);
    Ok(())
}

/// The `project close` CLI command.
fn command_close() -> Result<()> {
    let mut config = Config::get();
    if config.settings.project.is_empty() {
        info!("No project is open");
        return Ok(());
    }
    let old = mem::take(&mut config.settings.project);
    config.save()?;
    info!("Closed project: {}", old);
    Ok(())
}
