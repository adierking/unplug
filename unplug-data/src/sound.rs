use super::{Error, Music, Resource, Result, Sfx};

/// The special group value (hiword) corresponding to a music ID.
const MUSIC_GROUP: u32 = 0xffff;

/// A sound ID which refers to either a sound effect or music track. Internally the game represents
/// these as 32-bit values with the group in the hiword and the index in the loword.
#[allow(variant_size_differences)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Sound {
    None,
    Sfx(Sfx),
    Music(Music),
}

impl Sound {
    /// Retrieves a unique name for the sound. For `Sound::None`, this will return an empty string
    /// (because "none" is a valid sound effect!).
    pub fn name(&self) -> &'static str {
        match *self {
            Self::None => "",
            Self::Sfx(sfx) => sfx.name(),
            Self::Music(music) => music.name(),
        }
    }

    /// Tries to find the music or sound effect whose name matches `name`. Sound effects will be
    /// searched first before music.
    pub fn find(name: &str) -> Option<Sound> {
        Sfx::find(name).map(Sound::Sfx).or_else(|| Music::find(name).map(Sound::Music))
    }

    /// Gets the 32-bit ID value.
    pub fn value(&self) -> u32 {
        match *self {
            Sound::None => u32::MAX,
            Sound::Sfx(sound) => u32::from(sound),
            Sound::Music(music) => (MUSIC_GROUP << 16) | (u8::from(music) as u32),
        }
    }
}

impl From<Sfx> for Sound {
    fn from(sound: Sfx) -> Self {
        Self::Sfx(sound)
    }
}

impl From<Music> for Sound {
    fn from(music: Music) -> Self {
        Self::Music(music)
    }
}

impl From<Sound> for u32 {
    fn from(id: Sound) -> Self {
        id.value()
    }
}

impl TryFrom<u32> for Sound {
    type Error = Error;
    fn try_from(id: u32) -> Result<Self> {
        if id == u32::MAX {
            return Ok(Self::None);
        }
        let group = id >> 16;
        let index = id & 0xffff;
        if group == MUSIC_GROUP {
            match Music::try_from(index as u8) {
                Ok(music) => Ok(Self::Music(music)),
                Err(_) => Err(Error::InvalidSoundId(id)),
            }
        } else {
            match Sfx::try_from(id) {
                Ok(sound) => Ok(Self::Sfx(sound)),
                Err(_) => Err(Error::InvalidSoundId(id)),
            }
        }
    }
}

impl Default for Sound {
    fn default() -> Self {
        Self::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find() {
        assert_eq!(Sound::find("teriyaki"), Some(Sound::Music(Music::Teriyaki)));
        assert_eq!(Sound::find("TeRiYaKi"), Some(Sound::Music(Music::Teriyaki)));
        assert_eq!(Sound::find("kitchen_oil"), Some(Sound::Sfx(Sfx::KitchenOil)));
        assert_eq!(Sound::find("KiTcHeN_oIl"), Some(Sound::Sfx(Sfx::KitchenOil)));
        assert_eq!(Sound::find("none"), Some(Sound::Sfx(Sfx::None)));
        assert_eq!(Sound::find("foo"), None);
        assert_eq!(Sound::find(""), None);
    }

    #[test]
    fn test_into_u32() {
        assert_eq!(u32::from(Sound::None), 0xffffffff);
        assert_eq!(u32::from(Sound::Sfx(Sfx::KitchenOil)), 0x00040015);
        assert_eq!(u32::from(Sound::Music(Music::BgmNight)), 0xffff0010);
    }

    #[test]
    fn test_try_from_u32() {
        assert_eq!(Sound::try_from(0xffffffff).unwrap(), Sound::None);

        let id = Sound::try_from(0x00040015).unwrap();
        assert_eq!(id, Sound::Sfx(Sfx::KitchenOil));
        assert_eq!(id.name(), "kitchen_oil");

        let id = Sound::try_from(0xffff0010).unwrap();
        assert_eq!(id, Sound::Music(Music::BgmNight));
        assert_eq!(id.name(), "bgm_night");

        assert!(Sound::try_from(0x00040028).is_err());
        assert!(Sound::try_from(0xfffe0000).is_err());
        assert!(Sound::try_from(0xffff006e).is_err());
    }
}
