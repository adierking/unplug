use anyhow::Result;
use std::io::{Read, Seek, SeekFrom};
use unplug_test as common;

#[test]
fn test_open_dol() -> Result<()> {
    common::init_logging();

    let mut iso = common::open_iso()?;

    let (header, mut dol) = iso.open_dol()?;
    assert_eq!(header.entry_point, 0x80005240);
    let mut bytes = [0u8; 4];
    dol.read_exact(&mut bytes)?;
    assert_eq!(bytes, [0x00, 0x00, 0x01, 0x00]);
    assert_eq!(dol.seek(SeekFrom::End(0))?, 0x25e6a0);

    Ok(())
}
