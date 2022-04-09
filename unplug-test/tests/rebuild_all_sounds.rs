use anyhow::Result;
use log::{info, warn};
use std::io::Cursor;
use std::sync::Arc;
use unplug::audio::transport::ssm::BankSample;
use unplug::audio::transport::SfxBank;
use unplug::audio::{ReadSamples, SourceChannel, SourceTag};
use unplug::common::WriteTo;
use unplug::data::sfx_group::{SfxGroup, SFX_GROUPS};
use unplug::dvd::OpenFile;
use unplug_test as common;

/// Banks which don't rebuild to be bit-for-bit identical to the original file due to looping sounds
/// being truncated. sfx_sample.ssm is the largest bank and it rebuilds identically, so whatever.
const NON_IDENTICAL_BANKS: &[SfxGroup] = &[
    SfxGroup::Stage02,
    SfxGroup::Stage05,
    SfxGroup::Kitchen,
    SfxGroup::Souko,
    SfxGroup::Papamama,
    SfxGroup::Trex,
    SfxGroup::Army,
    SfxGroup::UraniwaAmbient2,
    SfxGroup::Martial,
    SfxGroup::Ending,
];

#[test]
fn test_rebuild_all_sounds() -> Result<()> {
    common::init_logging();

    let mut iso = common::open_iso()?;
    for group in SFX_GROUPS {
        let path = group.bank_path();
        info!("Reading {}", path);
        let mut reader = iso.open_file_at(&path)?;
        let mut original_bytes = vec![];
        reader.read_to_end(&mut original_bytes)?;
        let mut ssm = SfxBank::open(&mut Cursor::new(&original_bytes), path.as_ref())?;

        info!("Rebuilding sound bank");
        for original in ssm.samples_mut() {
            let tag = SourceTag::new(String::new());
            let rebuilt = if original.channels.len() == 2 {
                let mut left =
                    original.channel_reader(0, tag.clone().for_channel(SourceChannel::Left)).cast();
                let mut right =
                    original.channel_reader(1, tag.for_channel(SourceChannel::Right)).cast();
                BankSample::from_adpcm_stereo(&mut left, &mut right)?
            } else if original.channels.len() == 1 {
                BankSample::from_adpcm_mono(&mut original.channel_reader(0, tag).cast())?
            } else {
                panic!("Sound has no channels");
            };
            assert_eq!(**original, rebuilt);
            *original = Arc::new(rebuilt);
        }
        let mut cursor = Cursor::new(vec![]);
        ssm.write_to(&mut cursor)?;

        if NON_IDENTICAL_BANKS.contains(&group.id) {
            warn!("{} is known-broken; skipping comparison", path);
        } else {
            let rebuilt_bytes = cursor.into_inner();
            assert!(original_bytes == rebuilt_bytes);
        }
    }
    Ok(())
}
