use anyhow::Result;
use lazy_static::lazy_static;
use log::info;
use std::io::BufReader;
use unplug::data::stage::{Stage as StageId, StageDefinition};
use unplug::data::{Atc, Item};
use unplug::dvd::{ArchiveReader, OpenFile};
use unplug::globals::GlobalsReader;
use unplug::shop::{Requirement, Shop, Slot};
use unplug::stage::Stage;
use unplug_test as common;

/// Convenience macro for initializing HashSets
macro_rules! set {
    [$($value:expr),* $(,)*] => {
        vec![$($value),*].into_iter().collect::<::std::collections::HashSet<_>>()
    };
}

lazy_static! {
    /// Expected shop contents
    static ref EXPECTED: Vec<Slot> = vec![
        Slot {
            item: Some(Item::Timer5),
            limit: 1,
            requirements: set![],
        },
        Slot {
            item: Some(Item::Timer10),
            limit: 1,
            requirements: set![],
        },
        Slot {
            item: Some(Item::Timer15),
            limit: 1,
            requirements: set![],
        },
        Slot { item: Some(Item::PinkFlowerSeed), limit: 10, requirements: set![] },
        Slot { item: Some(Item::BlueFlowerSeed), limit: 10, requirements: set![] },
        Slot { item: Some(Item::WhiteFlowerSeed), limit: 10, requirements: set![] },
        Slot { item: Some(Item::NectarFlowerSeed), limit: 1, requirements: set![] },
        Slot {
            item: Some(Item::ChargeChip),
            limit: 1,
            requirements: set![Requirement::HaveAtc(Atc::ChibiBlaster)],
        },
        Slot {
            item: Some(Item::TraumaSuit),
            limit: 1,
            requirements: set![Requirement::HaveFlag(19)],
        },
        Slot {
            item: Some(Item::ChibiBattery),
            limit: 1,
            requirements: set![Requirement::HaveItem(Item::TraumaSuit)],
        },
        Slot {
            item: Some(Item::ChibiBlaster),
            limit: 1,
            requirements: set![],
        },
        Slot {
            item: Some(Item::RangeChip),
            limit: 1,
            requirements: set![Requirement::HaveAtc(Atc::ChibiBlaster)],
        },
        Slot {
            item: Some(Item::ChibiRadar),
            limit: 1,
            requirements: set![Requirement::HaveFlag(601)],
        },
        Slot {
            item: Some(Item::AlienEarChip),
            limit: 1,
            requirements: set![Requirement::HaveFlag(474)],
        },
        Slot {
            item: Some(Item::HotRod),
            limit: 1,
            requirements: set![],
        },
        Slot {
            item: Some(Item::SpaceScrambler),
            limit: 1,
            requirements: set![],
        },
        Slot { item: None, limit: 0, requirements: set![] },
        Slot { item: None, limit: 0, requirements: set![] },
        Slot { item: None, limit: 0, requirements: set![] },
        Slot { item: None, limit: 0, requirements: set![] },
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

    let stage_path = StageDefinition::get(StageId::ChibiHouse).path;
    info!("Reading {}", stage_path);
    let mut file = BufReader::new(qp.open_file_at(stage_path)?);
    let chibi_house = Stage::read_from(&mut file, &libs)?;
    let shop = Shop::parse(&chibi_house.script)?;

    assert_eq!(shop.slots().len(), EXPECTED.len());
    for (i, (actual, expected)) in shop.slots().iter().zip(&*EXPECTED).enumerate() {
        assert_eq!(actual, expected, "slot {}", i);
    }

    Ok(())
}
