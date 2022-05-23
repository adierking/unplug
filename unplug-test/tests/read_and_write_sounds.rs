use anyhow::Result;
use log::{info, warn};
use seahash::SeaHasher;
use std::hash::Hasher;
use std::io::Cursor;
use std::sync::Arc;
use unplug::audio::format::{PcmS16Le, ReadWriteBytes};
use unplug::audio::transport::ssm::BankSample;
use unplug::audio::transport::SfxBank;
use unplug::audio::{ReadSamples, SourceChannel, SourceTag};
use unplug::common::WriteTo;
use unplug::dvd::OpenFile;
use unplug_test as common;

#[test]
fn test_read_and_write_sounds() -> Result<()> {
    let rehash = common::check_rehash();
    if rehash {
        println!("const BANK_CHECKSUMS: &[(&str, u64)] = &[");
    } else {
        common::init_logging();
    }

    let mut iso = common::open_iso()?;
    for &(path, expected) in BANK_CHECKSUMS {
        info!("Reading {}", path);
        let mut reader = iso.open_file_at(path)?;
        let mut original_bytes = vec![];
        reader.read_to_end(&mut original_bytes)?;
        let mut ssm = SfxBank::open(&mut Cursor::new(&original_bytes), path)?;

        info!("Checking decoded audio");
        let mut hasher = SeaHasher::new();
        for i in 0..ssm.len() {
            let mut decoder = ssm.decoder(i);
            while let Some(samples) = decoder.read_samples()? {
                let mut bytes = vec![];
                PcmS16Le::write_bytes(&mut bytes, &samples.data)?;
                hasher.write(&bytes);
            }
        }
        let actual = hasher.finish();
        if rehash {
            println!("    ({:?}, 0x{:>016x}),", path, actual);
            continue;
        }
        assert_eq!(actual, expected, "{} checksum mismatch", path);

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

        if NON_IDENTICAL_BANKS.contains(&path) {
            warn!("{} is known-broken; skipping comparison", path);
        } else {
            let rebuilt_bytes = cursor.into_inner();
            assert!(original_bytes == rebuilt_bytes);
        }
    }

    if rehash {
        println!("];");
    }
    Ok(())
}

/// Banks which don't rebuild to be bit-for-bit identical to the original file due to looping sounds
/// being truncated. sfx_sample.ssm is the largest bank and it rebuilds identically, so whatever.
const NON_IDENTICAL_BANKS: &[&str] = &[
    "qp/sfx_stage02.ssm",
    "qp/sfx_stage05.ssm",
    "qp/sfx_kitchen.ssm",
    "qp/sfx_souko.ssm",
    "qp/sfx_papamama.ssm",
    "qp/sfx_trex.ssm",
    "qp/sfx_army.ssm",
    "qp/sfx_uraniwa_ambient2.ssm",
    "qp/sfx_martial.ssm",
    "qp/sfx_ending.ssm",
];

/// Decoded audio seahashes for finding regressions.
///
/// Run this test with `--nocapture` and the `UNPLUG_TEST_REHASH` environment variable set to
/// regenerate this list. e.g. on Unix systems:
///
///     UNPLUG_TEST_REHASH=1 cargo test --test read_and_write_sounds -- --nocapture
///
const BANK_CHECKSUMS: &[(&str, u64)] = &[
    ("qp/sfx_army.ssm", 0x808398bd2a14a5c3),
    ("qp/sfx_bb.ssm", 0x8fb4c7bc25f91ab1),
    ("qp/sfx_concert.ssm", 0x21da7a5fd19bfa45),
    ("qp/sfx_ending.ssm", 0xe3f40bff1b75ceeb),
    ("qp/sfx_gicco.ssm", 0x20002d00c9c97f17),
    ("qp/sfx_hock.ssm", 0x3f42441cbda4b76c),
    ("qp/sfx_hori.ssm", 0x101f2dab96ef749f),
    ("qp/sfx_jennyroom.ssm", 0x0ca436dfb1e9403d),
    ("qp/sfx_kaeru.ssm", 0xb795919250254dd4),
    ("qp/sfx_kitchen.ssm", 0x4acd94e96b89354b),
    ("qp/sfx_manual.ssm", 0x5a1c8db23737ded6),
    ("qp/sfx_martial.ssm", 0x65091833a1ac65b6),
    ("qp/sfx_papamama.ssm", 0xb6094c2728dbec16),
    ("qp/sfx_pipe.ssm", 0xa49c13fef90c4ddc),
    ("qp/sfx_sample.ssm", 0x1b18fa6715097635),
    ("qp/sfx_sanpoo.ssm", 0xfa8e248b6cbdddff),
    ("qp/sfx_souko.ssm", 0x62f4e1f52e03c42a),
    ("qp/sfx_stage02.ssm", 0xf2f1c11fb3b08726),
    ("qp/sfx_stage05.ssm", 0x63ead196eada6dde),
    ("qp/sfx_stage07.ssm", 0xbbb3d8f7c5e57eec),
    ("qp/sfx_trex.ssm", 0xdbbfb5c91bd3bad7),
    ("qp/sfx_ufo.ssm", 0x43ee5e72514cdb50),
    ("qp/sfx_uraniwa.ssm", 0x58e3bb3cb8169b83),
    ("qp/sfx_uraniwa_ambient1.ssm", 0x34d7a647d9b5db2b),
    ("qp/sfx_uraniwa_ambient2.ssm", 0x663a156f23918f26),
    ("qp/sfx_uraniwa_ambient3.ssm", 0x67b1be1345110e55),
];
