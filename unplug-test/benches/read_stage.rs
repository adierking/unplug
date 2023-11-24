use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::io::Cursor;
use unplug::data::Stage as StageId;
use unplug::dvd::{ArchiveReader, OpenFile};
use unplug::globals::{GlobalsReader, Libs};
use unplug::stage::Stage;
use unplug_test as common;

const TEST_STAGE: StageId = StageId::LivingRoom;

struct StageContext {
    libs: Libs,
    data: Vec<u8>,
}

fn preread() -> StageContext {
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
    let mut reader = qp.open_file_at(&TEST_STAGE.qp_path()).expect("failed to open stage file");
    let mut data = vec![];
    reader.read_to_end(&mut data).expect("failed to read stage data");
    StageContext { libs, data }
}

fn read_stage(context: &StageContext) -> Stage {
    let mut cursor = Cursor::new(context.data.as_slice());
    Stage::read_from(&mut cursor, &context.libs).expect("failed to read stage")
}

pub fn bench(c: &mut Criterion) {
    c.bench_with_input(BenchmarkId::new("read_stage", 0), &preread(), |b, data| {
        b.iter_with_large_drop(|| read_stage(data));
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
