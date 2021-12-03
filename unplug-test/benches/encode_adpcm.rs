use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use std::io::Cursor;
use unplug::audio::format::adpcm::EncoderBuilder;
use unplug::audio::format::GcAdpcm;
use unplug::audio::format::PcmS16Le;
use unplug::audio::transport::WavReader;
use unplug::audio::{ReadSamples, Samples};

const TEST_WAV: &[u8] = include_bytes!("../../unplug/src/test/ionpack.wav");

fn load_music() -> Samples<'static, PcmS16Le> {
    let mut wav = WavReader::new(Cursor::new(TEST_WAV), "").expect("failed to open test WAV");
    wav.read_all_samples().expect("failed to read WAV file")
}

fn encode_adpcm(music: &Samples<'static, PcmS16Le>) -> Samples<'static, GcAdpcm> {
    let music = Samples::from_pcm(music.data.as_ref(), music.channels, music.rate);
    let channel = music.into_reader("").split_channels().left();
    let mut encoder = EncoderBuilder::simple(channel).expect("failed to encode audio").0.owned();
    encoder.read_all_samples().expect("failed to encode audio")
}

pub fn bench(c: &mut Criterion) {
    c.bench_with_input(BenchmarkId::new("encode_adpcm", 0), &load_music(), |b, music| {
        b.iter_with_large_drop(|| encode_adpcm(music))
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
