use anyhow::Result;
use log::{debug, info};
use std::collections::{HashMap, HashSet};
use std::io::{BufReader, Cursor, Seek, SeekFrom};
use std::mem;
use unplug::common::WriteTo;
use unplug::data::Stage as StageId;
use unplug::dvd::{ArchiveReader, OpenFile};
use unplug::event::{Block, BlockId, CodeBlock, Command, DataBlock, Ip, Script};
use unplug::globals::{GlobalsBuilder, GlobalsReader};
use unplug::stage::Stage;
use unplug_test as common;

struct StageInfo {
    id: StageId,
    objects: usize,
    blocks: usize,
}

fn compare_scripts(script1: &Script, script2: &Script) {
    assert_eq!(script1.len(), script2.len());

    // Sort subroutines by offset to line them up. The new script writer implementation sorts blobs
    // by their offsets in the original file, so this actually works.
    let layout1 = script1.layout().unwrap();
    let layout2 = script2.layout().unwrap();
    let mut subs1 = layout1.subroutines().keys().copied().collect::<Vec<_>>();
    let mut subs2 = layout2.subroutines().keys().copied().collect::<Vec<_>>();
    assert_eq!(subs1.len(), subs2.len());
    let offsets1 =
        layout1.block_offsets().iter().map(|loc| (loc.id, loc.offset)).collect::<HashMap<_, _>>();
    let offsets2 =
        layout2.block_offsets().iter().map(|loc| (loc.id, loc.offset)).collect::<HashMap<_, _>>();
    subs1.sort_unstable_by_key(|a| offsets1.get(a).unwrap());
    subs2.sort_unstable_by_key(|a| offsets2.get(a).unwrap());

    let mut visited = HashSet::new();
    for (&sub1, &sub2) in subs1.iter().zip(&subs2) {
        compare_subroutines(script1, sub1, script2, sub2, &mut visited);
    }
    debug!("Compared {} blocks", visited.len());
}

fn compare_subroutines(
    script1: &Script,
    sub1: BlockId,
    script2: &Script,
    sub2: BlockId,
    visited: &mut HashSet<BlockId>,
) {
    assert_eq!(sub1, sub2);
    if !visited.insert(sub1) {
        return;
    }
    let b1 = script1.block(sub1);
    let b2 = script2.block(sub2);
    assert!(compare_blocks(b1, b2), "b1 = {:?}, b2 = {:?}", sub1, sub2);
    if let (Block::Code(code1), Block::Code(code2)) = (b1, b2) {
        if let (Some(Ip::Block(n1)), Some(Ip::Block(n2))) = (code1.next_block, code2.next_block) {
            compare_subroutines(script1, n1, script2, n2, visited);
        }
        if let (Some(Ip::Block(e1)), Some(Ip::Block(e2))) = (code1.else_block, code2.else_block) {
            compare_subroutines(script1, e1, script2, e2, visited);
        }
    }
}

fn compare_blocks(a: &Block, b: &Block) -> bool {
    match (a, b) {
        (Block::Placeholder, Block::Placeholder) => true,
        (Block::Code(a), Block::Code(b)) => compare_code(a, b),
        (Block::Data(a), Block::Data(b)) => compare_data(a, b),
        _ => false,
    }
}

fn compare_code(a: &CodeBlock, b: &CodeBlock) -> bool {
    a.next_block.is_some() == b.next_block.is_some()
        && a.else_block.is_some() == b.else_block.is_some()
        && a.commands.len() == b.commands.len()
        && a.commands.iter().zip(&b.commands).all(|(a, b)| compare_commands(a, b))
}

fn compare_commands(a: &Command, b: &Command) -> bool {
    mem::discriminant(a) == mem::discriminant(b)
}

fn compare_data(a: &DataBlock, b: &DataBlock) -> bool {
    match (a, b) {
        (DataBlock::ArrayIp(a), DataBlock::ArrayIp(b)) => a.len() == b.len(),
        _ => a == b,
    }
}

#[test]
fn test_read_and_write_stages() -> Result<()> {
    common::init_logging();

    let mut iso = common::open_iso()?;
    let mut qp = ArchiveReader::open(iso.open_file_at(common::QP_PATH)?)?;

    info!("Reading {}", common::QP_GLOBALS_PATH);
    let file = qp.open_file_at(common::QP_GLOBALS_PATH)?;
    let mut globals = GlobalsReader::open(file)?;
    let libs = globals.read_libs()?;
    assert_eq!(libs.script.blocks().len(), GLOBALS_BLOCKS);

    info!("Rebuilding globals");
    let rebuilt_libs = {
        let mut cursor = Cursor::new(Vec::<u8>::new());
        GlobalsBuilder::new().base(&mut globals).libs(&libs).write_to(&mut cursor)?;
        info!("Reading the rebuilt globals");
        cursor.seek(SeekFrom::Start(0))?;
        let mut rebuilt = GlobalsReader::open(cursor)?;
        rebuilt.read_libs()?
    };
    compare_scripts(&libs.script, &rebuilt_libs.script);
    drop(globals);

    for info in STAGE_INFO {
        let stage_path = info.id.path();
        info!("Reading {}", stage_path);
        let mut file = BufReader::new(qp.open_file_at(&stage_path)?);
        let original = Stage::read_from(&mut file, &libs)?;
        assert_eq!(original.objects.len(), info.objects);
        assert_eq!(original.script.len(), info.blocks);

        info!("Rebuilding the stage");
        let mut cursor = Cursor::new(Vec::<u8>::new());
        original.write_to(&mut cursor)?;

        info!("Reading the rebuilt stage");
        cursor.seek(SeekFrom::Start(0))?;
        let rebuilt = Stage::read_from(&mut cursor, &libs)?;
        assert_eq!(original.settings, rebuilt.settings);
        assert_eq!(original.objects.len(), rebuilt.objects.len());
        assert_eq!(original.actors, rebuilt.actors);
        assert_eq!(original.unk_28, rebuilt.unk_28);
        assert_eq!(original.unk_2c, rebuilt.unk_2c);
        assert_eq!(original.unk_30, rebuilt.unk_30);
        compare_scripts(&original.script, &rebuilt.script);
    }

    Ok(())
}

/// The expected number of script blocks in globals.bin. See below.
const GLOBALS_BLOCKS: usize = 3955;

/// A list of stages and their expected object and script block counts.
///
/// This isn't intended to be perfectly accurate (e.g. we could fix a bug in the script reader and
/// correctly change the block counts). Rather, it's for regression testing purposes - if the counts
/// for a stage change unintentionally (or a stage fails to load altogether), we can quickly detect
/// and investigate the change.
static STAGE_INFO: &[StageInfo] = &[
    StageInfo { id: StageId::Ahk, objects: 110, blocks: 65 },
    StageInfo { id: StageId::Backyard, objects: 329, blocks: 2430 },
    StageInfo { id: StageId::Basement, objects: 273, blocks: 1393 },
    StageInfo { id: StageId::Bedroom, objects: 383, blocks: 2414 },
    StageInfo { id: StageId::BedroomPast, objects: 131, blocks: 119 },
    StageInfo { id: StageId::ChibiHouse, objects: 87, blocks: 936 },
    StageInfo { id: StageId::ChibiManual, objects: 3, blocks: 162 },
    StageInfo { id: StageId::Debug, objects: 57, blocks: 621 },
    StageInfo { id: StageId::Drain, objects: 63, blocks: 180 },
    StageInfo { id: StageId::Foyer, objects: 513, blocks: 4188 },
    StageInfo { id: StageId::Hori, objects: 27, blocks: 20 },
    StageInfo { id: StageId::JennysRoom, objects: 379, blocks: 2196 },
    StageInfo { id: StageId::Junko, objects: 17, blocks: 9 },
    StageInfo { id: StageId::Kitchen, objects: 375, blocks: 3106 },
    StageInfo { id: StageId::LivingRoom, objects: 498, blocks: 3512 },
    StageInfo { id: StageId::LivingRoomBirthday, objects: 186, blocks: 96 },
    StageInfo { id: StageId::Mariko, objects: 6, blocks: 7 },
    StageInfo { id: StageId::Mory, objects: 82, blocks: 196 },
    StageInfo { id: StageId::MotherSpidersRoom, objects: 55, blocks: 75 },
    StageInfo { id: StageId::Ryosuke, objects: 2, blocks: 5 },
    StageInfo { id: StageId::Sayoko, objects: 26, blocks: 41 },
    StageInfo { id: StageId::Shun, objects: 144, blocks: 553 },
    StageInfo { id: StageId::StaffCredit, objects: 1, blocks: 4 },
    StageInfo { id: StageId::Stage08, objects: 5, blocks: 16 },
    StageInfo { id: StageId::Stage12, objects: 1, blocks: 0 },
    StageInfo { id: StageId::Stage15, objects: 1, blocks: 0 },
    StageInfo { id: StageId::Stage17, objects: 1, blocks: 0 },
    StageInfo { id: StageId::Stage19, objects: 14, blocks: 61 },
    StageInfo { id: StageId::Stage20, objects: 1, blocks: 0 },
    StageInfo { id: StageId::Stage21, objects: 1, blocks: 0 },
    StageInfo { id: StageId::Stage23, objects: 1, blocks: 3 },
    StageInfo { id: StageId::Stage24, objects: 1, blocks: 3 },
    StageInfo { id: StageId::Stage25, objects: 1, blocks: 3 },
    StageInfo { id: StageId::Stage26, objects: 1, blocks: 3 },
    StageInfo { id: StageId::Stage27, objects: 1, blocks: 3 },
    StageInfo { id: StageId::Stage28, objects: 87, blocks: 936 },
    StageInfo { id: StageId::Stage29, objects: 498, blocks: 3198 },
    StageInfo { id: StageId::Takanabe, objects: 157, blocks: 128 },
    StageInfo { id: StageId::Ufo, objects: 63, blocks: 498 },
];
