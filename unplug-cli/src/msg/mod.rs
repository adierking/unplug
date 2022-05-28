pub mod common;

mod export;
mod import;

pub use export::command as command_export;
pub use import::command as command_import;

use crate::context::Context;
use crate::opt::MessagesCommand;
use anyhow::Result;

/// The `messages` CLI command.
pub fn command(ctx: Context, opt: MessagesCommand) -> Result<()> {
    match opt {
        MessagesCommand::Export(opt) => command_export(ctx, opt),
        MessagesCommand::Import(opt) => command_import(ctx, opt),
    }
}
