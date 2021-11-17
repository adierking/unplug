use anyhow::Result;
use log::info;
use std::io::Cursor;
use unplug::audio::transport::HpsStream;
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
        let mut cursor = Cursor::new(vec![]);
        hps.write_to(&mut cursor)?;

        let rebuilt_bytes = cursor.into_inner();
        assert!(original_bytes == rebuilt_bytes);
    }
    Ok(())
}
