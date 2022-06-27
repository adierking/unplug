use anyhow::Result;
use log::info;
use std::io::{BufReader, Cursor, Seek, SeekFrom};
use unplug::common::WriteTo;
use unplug::data::Stage as StageId;
use unplug::dvd::{ArchiveReader, OpenFile};
use unplug::globals::GlobalsReader;
use unplug::shop::Shop;
use unplug::stage::Stage;
use unplug_test as common;

#[test]
fn test_read_and_write_shop() -> Result<()> {
    common::init_logging();

    let mut iso = common::open_iso()?;
    let mut qp = ArchiveReader::open(iso.open_file_at(common::QP_PATH)?)?;

    let libs = {
        info!("Reading {}", common::QP_GLOBALS_PATH);
        let file = qp.open_file_at(common::QP_GLOBALS_PATH)?;
        let mut globals = GlobalsReader::open(file)?;
        globals.read_libs()?
    };

    let stage_path = StageId::ChibiHouse.qp_path();
    info!("Reading {}", stage_path);
    let mut file = BufReader::new(qp.open_file_at(&stage_path)?);
    let mut original_stage = Stage::read_from(&mut file, &libs)?;
    let original_shop = Shop::parse(&original_stage.script)?;

    info!("Recompiling the shop");
    original_shop.compile(&mut original_stage.script)?;

    info!("Rebuilding the stage");
    let mut cursor = Cursor::new(Vec::<u8>::new());
    original_stage.write_to(&mut cursor)?;

    info!("Reading the rebuilt stage");
    cursor.seek(SeekFrom::Start(0))?;
    let rebuilt_stage = Stage::read_from(&mut cursor, &libs)?;
    let rebuilt_shop = Shop::parse(&rebuilt_stage.script)?;

    info!("Comparing shop configurations");
    let original_slots = original_shop.slots();
    let rebuilt_slots = rebuilt_shop.slots();
    assert_eq!(original_slots.len(), rebuilt_slots.len());
    for (i, (actual, expected)) in rebuilt_slots.iter().zip(original_slots).enumerate() {
        assert_eq!(actual, expected, "slot {}", i);
    }
    Ok(())
}
