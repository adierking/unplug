use anyhow::Result;
use log::{info, warn};
use std::io::Cursor;
use unplug::audio::transport::SoundBank;
use unplug::common::WriteTo;
use unplug::data::sound_bank::{SoundBank as SoundBankId, SOUND_BANKS};
use unplug::dvd::OpenFile;
use unplug_test as common;

/// Banks which don't rebuild to be bit-for-bit identical to the original file due to looping sounds
/// being truncated. sfx_sample.ssm is the largest bank and it rebuilds identically, so whatever.
const NON_IDENTICAL_BANKS: &[SoundBankId] = &[
    SoundBankId::Stage02,
    SoundBankId::Stage05,
    SoundBankId::Kitchen,
    SoundBankId::Souko,
    SoundBankId::Papamama,
    SoundBankId::Trex,
    SoundBankId::Army,
    SoundBankId::UraniwaAmbient2,
    SoundBankId::Martial,
    SoundBankId::Ending,
];

#[test]
fn test_rebuild_all_sounds() -> Result<()> {
    common::init_logging();

    let mut iso = common::open_iso()?;
    for bank in SOUND_BANKS {
        let path = bank.path();
        info!("Reading {}", path);
        let mut reader = iso.open_file_at(&path)?;
        let mut original_bytes = vec![];
        reader.read_to_end(&mut original_bytes)?;

        let ssm = SoundBank::open(&mut Cursor::new(&original_bytes), path.as_ref())?;
        info!("Rebuilding sound bank");
        let mut cursor = Cursor::new(vec![]);
        ssm.write_to(&mut cursor)?;

        if NON_IDENTICAL_BANKS.contains(&bank.id) {
            warn!("{} is known-broken; skipping comparison", path);
        } else {
            let rebuilt_bytes = cursor.into_inner();
            assert!(original_bytes == rebuilt_bytes);
        }
    }
    Ok(())
}
