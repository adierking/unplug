#![allow(clippy::needless_pass_by_value)]

pub mod archive;
pub mod audio;
pub mod config;
pub mod dolphin;
pub mod globals;
pub mod iso;
pub mod list;
pub mod messages;
pub mod project;
pub mod script;
pub mod shop;
pub mod stage;

#[cfg(feature = "debug")]
pub mod debug;

use crate::args::Command;
use crate::context::Context;
use anyhow::Result;

/// Runs a CLI command using `ctx`.
pub fn execute(ctx: Context, command: Command) -> Result<()> {
    match command {
        Command::Archive(command) => archive::command(ctx, command),
        Command::Audio(command) => audio::command(ctx, command),
        Command::Config(command) => config::command(ctx, command),
        Command::Dolphin(args) => dolphin::command(ctx, args),
        Command::Globals(command) => globals::command(ctx, command),
        Command::Iso(command) => iso::command(ctx, command),
        Command::List(command) => list::command(ctx, command),
        Command::Messages(command) => messages::command(ctx, command),
        Command::Project(command) => project::command(ctx, command),
        Command::Qp(command) => archive::command_qp(ctx, command),
        Command::Script(command) => script::command(ctx, command),
        Command::Shop(command) => shop::command(ctx, command),
        Command::Stage(command) => stage::command(ctx, command),
        #[cfg(feature = "debug")]
        Command::Debug(command) => debug::command(ctx, command),
        #[cfg(test)]
        Command::Test => Ok(()),
    }
}
