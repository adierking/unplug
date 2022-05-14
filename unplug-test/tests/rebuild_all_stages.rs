use anyhow::Result;
use log::{debug, info};
use std::collections::{HashMap, HashSet};
use std::io::{BufReader, Cursor, Seek, SeekFrom};
use std::mem;
use unplug::common::WriteTo;
use unplug::data::stage::STAGES;
use unplug::dvd::{ArchiveReader, OpenFile};
use unplug::event::{Block, BlockId, CodeBlock, Command, DataBlock, Ip, Script};
use unplug::globals::{GlobalsBuilder, GlobalsReader};
use unplug::stage::Stage;
use unplug_test as common;

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
    debug!("Compared {} block pairs", visited.len());
}

fn compare_subroutines(
    script1: &Script,
    sub1: BlockId,
    script2: &Script,
    sub2: BlockId,
    visited: &mut HashSet<(BlockId, BlockId)>,
) {
    if !visited.insert((sub1, sub2)) {
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
fn test_rebuild_all_stages() -> Result<()> {
    common::init_logging();

    let mut iso = common::open_iso()?;
    let mut qp = ArchiveReader::open(iso.open_file_at(common::QP_PATH)?)?;

    info!("Reading {}", common::QP_GLOBALS_PATH);
    let file = qp.open_file_at(common::QP_GLOBALS_PATH)?;
    let mut globals = GlobalsReader::open(file)?;
    let libs = globals.read_libs()?;

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

    for stage_def in STAGES {
        let stage_path = stage_def.path();
        info!("Reading {}", stage_path);
        let mut file = BufReader::new(qp.open_file_at(&stage_path)?);
        let original = Stage::read_from(&mut file, &libs)?;

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
