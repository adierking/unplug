use anyhow::Result;
use log::info;
use seahash::SeaHasher;
use std::hash::Hasher;
use std::io::BufReader;
use unplug::audio::format::{PcmS16Le, ReadWriteBytes};
use unplug::audio::transport::SoundBank;
use unplug::dvd::OpenFile;
use unplug_test as common;

#[test]
fn test_read_all_sounds() -> Result<()> {
    let rehash = common::check_rehash();
    if rehash {
        println!("const BANK_CHECKSUMS: &[(&str, u64)] = &[");
    } else {
        common::init_logging();
    }

    let mut iso = common::open_iso()?;
    for &(path, expected) in BANK_CHECKSUMS {
        info!("Reading {}", path);
        let mut reader = BufReader::new(iso.open_file_at(path)?);
        let ssm = SoundBank::open(&mut reader, path)?;
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
        } else {
            assert_eq!(actual, expected, "{} checksum mismatch", path);
        }
    }

    if rehash {
        println!("];");
    }
    Ok(())
}

/// Decoded audio seahashes for finding regressions.
///
/// Run this test with `--nocapture` and the `UNPLUG_TEST_REHASH` environment variable set to
/// regenerate this list. e.g. on Unix systems:
///
///     UNPLUG_TEST_REHASH=1 cargo test --test read_all_sounds -- --nocapture
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
