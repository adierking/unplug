use anyhow::Result;
use chumsky::{Parser, Stream};
use log::info;
use logos::Logos;
use std::fs;
use tempfile::TempDir;
use unplug_cli::asm::{Ast, Token};
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

    // Validate that every file passes the lexer and parser
    let mut paths = fs::read_dir(out_dir.path())?.map(|e| e.unwrap().path()).collect::<Vec<_>>();
    paths.sort_unstable();
    for path in paths {
        info!("Parsing {}", path.file_name().unwrap().to_string_lossy());
        let contents = fs::read_to_string(&path)?;
        let len = contents.chars().count();
        let lexer = Token::lexer(&contents);
        let stream = Stream::from_iter(len..len + 1, lexer.spanned());
        Ast::parser().parse(stream).unwrap();
    }
    Ok(())
}
