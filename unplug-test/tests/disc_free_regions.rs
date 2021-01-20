use anyhow::Result;
use serial_test::serial;
use unplug::dvd::disc::{self, DiscRegion};
use unplug_test as common;

#[test]
#[serial]
fn test_disc_free_regions() -> Result<()> {
    common::init_logging();

    let iso = common::open_iso()?;
    assert_eq!(
        iso.free_regions(),
        [
            DiscRegion::new(0x27f82b, 0x3d86fa9d),
            DiscRegion::new(0x42420bd3, 0x1),
            DiscRegion::new(0x57057ff6, 0xa),
        ]
    );
    Ok(())
}

#[test]
#[serial]
fn test_disc_max_file_size() -> Result<()> {
    common::init_logging();

    let iso = common::open_iso()?;

    let qp = iso.files.at("qp.bin")?;
    assert_eq!(iso.files.file(qp)?.size, 0x46b288b);
    assert_eq!(iso.max_file_size(qp)?, 0x46b288c);

    let cbr_ddf = iso.files.at("tmp/cbr.ddf")?;
    assert_eq!(iso.files.file(cbr_ddf)?.size, 0x23d6);
    assert_eq!(iso.max_file_size(cbr_ddf)?, 0x23e0);

    let opening = iso.files.at("opening.bnr")?;
    assert_eq!(iso.files.file(opening)?.size, 0x1960);
    assert_eq!(iso.max_file_size(opening)?, 0x1960);
    Ok(())
}

#[test]
#[serial]
fn test_disc_max_region_size() -> Result<()> {
    common::init_logging();

    let iso = common::open_iso()?;

    assert_eq!(iso.max_region_size(0x27f82b, 0x3d86fa9d), 0x3d86fa9d);
    assert_eq!(iso.max_region_size(0x27f82b, 0x3d86fa9c), 0x3d86fa9d);

    assert_eq!(iso.max_region_size(0x280000, 0x0), 0x3d86f2c8);
    assert_eq!(iso.max_region_size(0x280000, 0x100), 0x3d86f2c8);
    assert_eq!(iso.max_region_size(0x280000, 0x3d86f2c7), 0x3d86f2c8);
    assert_eq!(iso.max_region_size(0x280000, 0x3d86f2c8), 0x3d86f2c8);
    Ok(())
}

#[test]
#[serial]
fn test_disc_allocate() -> Result<()> {
    common::init_logging();

    let iso = common::open_iso()?;

    assert_eq!(iso.allocate(0x46b288b, 0x8000)?, DiscRegion::new(0x280000, 0x3d86f2c8));
    assert_eq!(iso.allocate(0x3d86f2c8, 0x8000)?, DiscRegion::new(0x280000, 0x3d86f2c8));
    assert!(matches!(
        iso.allocate(0x3d86f2c9, 0x8000),
        Err(disc::Error::NotEnoughSpace(0x3d86f2c9))
    ));
    Ok(())
}
