use anyhow::Result;
use log::info;
use logos::Logos;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use tempfile::TempDir;
use unplug_cli::asm::Token;
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
    script::command_disassemble_all(ctx, opt)?;

    // Validate that every file passes the lexer
    for entry in fs::read_dir(out_dir.path())? {
        let entry = entry?;
        info!("Lexing {}", entry.file_name().to_string_lossy());
        let mut reader = BufReader::new(File::open(entry.path())?);
        let mut line = String::new();
        while reader.read_line(&mut line)? > 0 {
            let mut lexer = Token::lexer(&line);
            assert!(lexer.all(|t| t != Token::Error), "{:?}", line);
            line.clear();
        }
    }
    Ok(())
}
