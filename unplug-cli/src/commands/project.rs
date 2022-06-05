use crate::config::{Config, Project, ProjectKind};
use crate::context::Context;
use crate::opt::{ProjectAddOpt, ProjectCommand, ProjectForgetOpt, ProjectInfoOpt, ProjectOpenOpt};
use anyhow::{anyhow, bail, Result};
use log::info;
use std::fs;
use std::mem;

/// The `project info` CLI command.
fn command_info(opt: ProjectInfoOpt) -> Result<()> {
    let config = Config::get();
    let (name, project) = match &opt.name {
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
fn command_add(opt: ProjectAddOpt) -> Result<()> {
    let filename = opt
        .path
        .file_name()
        .ok_or_else(|| anyhow!("Invalid project path: {}", opt.path.display()))?
        .to_string_lossy()
        .to_lowercase();

    // If a project name was not passed in, use the part of the filename before the extension
    let (name, ext) = match &opt.name {
        Some(name) => (name.as_str(), filename.rsplit('.').next().unwrap_or("")),
        None => filename.rsplit_once('.').unwrap_or((&filename, "")),
    };
    let info = fs::metadata(&opt.path)?;
    // This is lazy, but checking the extension should be fine here
    if !info.is_file() || ext != "iso" {
        bail!("Projects must point to a .iso file");
    }

    let mut config = Config::get();
    let key = name.into();
    if config.projects.contains_key(&key) {
        bail!("Project \"{}\" is already defined", name);
    }
    let project = Project { kind: ProjectKind::Iso, path: opt.path.to_string_lossy().into_owned() };
    config.projects.insert(key, project);
    config.save()?;
    info!("Added project: {}", name);

    Ok(())
}

/// The `project forget` CLI command.
fn command_forget(opt: ProjectForgetOpt) -> Result<()> {
    let mut config = Config::get();
    let name = match config.projects.remove_entry(&opt.name.clone().into()) {
        Some((name, _)) => name,
        None => bail!("Unknown project \"{}\"", opt.name),
    };
    if name.matches(&config.settings.project) {
        config.settings.project = String::new();
    }
    config.save()?;
    info!("Forgot project: {}", name);
    Ok(())
}

/// The `project open` CLI command.
fn command_open(opt: ProjectOpenOpt) -> Result<()> {
    let mut config = Config::get();
    let (name, project) = config.find_project(&opt.name)?;
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

/// The `project` CLI command.
pub fn command(_ctx: Context, opt: ProjectCommand) -> Result<()> {
    match opt {
        ProjectCommand::Info(opt) => command_info(opt),
        ProjectCommand::List => command_list(),
        ProjectCommand::Add(opt) => command_add(opt),
        ProjectCommand::Forget(opt) => command_forget(opt),
        ProjectCommand::Open(opt) => command_open(opt),
        ProjectCommand::Close => command_close(),
    }
}
