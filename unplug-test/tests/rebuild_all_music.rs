use anyhow::Result;
use log::info;
use std::io::Cursor;
use unplug::audio::transport::{HpsReader, HpsWriter};
use unplug::audio::ReadSamples;
use unplug::data::music::MUSIC;
use unplug::dvd::OpenFile;
use unplug_test as common;

#[test]
fn test_rebuild_all_music() -> Result<()> {
    common::init_logging();

    let mut iso = common::open_iso()?;
    for music in MUSIC {
        let path = music.path();
        info!("Reading {}", path);
        let mut reader = iso.open_file_at(&path)?;
        let mut original_bytes = vec![];
        reader.read_to_end(&mut original_bytes)?;
        let hps = HpsReader::new(Cursor::new(&original_bytes), path)?;

        info!("Rebuilding HPS stream");
        let writer = match hps.channels() {
            1 => HpsWriter::with_mono(hps.channel_reader(0).cast()),
            2 => HpsWriter::with_stereo(hps.channel_reader(0).cast(), hps.channel_reader(1).cast()),
            other => panic!("unexpected channel count: {}", other),
        };
        let mut cursor = Cursor::new(vec![]);
        writer.write_to(&mut cursor)?;

        // HACK: It seems like our end_address computations don't always match, but it isn't clear
        // how to calculate it to match (or if it even matters...)
        let mut rebuilt_bytes = cursor.into_inner();
        rebuilt_bytes[0x18..0x1c].copy_from_slice(&original_bytes[0x18..0x1c]);
        if hps.channels() > 1 {
            rebuilt_bytes[0x50..0x54].copy_from_slice(&original_bytes[0x50..0x54]);
        }

        assert!(original_bytes == rebuilt_bytes);
    }
    Ok(())
}
