use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::io::Cursor;
use unplug::audio::format::PcmS16Le;
use unplug::audio::transport::HpsReader;
use unplug::audio::{ReadSamples, Samples};
use unplug::data::Music;
use unplug::dvd::OpenFile;
use unplug_test as common;

const MUSIC_TO_DECODE: Music = Music::Bgm;

fn load_music() -> HpsReader<'static> {
    let mut iso = common::open_iso().expect("could not open ISO");
    let path = MUSIC_TO_DECODE.disc_path().unwrap();
    let mut reader = iso.open_file_at(&path).expect("could not open HPS file");
    let mut bytes = vec![];
    reader.read_to_end(&mut bytes).expect("could not read HPS file");
    HpsReader::new(Cursor::new(bytes), MUSIC_TO_DECODE.file_name().unwrap())
        .expect("failed to load HPS file")
}

fn decode_adpcm(music: &HpsReader) -> Vec<Samples<'static, PcmS16Le>> {
    let mut decoder = music.decoder().owned();
    let mut samples = vec![];
    loop {
        match decoder.read_samples() {
            Ok(Some(s)) => samples.push(s),
            Ok(None) => break samples,
            Err(e) => panic!("decode failed: {:#}", e),
        }
    }
}

pub fn bench(c: &mut Criterion) {
    c.bench_with_input(BenchmarkId::new("decode_adpcm", 0), &load_music(), |b, music| {
        b.iter_with_large_drop(|| decode_adpcm(music));
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
