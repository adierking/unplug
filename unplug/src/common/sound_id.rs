use crate::data::{Music, SoundEvent};
use std::convert::TryFrom;

/// The special bank value (hiword) corresponding to a music ID.
const MUSIC_BANK: u32 = 0xffff;

/// An ID pointing to either a sound event or music. Internally the game represents these as 32-bit
/// values with the bank in the hiword and the index in the loword.
#[allow(variant_size_differences)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SoundId {
    Sound(SoundEvent),
    Music(Music),
}

impl From<SoundEvent> for SoundId {
    fn from(sound: SoundEvent) -> Self {
        Self::Sound(sound)
    }
}

impl From<Music> for SoundId {
    fn from(music: Music) -> Self {
        Self::Music(music)
    }
}

impl From<SoundId> for u32 {
    fn from(id: SoundId) -> Self {
        match id {
            SoundId::Sound(sound) => u32::from(sound),
            SoundId::Music(music) => (MUSIC_BANK << 16) | (u8::from(music) as u32),
        }
    }
}

impl TryFrom<u32> for SoundId {
    type Error = u32;
    fn try_from(id: u32) -> Result<Self, Self::Error> {
        let bank = id >> 16;
        let index = id & 0xffff;
        if bank == MUSIC_BANK {
            match Music::try_from(index as u8) {
                Ok(music) => Ok(Self::Music(music)),
                Err(_) => Err(id),
            }
        } else {
            match SoundEvent::try_from(id) {
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
        assert_eq!(u32::from(SoundId::Sound(SoundEvent::KitchenOil)), 0x00040015);
        assert_eq!(u32::from(SoundId::Music(Music::BgmNight)), 0xffff0010);
    }

    #[test]
    fn test_try_from_u32() {
        let id = SoundId::try_from(0x00040015).unwrap();
        assert_eq!(id, SoundId::Sound(SoundEvent::KitchenOil));

        let id = SoundId::try_from(0xffff0010).unwrap();
        assert_eq!(id, SoundId::Music(Music::BgmNight));

        assert!(SoundId::try_from(0x00040028).is_err());
        assert!(SoundId::try_from(0xfffe0000).is_err());
        assert!(SoundId::try_from(0xffff006e).is_err());
    }
}
