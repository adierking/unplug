use crate::private::Sealed;
use crate::Resource;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use phf::phf_map;
use std::fmt::{self, Debug};
use unicase::UniCase;

const MUSIC_DIR: &str = "qp/streaming";
const MUSIC_EXT: &str = ".hps";

/// Metadata describing a music file.
struct Metadata {
    /// The music's volume (0-255).
    volume: u8,
    /// The name of the music file without the directory or file extension.
    name: &'static str,
}

/// Expands a filename into an `Option<&'static str>`.
macro_rules! __impl_object_id {
    (None) => {
        None
    };
    ($object:ident) => {
        Some(Object::$object)
    };
}

/// Macro used in the generated music list.
macro_rules! declare_music {
    {
        $($index:literal => $id:ident { $volume:literal, $name:tt }),*
        $(,)*
    } => {
        /// A music ID.
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(IntoPrimitive, TryFromPrimitive)]
        #[repr(u8)]
        pub enum Music {
            $($id = $index),*
        }

        const METADATA: &[Metadata] = &[
            $(
                Metadata {
                    volume: $volume,
                    name: $name,
                }
            ),*
        ];

        static LOOKUP: phf::Map<UniCase<&'static str>, Music> = phf_map! {
            $(UniCase::ascii($name) => Music::$id),*
        };
    }
}

impl Music {
    /// Returns the music's playback volume (0-255).
    #[inline]
    pub fn volume(self) -> u8 {
        self.meta().volume
    }

    /// Returns the name of the music file on disc if the music is not `None`.
    pub fn file_name(self) -> Option<String> {
        match self {
            Music::None => None,
            _ => Some(format!("{}{}", self.meta().name, MUSIC_EXT)),
        }
    }

    /// Returns the path to the music file on disc if the music is not `None`.
    pub fn disc_path(self) -> Option<String> {
        match self {
            Music::None => None,
            _ => Some(format!("{}/{}{}", MUSIC_DIR, self.meta().name, MUSIC_EXT)),
        }
    }

    #[inline]
    fn meta(self) -> &'static Metadata {
        &METADATA[u8::from(self) as usize]
    }
}

impl Sealed for Music {}

impl Resource for Music {
    type Value = u8;
    const COUNT: usize = METADATA.len();

    #[inline]
    fn at(index: u8) -> Self {
        Self::try_from(index).unwrap()
    }

    #[inline]
    fn name(self) -> &'static str {
        self.meta().name
    }

    #[inline]
    fn is_none(self) -> bool {
        self == Music::None
    }

    fn find(name: impl AsRef<str>) -> Option<Self> {
        LOOKUP.get(&UniCase::ascii(name.as_ref())).copied()
    }
}

impl Debug for Music {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{}>", self.name())
    }
}

// Generated using unplug-datagen
include!("gen/music.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_music() {
        let music = Music::BgmNight;
        assert!(music.is_some());
        assert_eq!(music.name(), "bgm_night");
        assert_eq!(music.volume(), 180);
        assert_eq!(music.file_name().as_deref(), Some("bgm_night.hps"));
        assert_eq!(music.disc_path().as_deref(), Some("qp/streaming/bgm_night.hps"));
        assert_eq!(format!("{:?}", music), "<bgm_night>");
    }

    #[test]
    fn test_get_none() {
        let music = Music::None;
        assert!(music.is_none());
        assert_eq!(music.name(), "none");
        assert_eq!(music.volume(), 255);
        assert_eq!(music.file_name(), None);
        assert_eq!(music.disc_path(), None);
    }

    #[test]
    fn test_find() {
        assert_eq!(Music::find("teriyaki"), Some(Music::Teriyaki));
        assert_eq!(Music::find("TeRiYaKi"), Some(Music::Teriyaki));
        assert_eq!(Music::find("none"), Some(Music::None));
        assert_eq!(Music::find("foo"), None);
    }

    #[test]
    fn test_iter() {
        let music = Music::iter().collect::<Vec<_>>();
        assert_eq!(music.len(), 109);
        assert_eq!(music[0], Music::None);
        assert_eq!(music[1], Music::Sample);
        assert_eq!(music[107], Music::Living);
        assert_eq!(music[108], Music::Entrance);
    }
}
