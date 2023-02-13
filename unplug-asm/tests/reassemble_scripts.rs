use anyhow::Result;
use std::io::{BufReader, Cursor, Seek, SeekFrom};
use tracing::info;
use unplug::common::WriteTo;
use unplug::data::{Resource, Stage as StageId};
use unplug::dvd::{ArchiveReader, OpenFile};
use unplug::globals::{GlobalsBuilder, GlobalsReader};
use unplug::stage::Stage;
use unplug_asm as asm;
use unplug_asm::assembler::ProgramAssembler;
use unplug_asm::compiler::CompiledScript;
use unplug_asm::lexer::Lexer;
use unplug_asm::parser::Parser;
use unplug_asm::program::Program;
use unplug_test as common;

fn program_string(program: &Program) -> String {
    let mut bytes = vec![];
    asm::write_program(program, &mut bytes).unwrap();
    String::from_utf8(bytes).unwrap()
}

fn assemble(source: &str) -> Result<CompiledScript> {
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let ast = parser.parse().unwrap();
    let program = ProgramAssembler::new(&ast).assemble().unwrap();
    Ok(asm::compile(&program).unwrap())
}

#[test]
fn test_reassemble_scripts() -> Result<()> {
    common::init_logging();

    let mut iso = common::open_iso()?;
    let mut qp = ArchiveReader::open(iso.open_file_at(common::QP_PATH)?)?;

    info!("Reading {}", common::QP_GLOBALS_PATH);
    let file = qp.open_file_at(common::QP_GLOBALS_PATH)?;
    let mut globals = GlobalsReader::open(file)?;
    let libs = globals.read_libs()?;

    info!("Reassembling globals");
    let reassembled_libs = {
        let program = asm::disassemble_globals(&libs)?;
        let source = program_string(&program);
        let compiled = assemble(&source)?;
        let compiled_libs = compiled.into_libs()?;
        info!("Reading the reassembled globals");
        let mut cursor = Cursor::new(Vec::<u8>::new());
        GlobalsBuilder::new().base(&mut globals).libs(&compiled_libs).write_to(&mut cursor)?;
        drop(globals);
        cursor.seek(SeekFrom::Start(0))?;
        let mut rebuilt = GlobalsReader::open(cursor)?;
        rebuilt.read_libs()?
    };

    info!("Comparing scripts");
    common::assert_scripts_match(&libs.script, &reassembled_libs.script);

    for id in StageId::iter() {
        let stage_path = id.qp_path();
        info!("Reading {}", stage_path);
        let mut file = BufReader::new(qp.open_file_at(&stage_path)?);
        let original = Stage::read_from(&mut file, &libs)?;

        info!("Reassembling the stage");
        let program = asm::disassemble_stage(&original, id.name())?;
        let source = program_string(&program);
        let compiled = assemble(&source)?;
        let compiled_stage = compiled.into_stage(original.clone_without_script())?;
        let mut cursor = Cursor::new(Vec::<u8>::new());
        compiled_stage.write_to(&mut cursor)?;

        info!("Reading the reassembled stage");
        cursor.seek(SeekFrom::Start(0))?;
        let rebuilt = Stage::read_from(&mut cursor, &libs)?;
        assert_eq!(original.settings, rebuilt.settings);
        assert_eq!(original.objects.len(), rebuilt.objects.len());
        assert_eq!(original.actors, rebuilt.actors);
        assert_eq!(original.unk_28, rebuilt.unk_28);
        assert_eq!(original.unk_2c, rebuilt.unk_2c);
        assert_eq!(original.unk_30, rebuilt.unk_30);

        info!("Comparing scripts");
        common::assert_scripts_match(&original.script, &rebuilt.script);
    }
    Ok(())
}
