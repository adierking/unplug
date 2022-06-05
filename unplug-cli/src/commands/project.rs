use crate::common::IString;
use crate::config::{Config, Project, ProjectKind};
use crate::context::Context;
use crate::opt::{
    ProjectAddOpt, ProjectCommand, ProjectForgetOpt, ProjectInfoOpt, ProjectNewOpt, ProjectOpenOpt,
    ProjectWipeOpt,
};
use crate::terminal::ask_yes_no;
use anyhow::{anyhow, bail, Result};
use log::info;
use std::fs;
use std::mem;
use std::path::Path;

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

/// The `project new` CLI command.
fn command_new(opt: ProjectNewOpt) -> Result<()> {
    let mut config = Config::get();
    let project_key = IString::new(&opt.name);
    if !opt.force && config.projects.contains_key(&project_key) {
        bail!("Project \"{}\" is already defined (use --force to overwrite)", opt.name);
    }

    let source = match opt.source {
        Some(s) => s.canonicalize()?,
        None if !config.settings.default_iso.is_empty() => {
            Path::new(&config.settings.default_iso).to_owned()
        }
        None => bail!("No default ISO is configured. Use `config set default-iso <PATH>`."),
    };
    let dest = match opt.output {
        Some(o) => o.canonicalize()?,
        None => source.with_file_name(format!("{}.iso", opt.name)),
    };

    info!("Source path: {}", source.display());
    info!("Destination path: {}", dest.display());
    if source == dest {
        bail!("The source and destination paths cannot be the same");
    }
    if !opt.force && dest.exists() {
        bail!("The destination file already exists (use --force to overwrite)");
    }
    fs::copy(&source, &dest)?;

    let project = Project { kind: ProjectKind::Iso, path: dest.to_string_lossy().into_owned() };
    config.projects.insert(project_key, project);
    config.save()?;
    info!("Created project: {}", opt.name);
    Ok(())
}

/// The `project wipe` CLI command.
fn command_wipe(opt: ProjectWipeOpt) -> Result<()> {
    let mut config = Config::get();
    let (name, project) = config.find_project(&opt.name)?;

    if !opt.force {
        print!("This will irreversibly delete all data for {}! Continue (y/n)? ", name);
        if !ask_yes_no() {
            println!("Canceled.");
            return Ok(());
        }
    }

    info!("Deleting file: {}", project.path);
    std::fs::remove_file(&project.path)?;

    let project_key = IString::new(name);
    config.projects.remove_entry(&project_key);
    if project_key.matches(&config.settings.project) {
        config.settings.project = String::new();
    }
    config.save()?;
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
        ProjectCommand::New(opt) => command_new(opt),
        ProjectCommand::Wipe(opt) => command_wipe(opt),
        ProjectCommand::Add(opt) => command_add(opt),
        ProjectCommand::Forget(opt) => command_forget(opt),
        ProjectCommand::Open(opt) => command_open(opt),
        ProjectCommand::Close => command_close(),
    }
}
