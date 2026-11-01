use std::io::Cursor;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use unplug::data::{Resource, Stage as StageId};
use unplug::dvd::{ArchiveReader, OpenFile};
use unplug::globals::GlobalsReader;
use unplug::stage::Stage;
use unplug_asm as asm;
use unplug_asm::assembler::ProgramAssembler;
use unplug_asm::ast::Ast;
use unplug_asm::compiler::CompiledScript;
use unplug_asm::lexer::Lexer;
use unplug_asm::parser::Parser;
use unplug_asm::program::Program;
use unplug_test as common;

const TEST_STAGE: StageId = StageId::Foyer;

fn disassemble() -> String {
    let mut iso = common::open_iso().expect("failed to open ISO");
    let mut qp = {
        let reader = iso.open_file_at(common::QP_PATH).expect("failed to open qp.bin");
        ArchiveReader::open(reader).expect("failed to read qp.bin")
    };
    let libs = {
        let reader = qp.open_file_at(common::QP_GLOBALS_PATH).expect("failed to open globals.bin");
        let mut globals = GlobalsReader::open(reader).expect("failed to read globals.bin");
        globals.read_libs().expect("failed to read libs")
    };
    let stage = {
        let mut data = vec![];
        let mut reader = qp.open_file_at(&TEST_STAGE.qp_path()).expect("failed to open stage file");
        reader.read_to_end(&mut data).expect("failed to read stage data");
        Stage::read_from(&mut Cursor::new(data), &libs).expect("failed to read stage")
    };
    let program =
        asm::disassemble_stage(&stage, TEST_STAGE.name()).expect("failed to disassemble stage");
    let mut source = vec![];
    asm::write_program(&program, &mut source).expect("failed to write the program source");
    String::from_utf8(source).expect("failed to parse source as UTF-8")
}

fn assemble_script(source: &str) -> (Ast, Program, CompiledScript) {
    let lexer = Lexer::new(source);
    let parser = Parser::new(lexer);
    let ast = parser.parse().unwrap();
    let program = ProgramAssembler::new(&ast).assemble().unwrap();
    let script = asm::compile(&program).unwrap();
    (ast, program, script)
}

pub fn bench(c: &mut Criterion) {
    c.bench_with_input(BenchmarkId::new("assemble_script", 0), &disassemble(), |b, source| {
        b.iter_with_large_drop(|| assemble_script(source));
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
