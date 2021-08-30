use anyhow::Result;
use log::info;
use std::io::BufReader;
use unplug::audio::hps::{Block, HpsStream};
use unplug::audio::SampleFormat;
use unplug::common::ReadFrom;
use unplug::data::music::MUSIC;
use unplug::dvd::OpenFile;
use unplug_test as common;

fn validate_channel(hps: &HpsStream, block: &Block, channel: usize) {
    if channel >= hps.channels.len() {
        return;
    }
    if hps.channels[channel].format != SampleFormat::Adpcm {
        return;
    }
    // Basic validity check: the first byte should match the context's predictor_and_scale
    // TODO: anything else?
    let data = block.channel_data(channel);
    let expected = block.channels[channel].initial_context.predictor_and_scale;
    assert_eq!(data[0] as u16, expected);
}

#[test]
fn test_read_all_music() -> Result<()> {
    common::init_logging();

    let mut iso = common::open_iso()?;
    for music in MUSIC {
        let path = music.path;
        info!("Reading {}", path);
        let mut reader = BufReader::new(iso.open_file_at(path)?);
        let hps = HpsStream::read_from(&mut reader)?;
        assert!(!hps.blocks.is_empty());
        for block in &hps.blocks {
            validate_channel(&hps, block, 0);
            validate_channel(&hps, block, 1);
        }
    }

    Ok(())
}
