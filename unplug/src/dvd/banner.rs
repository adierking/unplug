use crate::common::text::{self, FixedText};
use crate::common::{ReadFrom, ReadStructExt, WriteStructExt, WriteTo};
use byteorder::{ByteOrder, ReadBytesExt, BE};
use std::fmt::{self, Debug, Formatter};
use std::io::{self, Read, Write};
use thiserror::Error;

const MAGIC_NTSC: [u8; 4] = *b"BNR1";
const MAGIC_PAL: [u8; 4] = *b"BNR2";

const SHORT_TEXT_SIZE: usize = 0x20;
const LONG_TEXT_SIZE: usize = 0x40;
const DESCRIPTION_SIZE: usize = 0x80;

pub const NUM_LANGUAGES_NTSC: usize = 1;
pub const NUM_LANGUAGES_PAL: usize = 6;

pub const IMAGE_WIDTH: usize = 96;
pub const IMAGE_HEIGHT: usize = 32;
pub const IMAGE_SIZE: usize = IMAGE_WIDTH * IMAGE_HEIGHT;

/// The result type for banner operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for banner operations.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("invalid banner magic")]
    InvalidMagic,

    #[error("invalid language count")]
    InvalidLanguageCount,

    #[error("invalid image size")]
    InvalidImageSize,

    #[error(transparent)]
    Io(Box<io::Error>),

    #[error(transparent)]
    Text(Box<text::Error>),
}

from_error_boxed!(Error::Io, io::Error);
from_error_boxed!(Error::Text, text::Error);

/// A GameCube opening.bnr file.
#[derive(Default, Clone, PartialEq, Eq)]
pub struct Banner {
    /// Image data (96x32 RGB5A3)
    pub image: Box<[u16]>,
    /// Game info for each language (1 in NTSC, 6 in PAL)
    pub languages: Vec<GameInfo>,
}

impl Banner {
    pub fn new() -> Self {
        Self::default()
    }
}

#[allow(clippy::missing_fields_in_debug)]
impl Debug for Banner {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Banner").field("languages", &self.languages).finish()
    }
}

impl<R: Read> ReadFrom<R> for Banner {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;
        let num_languages = match magic {
            MAGIC_NTSC => NUM_LANGUAGES_NTSC,
            MAGIC_PAL => NUM_LANGUAGES_PAL,
            _ => return Err(Error::InvalidMagic),
        };

        let mut padding = [0u8; 0x1c];
        reader.read_exact(&mut padding)?;

        let mut image = vec![0u16; IMAGE_SIZE];
        reader.read_u16_into::<BE>(&mut image)?;

        let mut languages = vec![GameInfo::default(); num_languages];
        GameInfo::read_all_from(reader, &mut languages)?;

        Ok(Self { image: image.into_boxed_slice(), languages })
    }
}

impl<W: Write> WriteTo<W> for Banner {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        let magic = match self.languages.len() {
            NUM_LANGUAGES_NTSC => MAGIC_NTSC,
            NUM_LANGUAGES_PAL => MAGIC_PAL,
            _ => return Err(Error::InvalidLanguageCount),
        };
        writer.write_all(&magic)?;

        let padding = [0u8; 0x1c];
        writer.write_all(&padding)?;

        if self.image.len() != IMAGE_SIZE {
            return Err(Error::InvalidImageSize);
        }
        let mut bytes = vec![0u8; IMAGE_SIZE * 2];
        BE::write_u16_into(&self.image, &mut bytes);
        writer.write_all(&bytes)?;

        GameInfo::write_all_to(writer, &self.languages)?;
        Ok(())
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct GameInfo {
    pub name_short: FixedText<SHORT_TEXT_SIZE>,
    pub maker_short: FixedText<SHORT_TEXT_SIZE>,
    pub name_long: FixedText<LONG_TEXT_SIZE>,
    pub maker_long: FixedText<LONG_TEXT_SIZE>,
    pub description: FixedText<DESCRIPTION_SIZE>,
}

impl<R: Read + ?Sized> ReadFrom<R> for GameInfo {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self {
            name_short: reader.read_struct()?,
            maker_short: reader.read_struct()?,
            name_long: reader.read_struct()?,
            maker_long: reader.read_struct()?,
            description: reader.read_struct()?,
        })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for GameInfo {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_struct(&self.name_short)?;
        writer.write_struct(&self.maker_short)?;
        writer.write_struct(&self.name_long)?;
        writer.write_struct(&self.maker_long)?;
        writer.write_struct(&self.description)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_write_and_read;
    use crate::common::Text;
    use crate::test::TEST_BANNER;
    use std::io::Cursor;

    fn game_info() -> GameInfo {
        GameInfo {
            name_short: Text::from_bytes("name_short").unwrap(),
            maker_short: Text::from_bytes("maker_short").unwrap(),
            name_long: Text::from_bytes("name_long").unwrap(),
            maker_long: Text::from_bytes("maker_long").unwrap(),
            description: Text::from_bytes("description").unwrap(),
        }
    }

    #[test]
    fn test_write_and_read_banner_ntsc() {
        assert_write_and_read!(Banner {
            image: vec![0u16; IMAGE_SIZE].into_boxed_slice(),
            languages: vec![game_info(); NUM_LANGUAGES_NTSC],
        });
    }

    #[test]
    fn test_write_and_read_banner_pal() {
        assert_write_and_read!(Banner {
            image: vec![0u16; IMAGE_SIZE].into_boxed_slice(),
            languages: vec![game_info(); NUM_LANGUAGES_PAL],
        });
    }

    #[test]
    fn test_parse_banner() -> Result<()> {
        let mut reader = Cursor::new(TEST_BANNER);
        let banner = reader.read_struct::<Banner>()?;
        assert!(banner.image.iter().all(|x| *x == 0xffff));
        assert_eq!(banner.languages.len(), 1);
        let info = &banner.languages[0];
        assert_eq!(info.name_short.decode().unwrap(), "Short Name");
        assert_eq!(info.maker_short.decode().unwrap(), "Short Maker");
        assert_eq!(info.name_long.decode().unwrap(), "Long Name");
        assert_eq!(info.maker_long.decode().unwrap(), "Long Maker");
        assert_eq!(info.description.decode().unwrap(), "Description line 1\nDescription line 2");
        Ok(())
    }
}
