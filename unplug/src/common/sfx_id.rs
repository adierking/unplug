use super::{ReadFrom, WriteTo};
use crate::data::music::{Music, MusicDefinition};
use crate::data::sfx::{Sfx, SfxDefinition};
use byteorder::{ReadBytesExt, WriteBytesExt, LE};
use std::convert::TryFrom;
use std::io::{self, Read, Write};
use thiserror::Error;

/// The special group value (hiword) corresponding to a music ID.
const MUSIC_GROUP: u32 = 0xffff;

pub type Result<T> = std::result::Result<T, Error>;

/// The error type for `SfxId` operations.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("invalid SFX id: 0x{0:>08x}")]
    InvalidId(u32),

    #[error(transparent)]
    Io(Box<io::Error>),
}

from_error_boxed!(Error::Io, io::Error);

/// An ID pointing to either a sound effect or music track. Internally the game represents these as
/// 32-bit values with the group in the hiword and the index in the loword.
#[allow(variant_size_differences)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SfxId {
    Sound(Sfx),
    Music(Music),
}

impl SfxId {
    /// Gets the name of the corresponding audio file without the extension.
    pub fn name(&self) -> &'static str {
        match *self {
            Self::Sound(sound) => SfxDefinition::get(sound).name,
            Self::Music(music) => MusicDefinition::get(music).name,
        }
    }

    /// Gets the 32-bit ID value.
    pub fn value(&self) -> u32 {
        match *self {
            SfxId::Sound(sound) => u32::from(sound),
            SfxId::Music(music) => (MUSIC_GROUP << 16) | (u8::from(music) as u32),
        }
    }
}

impl From<Sfx> for SfxId {
    fn from(sound: Sfx) -> Self {
        Self::Sound(sound)
    }
}

impl From<Music> for SfxId {
    fn from(music: Music) -> Self {
        Self::Music(music)
    }
}

impl From<SfxId> for u32 {
    fn from(id: SfxId) -> Self {
        id.value()
    }
}

impl TryFrom<u32> for SfxId {
    type Error = Error;
    fn try_from(id: u32) -> Result<Self> {
        let group = id >> 16;
        let index = id & 0xffff;
        if group == MUSIC_GROUP {
            match Music::try_from(index as u8) {
                Ok(music) => Ok(Self::Music(music)),
                Err(_) => Err(Error::InvalidId(id)),
            }
        } else {
            match Sfx::try_from(id) {
                Ok(sound) => Ok(Self::Sound(sound)),
                Err(_) => Err(Error::InvalidId(id)),
            }
        }
    }
}

impl Default for SfxId {
    fn default() -> Self {
        Self::Sound(Sfx::None)
    }
}

impl<R: Read + ?Sized> ReadFrom<R> for SfxId {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Self::try_from(reader.read_u32::<LE>()?)
    }
}

impl<W: Write + ?Sized> WriteTo<W> for SfxId {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_u32::<LE>(u32::from(*self))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_write_and_read;

    #[test]
    fn test_into_u32() {
        assert_eq!(u32::from(SfxId::Sound(Sfx::KitchenOil)), 0x00040015);
        assert_eq!(u32::from(SfxId::Music(Music::BgmNight)), 0xffff0010);
        assert_eq!(u32::from(SfxId::default()), 0);
    }

    #[test]
    fn test_try_from_u32() {
        let id = SfxId::try_from(0x00040015).unwrap();
        assert_eq!(id, SfxId::Sound(Sfx::KitchenOil));
        assert_eq!(id.name(), "kitchen_oil");

        let id = SfxId::try_from(0xffff0010).unwrap();
        assert_eq!(id, SfxId::Music(Music::BgmNight));
        assert_eq!(id.name(), "bgm_night");

        assert!(SfxId::try_from(0x00040028).is_err());
        assert!(SfxId::try_from(0xfffe0000).is_err());
        assert!(SfxId::try_from(0xffff006e).is_err());
    }

    #[test]
    fn test_write_and_read_sfx_id() {
        assert_write_and_read!(SfxId::Sound(Sfx::KitchenOil));
        assert_write_and_read!(SfxId::Music(Music::BgmNight));
    }
}
