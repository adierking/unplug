use anyhow::Result;
use log::info;
use std::io::{BufReader, Cursor, Seek, SeekFrom};
use unplug::common::WriteTo;
use unplug::data::stage::STAGES;
use unplug::dvd::{ArchiveReader, OpenFile};
use unplug::globals::GlobalsReader;
use unplug::stage::Stage;
use unplug_test as common;

#[test]
fn test_rebuild_all_stages() -> Result<()> {
    common::init_logging();

    let mut iso = common::open_iso()?;
    let mut qp = ArchiveReader::open(iso.open_file_at(common::QP_PATH)?)?;

    let libs = {
        info!("Reading {}", common::QP_GLOBALS_PATH);
        let file = qp.open_file_at(common::QP_GLOBALS_PATH)?;
        let mut globals = GlobalsReader::open(file)?;
        globals.read_libs()?
    };

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
        // TODO: Ideally we could actually compare the scripts block-by-block, but that's tricky
        // because things like file offsets won't line up
        assert_eq!(original.script.len(), rebuilt.script.len());
    }

    Ok(())
}
