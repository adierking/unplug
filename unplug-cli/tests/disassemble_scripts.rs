use anyhow::Result;
use tempfile::TempDir;
use unplug_cli::commands::script;
use unplug_cli::context::Context;
use unplug_cli::opt::ScriptDisassembleAllOpt;
use unplug_test as common;

#[test]
fn test_disassemble_scripts() -> Result<()> {
    common::init_logging();
    let ctx = Context::Iso(common::iso_path()?.into());
    let out_dir = TempDir::new()?;
    let opt = ScriptDisassembleAllOpt { output: out_dir.path().to_owned() };
    script::command_disassemble_all(ctx, opt)
}
