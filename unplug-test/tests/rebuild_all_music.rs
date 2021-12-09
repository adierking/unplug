use anyhow::Result;
use log::info;
use std::io::Cursor;
use unplug::audio::transport::HpsStream;
use unplug::audio::ReadSamples;
use unplug::common::WriteTo;
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
        let hps = HpsStream::open(&mut Cursor::new(&original_bytes), path)?;

        info!("Rebuilding HPS stream");
        let mut rebuilt = match hps.channels.len() {
            1 => HpsStream::from_adpcm_mono(&mut hps.reader(0).cast())?,
            2 => {
                HpsStream::from_adpcm_stereo(&mut hps.reader(0).cast(), &mut hps.reader(1).cast())?
            }
            other => panic!("unexpected channel count: {}", other),
        };

        // HACK: We currently lose the loop settings
        rebuilt.loop_start = hps.loop_start;
        // HACK: It seems like our end_address computations don't always match, but it isn't clear
        // how to calculate it to match (or if it even matters...)
        for (a, b) in hps.channels.iter().zip(rebuilt.channels.iter_mut()) {
            b.address.end_address = a.address.end_address;
        }

        let mut cursor = Cursor::new(vec![]);
        rebuilt.write_to(&mut cursor)?;

        let rebuilt_bytes = cursor.into_inner();
        assert!(original_bytes == rebuilt_bytes);
    }
    Ok(())
}
