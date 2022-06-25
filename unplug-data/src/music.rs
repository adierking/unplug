use crate::private::Sealed;
use crate::resource::{Resource, ResourceIterator};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::fmt::{self, Debug};

/// The total number of music IDs.
pub const NUM_MUSIC: usize = 109;

const MUSIC_DIR: &str = "qp/streaming";
const MUSIC_EXT: &str = ".hps";

/// Metadata describing a music file.
struct Metadata {
    /// The music's volume (0-255).
    volume: u8,
    /// The name of the music file without the directory or file extension.
    name: &'static str,
}

/// Macro used in the generated music list.
macro_rules! declare_music {
    {
        $($index:literal => $id:ident { $volume:literal, $name:literal }),*
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
    }
}

impl Music {
    /// Returns an iterator over all music IDs.
    pub fn iter() -> ResourceIterator<Self> {
        ResourceIterator::new()
    }

    /// Tries to find the music whose name matches `name`.
    pub fn find(name: &str) -> Option<Self> {
        // skip(1) to ignore None
        Self::iter().skip(1).find(|m| m.name() == name)
    }

    /// Returns a unique name for the music assigned by unplug-datagen.
    pub fn name(self) -> &'static str {
        self.meta().name
    }

    /// Returns the music's playback volume (0-255).
    pub fn volume(self) -> u8 {
        self.meta().volume
    }

    /// Gets the name of the music file within the ISO. Returns `None` for `Music::None`.
    pub fn file_name(self) -> Option<String> {
        match self {
            Music::None => None,
            _ => Some(format!("{}{}", self.name(), MUSIC_EXT)),
        }
    }

    /// Gets the path to the music file within the ISO. Returns `None` for `Music::None`.
    pub fn path(self) -> Option<String> {
        match self {
            Music::None => None,
            _ => Some(format!("{}/{}{}", MUSIC_DIR, self.name(), MUSIC_EXT)),
        }
    }

    fn meta(self) -> &'static Metadata {
        &METADATA[u8::from(self) as usize]
    }
}

impl Sealed for Music {}

impl Resource for Music {
    const COUNT: usize = NUM_MUSIC;
    fn at(index: usize) -> Self {
        Self::try_from(index as u8).unwrap()
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
        assert_eq!(music.name(), "bgm_night");
        assert_eq!(music.volume(), 180);
        assert_eq!(music.file_name().as_deref(), Some("bgm_night.hps"));
        assert_eq!(music.path().as_deref(), Some("qp/streaming/bgm_night.hps"));
        assert_eq!(format!("{:?}", music), "<bgm_night>");
    }

    #[test]
    fn test_get_none() {
        let music = Music::None;
        assert_eq!(music.name(), "none");
        assert_eq!(music.volume(), 255);
        assert_eq!(music.file_name(), None);
        assert_eq!(music.path(), None);
    }

    #[test]
    fn test_find() {
        assert_eq!(Music::find("teriyaki"), Some(Music::Teriyaki));
        assert_eq!(Music::find("foo"), None);
        assert_eq!(Music::find("none"), None);
    }

    #[test]
    fn test_iter() {
        let music = Music::iter().collect::<Vec<_>>();
        assert_eq!(music.len(), NUM_MUSIC);
        assert_eq!(music[0], Music::None);
        assert_eq!(music[1], Music::Sample);
        assert_eq!(music[107], Music::Living);
        assert_eq!(music[108], Music::Entrance);
    }
}
