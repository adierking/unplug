use anyhow::Result;
use log::info;
use std::io::Cursor;
use unplug::audio::HpsStream;
use unplug::common::{ReadFrom, WriteTo};
use unplug::data::music::MUSIC;
use unplug::dvd::OpenFile;
use unplug_test as common;

#[test]
fn test_rebuild_all_music() -> Result<()> {
    common::init_logging();

    let mut iso = common::open_iso()?;
    for music in MUSIC {
        info!("Reading {}", music.path);
        let mut reader = iso.open_file_at(music.path)?;
        let mut original_bytes = vec![];
        reader.read_to_end(&mut original_bytes)?;

        let hps = HpsStream::read_from(&mut Cursor::new(&original_bytes))?;
        info!("Rebuilding HPS stream");
        let mut cursor = Cursor::new(vec![]);
        hps.write_to(&mut cursor)?;

        let rebuilt_bytes = cursor.into_inner();
        assert!(original_bytes == rebuilt_bytes);
    }
    Ok(())
}
