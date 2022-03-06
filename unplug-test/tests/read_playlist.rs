use anyhow::Result;
use log::info;
use std::io::BufReader;
use unplug::audio::metadata::sem::{Action, Command, SfxPlaylist, SoundMaterial};
use unplug::common::ReadFrom;
use unplug::dvd::OpenFile;
use unplug_test as common;

const PLAYLIST_PATH: &str = "qp/sfx_sample.sem";

#[test]
fn test_read_playlist() -> Result<()> {
    common::init_logging();

    let mut iso = common::open_iso()?;
    info!("Reading {}", PLAYLIST_PATH);
    let mut reader = BufReader::new(iso.open_file_at(PLAYLIST_PATH)?);
    let playlist = SfxPlaylist::read_from(&mut reader)?;
    assert_eq!(playlist.group_indexes, EXPECTED_GROUP_INDEXES);
    assert_eq!(playlist.sounds.len(), 1120);
    assert_eq!(
        playlist.sounds[0x2d6], // randomly-chosen sound effect with 3 actions
        SoundMaterial {
            actions: vec![
                Action { command: Command::Sample, delay: 0, data: 0x02d1 },
                Action { command: Command::Unk6, delay: 0, data: 0x99 },
                Action { command: Command::End, delay: 0, data: 0 },
            ],
        }
    );

    Ok(())
}

/// The expected base index for each group in the bank.
const EXPECTED_GROUP_INDEXES: &[u32] = &[
    0x0000, 0x01ba, 0x01e8, 0x020b, 0x0231, 0x0259, 0x0283, 0x02a7, 0x02d2, 0x02fe, 0x031f, 0x0320,
    0x0326, 0x0338, 0x0352, 0x0363, 0x038b, 0x03ab, 0x03e1, 0x040e, 0x0416, 0x041e, 0x0420, 0x0446,
    0x0450,
];
