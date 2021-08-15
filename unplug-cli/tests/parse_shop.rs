use anyhow::Result;
use lazy_static::lazy_static;
use log::info;
use std::collections::HashSet;
use std::io::BufReader;
use std::iter::FromIterator;
use unplug::data::atc::AtcId;
use unplug::data::item::ItemId;
use unplug::data::stage::CHIBI_HOUSE;
use unplug::dvd::{ArchiveReader, OpenFile};
use unplug::globals::GlobalsReader;
use unplug::stage::Stage;
use unplug_cli::shop::{Requirement, Shop, Slot};
use unplug_test as common;

/// Convenience macro for initializing HashSets
macro_rules! set {
    [$($value:expr),* $(,)*] => {
        HashSet::from_iter(vec![$($value),*])
    };
}

lazy_static! {
    /// Expected shop contents
    static ref EXPECTED: Vec<Slot> = vec![
        Slot {
            item: Some(ItemId::Timer5),
            limit: 1,
            enabled: set![Requirement::MissingItem(ItemId::Timer5)],
            visible: set![],
        },
        Slot {
            item: Some(ItemId::Timer10),
            limit: 1,
            enabled: set![Requirement::MissingItem(ItemId::Timer10)],
            visible: set![],
        },
        Slot {
            item: Some(ItemId::Timer15),
            limit: 1,
            enabled: set![Requirement::MissingItem(ItemId::Timer15)],
            visible: set![],
        },
        Slot { item: Some(ItemId::PinkFlowerSeed), limit: 10, enabled: set![], visible: set![] },
        Slot { item: Some(ItemId::BlueFlowerSeed), limit: 10, enabled: set![], visible: set![] },
        Slot { item: Some(ItemId::WhiteFlowerSeed), limit: 10, enabled: set![], visible: set![] },
        Slot { item: Some(ItemId::NectarFlowerSeed), limit: 1, enabled: set![], visible: set![] },
        Slot {
            item: Some(ItemId::ChargeChip),
            limit: 1,
            enabled: set![Requirement::MissingItem(ItemId::ChargeChip)],
            visible: set![Requirement::HaveAtc(AtcId::ChibiBlaster)],
        },
        Slot {
            item: Some(ItemId::TraumaSuit),
            limit: 1,
            enabled: set![Requirement::MissingItem(ItemId::TraumaSuit)],
            visible: set![Requirement::HaveFlag(19)],
        },
        Slot {
            item: Some(ItemId::ChibiBattery),
            limit: 1,
            enabled: set![Requirement::MissingItem(ItemId::ChibiBattery)],
            visible: set![Requirement::HaveItem(ItemId::TraumaSuit)],
        },
        Slot {
            item: Some(ItemId::ChibiBlaster),
            limit: 1,
            enabled: set![Requirement::MissingAtc(AtcId::ChibiBlaster)],
            visible: set![],
        },
        Slot {
            item: Some(ItemId::RangeChip),
            limit: 1,
            enabled: set![Requirement::MissingItem(ItemId::RangeChip)],
            visible: set![Requirement::HaveAtc(AtcId::ChibiBlaster)],
        },
        Slot {
            item: Some(ItemId::ChibiRadar),
            limit: 1,
            enabled: set![Requirement::MissingAtc(AtcId::ChibiRadar)],
            visible: set![Requirement::HaveFlag(601)],
        },
        Slot {
            item: Some(ItemId::AlienEarChip),
            limit: 1,
            enabled: set![Requirement::MissingItem(ItemId::AlienEarChip)],
            visible: set![Requirement::HaveFlag(474)],
        },
        Slot {
            item: Some(ItemId::HotRod),
            limit: 1,
            enabled: set![Requirement::MissingItem(ItemId::HotRod)],
            visible: set![],
        },
        Slot {
            item: Some(ItemId::SpaceScrambler),
            limit: 1,
            enabled: set![Requirement::MissingItem(ItemId::SpaceScrambler)],
            visible: set![],
        },
        Slot { item: None, limit: 0, enabled: set![], visible: set![] },
        Slot { item: None, limit: 0, enabled: set![], visible: set![] },
        Slot { item: None, limit: 0, enabled: set![], visible: set![] },
        Slot { item: None, limit: 0, enabled: set![], visible: set![] },
    ];
}

#[test]
fn test_parse_shop() -> Result<()> {
    common::init_logging();

    let mut iso = common::open_iso()?;
    let mut qp = ArchiveReader::open(iso.open_file_at(common::QP_PATH)?)?;

    let libs = {
        info!("Reading {}", common::QP_GLOBALS_PATH);
        let file = qp.open_file_at(common::QP_GLOBALS_PATH)?;
        let mut globals = GlobalsReader::open(file)?;
        globals.read_libs()?
    };

    let stage_path = CHIBI_HOUSE.path();
    info!("Reading {}", stage_path);
    let mut file = BufReader::new(qp.open_file_at(&stage_path)?);
    let chibi_house = Stage::read_from(&mut file, &libs)?;
    let shop = Shop::parse(&chibi_house.script)?;

    assert_eq!(shop.slots.len(), EXPECTED.len());
    for (i, (actual, expected)) in shop.slots.iter().zip(&*EXPECTED).enumerate() {
        assert_eq!(actual, expected, "slot {}", i);
    }

    Ok(())
}
