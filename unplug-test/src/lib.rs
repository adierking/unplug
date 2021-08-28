use anyhow::{bail, ensure, Result};
use log::info;
use simplelog::{Color, ColorChoice, ConfigBuilder, Level, LevelFilter, TermLogger, TerminalMode};
use std::env;
use std::fs::{self, File};
use std::io::{Read, Seek};
use std::sync::Once;
use tempfile::{NamedTempFile, TempPath};
// rust-analyzer doesn't like this import but it should be fine, just disable the diagnostic
use unplug::dvd::DiscStream;

pub static GAME_ID: &str = "GGTE01";

pub static QP_PATH: &str = "qp.bin";

pub static QP_GLOBALS_PATH: &str = "bin/e/globals.bin";

static INIT_LOGGING: Once = Once::new();

/// Configures logging at the beginning of a test.
pub fn init_logging() {
    INIT_LOGGING.call_once(|| {
        let config = ConfigBuilder::new()
            .set_time_format_str("%T%.3f")
            .set_level_color(Level::Info, Some(Color::Green))
            .build();
        TermLogger::init(LevelFilter::Debug, config, TerminalMode::Stderr, ColorChoice::Auto)
            .unwrap();
    });
}

/// Reads the `CHIBI_ISO` environment variable.
pub fn iso_path() -> Result<String> {
    match env::var("CHIBI_ISO") {
        Ok(path) => Ok(path),
        Err(_) => {
            bail!("The CHIBI_ISO environment variable must point to a {} ISO", GAME_ID);
        }
    }
}

/// Opens the ISO for reading.
pub fn open_iso() -> Result<DiscStream<File>> {
    let file = File::open(iso_path()?)?;
    let iso = DiscStream::open(file)?;
    check_iso(&iso)?;
    Ok(iso)
}

/// Checks that an ISO is GGTE01.
pub fn check_iso(iso: &DiscStream<impl Read + Seek>) -> Result<()> {
    let game_id = iso.game_id();
    ensure!(game_id == GAME_ID, "Unsupported game id: {}", game_id);
    Ok(())
}

/// Makes a temporary copy of the ISO and returns its path.
pub fn copy_iso() -> Result<TempPath> {
    let original_path = iso_path()?;
    let copy_path = NamedTempFile::new()?.into_temp_path();
    info!("Copying {} to {}", original_path, copy_path.to_str().unwrap());
    fs::copy(original_path, &copy_path)?;
    Ok(copy_path)
}

/// Compares the contents of two streams for equality.
pub fn compare_streams(mut a: impl Read, mut b: impl Read) -> Result<bool> {
    let mut a_buf = [0u8; 0x8000];
    let mut b_buf = [0u8; 0x8000];
    loop {
        let a_len = a.read(&mut a_buf)?;
        let b_len = b.read(&mut b_buf)?;
        if a_len != b_len {
            return Ok(false);
        }
        if a_len == 0 {
            return Ok(true);
        }
        if a_buf[..a_len] != b_buf[..a_len] {
            return Ok(false);
        }
    }
}
