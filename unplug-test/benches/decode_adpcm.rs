use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::io::BufReader;
use unplug::audio::format::PcmS16Le;
use unplug::audio::transport::HpsStream;
use unplug::audio::{ReadSamples, Samples};
use unplug::data::music::{Music, MusicDefinition};
use unplug::dvd::OpenFile;
use unplug_test as common;

const MUSIC_TO_DECODE: Music = Music::Bgm;

fn load_music() -> HpsStream {
    let mut iso = common::open_iso().expect("could not open ISO");
    let music = MusicDefinition::get(MUSIC_TO_DECODE);
    let reader = iso.open_file_at(&music.path()).expect("could not open HPS file");
    let mut reader = BufReader::new(reader);
    HpsStream::open(&mut reader, music.name).expect("failed to load HPS file")
}

fn decode_adpcm(music: &HpsStream) -> Vec<Samples<'static, PcmS16Le>> {
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
        b.iter_with_large_drop(|| decode_adpcm(music))
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
