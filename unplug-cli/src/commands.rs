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

use crate::context::Context;
use crate::opt::Command;
use anyhow::Result;

/// Runs a CLI command using `ctx`.
pub fn execute(ctx: Context, command: Command) -> Result<()> {
    match command {
        Command::Archive(opt) => archive::command(ctx, opt),
        Command::Audio(opt) => audio::command(ctx, opt),
        Command::Config(opt) => config::command(ctx, opt),
        Command::Dolphin(opt) => dolphin::command(ctx, opt),
        Command::Globals(opt) => globals::command(ctx, opt),
        Command::Iso(opt) => iso::command(ctx, opt),
        Command::List(opt) => list::command(ctx, opt),
        Command::Messages(opt) => messages::command(ctx, opt),
        Command::Project(opt) => project::command(ctx, opt),
        Command::Qp(opt) => archive::command_qp(ctx, opt),
        Command::Script(opt) => script::command(ctx, opt),
        Command::Shop(opt) => shop::command(ctx, opt),
        Command::Stage(opt) => stage::command(ctx, opt),
        #[cfg(feature = "debug")]
        Command::Debug(opt) => debug::command(ctx, opt),
        #[cfg(test)]
        Command::Test => Ok(()),
    }
}
