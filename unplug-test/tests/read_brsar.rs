use anyhow::Result;
use log::info;
use std::fs::File;
use std::io::BufReader;
use unplug::audio::transport::Brsar;
use unplug::common::ReadFrom;
use unplug_test as common;

const EXPECTED_NUM_SOUNDS: usize = 1427;
const EXPECTED_NUM_COLLECTIONS: usize = 133;
const EXPECTED_NUM_GROUPS: usize = 26;

const UFO_SOUND_INDEX: usize = 1076;
const UFO_SOUND_NAME: &str = "UFO_KIME";
const UFO_COLLECTION_INDEX: usize = 119;
const UFO_GROUP_INDEX: usize = 11;
const UFO_GROUP_NAME: &str = "GROUP_UFO";

#[test]
fn test_read_brsar() -> Result<()> {
    common::init_logging();

    let brsar_path = match common::brsar_path() {
        Some(path) => path,
        None => return Ok(()), // Skip test if no path is set
    };
    info!("Reading {}", brsar_path);
    let mut reader = BufReader::new(File::open(&brsar_path)?);
    let brsar = Brsar::read_from(&mut reader)?;

    // Check that we read everything
    assert_eq!(brsar.sounds.len(), EXPECTED_NUM_SOUNDS);
    assert_eq!(brsar.collections.len(), EXPECTED_NUM_COLLECTIONS);
    assert_eq!(brsar.groups.len(), EXPECTED_NUM_GROUPS);

    // Check a random sound
    let sound = &brsar.sounds[UFO_SOUND_INDEX];
    assert_eq!(brsar.symbol(sound.name_index), UFO_SOUND_NAME);
    assert_eq!(sound.collection_index, UFO_COLLECTION_INDEX as u32);
    let collection = &brsar.collections[UFO_COLLECTION_INDEX];
    assert_eq!(collection.groups[0].index, UFO_GROUP_INDEX as u32);
    let group = &brsar.groups[UFO_GROUP_INDEX];
    assert_eq!(brsar.symbol(group.name_index), UFO_GROUP_NAME);
    Ok(())
}
