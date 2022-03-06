use crate::data::music::{Music, MusicDefinition};
use crate::data::sfx::{Sfx, SfxDefinition};
use std::convert::TryFrom;

/// The special group value (hiword) corresponding to a music ID.
const MUSIC_GROUP: u32 = 0xffff;

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
        match id {
            SfxId::Sound(sound) => u32::from(sound),
            SfxId::Music(music) => (MUSIC_GROUP << 16) | (u8::from(music) as u32),
        }
    }
}

impl TryFrom<u32> for SfxId {
    type Error = u32;
    fn try_from(id: u32) -> Result<Self, Self::Error> {
        let group = id >> 16;
        let index = id & 0xffff;
        if group == MUSIC_GROUP {
            match Music::try_from(index as u8) {
                Ok(music) => Ok(Self::Music(music)),
                Err(_) => Err(id),
            }
        } else {
            match Sfx::try_from(id) {
                Ok(sound) => Ok(Self::Sound(sound)),
                Err(_) => Err(id),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_into_u32() {
        assert_eq!(u32::from(SfxId::Sound(Sfx::KitchenOil)), 0x00040015);
        assert_eq!(u32::from(SfxId::Music(Music::BgmNight)), 0xffff0010);
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
}
