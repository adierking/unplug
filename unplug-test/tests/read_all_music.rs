use anyhow::Result;
use crc32fast::Hasher;
use log::info;
use std::io::BufReader;
use unplug::audio::dsp::DspFormat;
use unplug::audio::hps::{Block, HpsStream};
use unplug::common::ReadFrom;
use unplug::data::music::{Music, MusicDefinition};
use unplug::dvd::OpenFile;
use unplug_test as common;

fn validate_channel(hps: &HpsStream, block: &Block, channel: usize) {
    if channel >= hps.channels.len() {
        return;
    }
    if hps.channels[channel].address.format != DspFormat::Adpcm {
        return;
    }
    // Basic validity check: the first byte should match the context's predictor_and_scale
    // TODO: anything else?
    let data = block.channel_data(channel);
    let expected = block.channels[channel].initial_context.predictor_and_scale;
    assert_eq!(data[0] as u16, expected);
}

fn decode_and_crc32(hps: &HpsStream) -> Result<u32> {
    let mut hasher = Hasher::new();
    let mut decoder = hps.decoder();
    while let Some(samples) = decoder.read_samples()? {
        hasher.update(&samples.bytes);
    }
    Ok(hasher.finalize())
}

#[test]
fn test_read_all_music() -> Result<()> {
    common::init_logging();

    let mut iso = common::open_iso()?;
    for &(id, expected) in CHECKSUMS {
        let path = MusicDefinition::get(id).path;
        info!("Reading {}", path);
        let mut reader = BufReader::new(iso.open_file_at(path)?);
        let hps = HpsStream::read_from(&mut reader)?;
        assert!(!hps.blocks.is_empty());
        for block in &hps.blocks {
            validate_channel(&hps, block, 0);
            validate_channel(&hps, block, 1);
        }
        let actual = decode_and_crc32(&hps)?;
        assert_eq!(actual, expected, "{:?} checksum mismatch", id);
    }

    Ok(())
}

/// Decoded audio CRC32s for finding regressions.
const CHECKSUMS: &[(Music, u32)] = &[
    (Music::Abare, 0x621690e5),
    (Music::American, 0xdee99213),
    (Music::Angry, 0x6b831e0b),
    (Music::Appearance, 0x42e0202d),
    (Music::Arise, 0x18ca8d3),
    (Music::ArmyEscapeJ1, 0xc19c0214),
    (Music::ArmyEscapeJ2, 0x8d41ba4f),
    (Music::ArmyTheme, 0x8f0338ab),
    (Music::ArmyTraining, 0x29f3f6d8),
    (Music::Battle1, 0xe9e9eecc),
    (Music::Battle2, 0x2f01a956),
    (Music::Bb, 0x8efcb1ed),
    (Music::Bgm, 0x3e0253e7),
    (Music::BgmNight, 0xaf37fe98),
    (Music::Blue, 0xe8804386),
    (Music::Blues, 0x3a1c272b),
    (Music::Capsule, 0x911ffe73),
    (Music::Car, 0x9bbeb128),
    (Music::Change, 0x14ff9779),
    (Music::Chicken, 0xc7361b69),
    (Music::Chip, 0xb866de53),
    (Music::Conquest, 0xbefc811b),
    (Music::Cooking, 0x8c4d8f3a),
    (Music::Death, 0xae6f2017),
    (Music::Departure, 0x6e13367),
    (Music::Dexy, 0x8a203320),
    (Music::Dokodaka, 0x2bdafc3b),
    (Music::Ds, 0x2486e8bb),
    (Music::Dupe, 0xed527700),
    (Music::Ecstasy, 0xeb96ccb0),
    (Music::Ennui, 0xf09aca52),
    (Music::Entrance, 0xa62d84e4),
    (Music::Faceoff, 0x3818bb8f),
    (Music::Fake, 0xab18e6ad),
    (Music::Fear, 0x31636066),
    (Music::Ff, 0xc53ba7ec),
    (Music::Funk, 0x62800a40),
    (Music::GiccoUs, 0x389087),
    (Music::Grief, 0x63ac763d),
    (Music::Heaven, 0xded5c708),
    (Music::Hiru, 0x67454189),
    (Music::HiruWipe, 0x9e64e406),
    (Music::Hock, 0x9727d315),
    (Music::Kaimono, 0x7130140d),
    (Music::Kako, 0x53bc457a),
    (Music::Kofunk1, 0xca249acf),
    (Music::Kofunk2, 0xaee09280),
    (Music::KSabi, 0x967c3953),
    (Music::KTheme1, 0x16bbb277),
    (Music::Living, 0x50996bf),
    (Music::Love, 0xe5de6089),
    (Music::MartialLaw, 0xbc5178a3),
    (Music::Memory, 0xa106c3bb),
    (Music::Menu, 0x1e2bdfd2),
    (Music::MissionFailure, 0xd701ab33),
    (Music::MissionSuccess, 0x7611f051),
    (Music::MSpd1, 0xcfa3c5d2),
    (Music::MSpd2, 0x95ad832c),
    (Music::Mugen, 0xb586438d),
    (Music::Nosehair, 0x745ef0f9),
    (Music::Nwing, 0xff0a085),
    (Music::Papa1, 0x741f5791),
    (Music::Papa2, 0xb5884437),
    (Music::Papa3, 0x430808a7),
    (Music::Party, 0x48387a9b),
    (Music::Peach, 0x75e45a5),
    (Music::Peach2, 0x1b3d3d9e),
    (Music::Pipe, 0xe1c22a2b),
    (Music::Prelude, 0xf156fd20),
    (Music::Present, 0x63c4772e),
    (Music::Rain, 0xf082181c),
    (Music::RankupJ1, 0xaba94611),
    (Music::RankupJ2, 0xd58395d),
    (Music::Recollection, 0x252861ff),
    (Music::RecollectionIntro, 0x717dd018),
    (Music::Reminiscence, 0x6c3d7841),
    (Music::Reunion, 0xd1832dbc),
    (Music::Sample, 0xbaa129eb),
    (Music::Sanpoo, 0xa3bec514),
    (Music::Shadow, 0x526bb05d),
    (Music::Ship, 0xa6ddf7f8),
    (Music::SnareLoop, 0xb34ed123),
    (Music::Souko, 0x675310f),
    (Music::Spider, 0xc0ae0314),
    (Music::Spider2, 0xc0ae0314),
    (Music::Sunmida, 0x7770b3d4),
    (Music::SuperRobo, 0x3db61ee3),
    (Music::Tao, 0x2ae00d42),
    (Music::TeaParty, 0x367718a7),
    (Music::Teriyaki, 0xf8c9c486),
    (Music::Theme, 0x4fc9a357),
    (Music::Timeslip, 0xd43199f7),
    (Music::ToyBgm1, 0x860d1c59),
    (Music::ToyBgm2, 0x9697fa5c),
    (Music::ToyBgm3, 0xb93c98f9),
    (Music::Toyrex, 0x9e10099d),
    (Music::Tpds, 0xf5f307dc),
    (Music::Training, 0x29efcd2),
    (Music::Ufobgm, 0x78ca6700),
    (Music::Victory, 0x84dee699),
    (Music::Violin, 0xff5bb9d4),
    (Music::Wrench, 0xcae439ed),
    (Music::Yodomi, 0x173b5ff8),
    (Music::Yodomi2, 0x6bd83e0),
    (Music::Yoru, 0x132bf236),
    (Music::YoruWipe, 0xc7aa3b62),
    (Music::Yusho, 0x9347a315),
    (Music::Zobin, 0xc764ed5a),
];
