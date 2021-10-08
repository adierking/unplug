use anyhow::Result;
use crc32fast::Hasher;
use log::info;
use std::io::BufReader;
use unplug::audio::format::{PcmS16Le, ReadWriteBytes};
use unplug::audio::SoundBank;
use unplug::common::ReadFrom;
use unplug::dvd::OpenFile;
use unplug_test as common;

#[test]
fn test_read_all_sounds() -> Result<()> {
    common::init_logging();

    let mut iso = common::open_iso()?;
    for &(path, expected) in BANK_CHECKSUMS {
        info!("Reading {}", path);
        let mut reader = BufReader::new(iso.open_file_at(path)?);
        let ssm = SoundBank::read_from(&mut reader)?;
        let mut hasher = Hasher::new();
        for sound in &ssm.sounds {
            let mut decoder = sound.decoder();
            while let Some(samples) = decoder.read_samples()? {
                let mut bytes = vec![];
                PcmS16Le::write_bytes(&mut bytes, &samples.data)?;
                hasher.update(&bytes);
            }
        }
        let actual = hasher.finalize();
        assert_eq!(actual, expected, "{} checksum mismatch", path);
    }

    Ok(())
}

/// Decoded audio CRC32s for finding regressions.
const BANK_CHECKSUMS: &[(&str, u32)] = &[
    ("qp/sfx_army.ssm", 0xb363d0d8),
    ("qp/sfx_bb.ssm", 0x43793b96),
    ("qp/sfx_concert.ssm", 0x10a95c80),
    ("qp/sfx_ending.ssm", 0x678f2825),
    ("qp/sfx_gicco.ssm", 0x81a85002),
    ("qp/sfx_hock.ssm", 0x14569616),
    ("qp/sfx_hori.ssm", 0x9d16f13f),
    ("qp/sfx_jennyroom.ssm", 0xa92503c9),
    ("qp/sfx_kaeru.ssm", 0xf3119c77),
    ("qp/sfx_kitchen.ssm", 0x592dba73),
    ("qp/sfx_manual.ssm", 0xa3fc46e7),
    ("qp/sfx_martial.ssm", 0x18a03160),
    ("qp/sfx_papamama.ssm", 0x52507df6),
    ("qp/sfx_pipe.ssm", 0xa4f8215d),
    ("qp/sfx_sample.ssm", 0x8006299a),
    ("qp/sfx_sanpoo.ssm", 0x84b310d5),
    ("qp/sfx_souko.ssm", 0x7c145b5),
    ("qp/sfx_stage02.ssm", 0x28924554),
    ("qp/sfx_stage05.ssm", 0xc6a30813),
    ("qp/sfx_stage07.ssm", 0xc84be6b),
    ("qp/sfx_trex.ssm", 0x1964ea9d),
    ("qp/sfx_ufo.ssm", 0x682c83de),
    ("qp/sfx_uraniwa.ssm", 0x78bcfffb),
    ("qp/sfx_uraniwa_ambient1.ssm", 0x65a1713d),
    ("qp/sfx_uraniwa_ambient2.ssm", 0xba15ba3e),
    ("qp/sfx_uraniwa_ambient3.ssm", 0xa4ab024e),
];
