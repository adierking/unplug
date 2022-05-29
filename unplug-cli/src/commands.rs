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

#[cfg(feature = "debug")]
pub mod debug;

use crate::context::Context;
use crate::opt::Subcommand;
use anyhow::Result;

/// Runs a CLI command using `ctx`.
pub fn execute(ctx: Context, command: Subcommand) -> Result<()> {
    match command {
        Subcommand::Archive(opt) => archive::command(ctx, opt),
        Subcommand::Audio(opt) => audio::command(ctx, opt),
        Subcommand::Config(opt) => config::command(ctx, opt),
        Subcommand::Dolphin(opt) => dolphin::command(ctx, opt),
        Subcommand::Globals(opt) => globals::command(ctx, opt),
        Subcommand::Iso(opt) => iso::command(ctx, opt),
        Subcommand::List(opt) => list::command(ctx, opt),
        Subcommand::Messages(opt) => messages::command(ctx, opt),
        Subcommand::Project(opt) => project::command(ctx, opt),
        Subcommand::Qp(opt) => archive::command_qp(ctx, opt),
        Subcommand::Script(opt) => script::command(ctx, opt),
        Subcommand::Shop(opt) => shop::command(ctx, opt),
        #[cfg(feature = "debug")]
        Subcommand::Debug(opt) => debug::command(ctx, opt),
    }
}
