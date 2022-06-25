use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::fmt::{self, Debug};

const MUSIC_DIR: &str = "qp/streaming";
const MUSIC_EXT: &str = ".hps";

/// Metadata describing a music file.
#[derive(Debug)]
pub struct MusicDefinition {
    /// The music's corresponding `Music`.
    pub id: Music,
    /// The music's volume (0-255).
    pub volume: u8,
    /// The name of the music file without the directory or file extension.
    pub name: &'static str,
}

impl MusicDefinition {
    /// Retrieves the definition corresponding to a `Music`.
    pub fn get(id: Music) -> &'static Self {
        &MUSIC[u8::from(id) as usize]
    }

    /// Tries to find the music whose name matches `name`.
    pub fn find(name: &str) -> Option<&'static Self> {
        // skip(1) to ignore None
        MUSIC.iter().skip(1).find(|a| a.name == name)
    }

    /// Gets the name of the music file within the ISO. Returns `None` for `Music::None`.
    pub fn file_name(&self) -> Option<String> {
        match self.id {
            Music::None => None,
            _ => Some(format!("{}{}", self.name, MUSIC_EXT)),
        }
    }

    /// Gets the path to the music file within the ISO. Returns `None` for `Music::None`.
    pub fn path(&self) -> Option<String> {
        match self.id {
            Music::None => None,
            _ => Some(format!("{}/{}{}", MUSIC_DIR, self.name, MUSIC_EXT)),
        }
    }
}

/// Macro used in the generated music list.
macro_rules! declare_music {
    {
        $($index:literal => $id:ident { $volume:literal, $name:literal }),*
        $(,)*
    } => {
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(IntoPrimitive, TryFromPrimitive)]
        #[repr(u8)]
        pub enum Music {
            $($id = $index),*
        }

        pub static MUSIC: &[MusicDefinition] = &[
            $(
                MusicDefinition {
                    id: Music::$id,
                    volume: $volume,
                    name: $name,
                }
            ),*
        ];
    }
}

impl Debug for Music {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{}>", MusicDefinition::get(*self).name)
    }
}

// Generated using unplug-datagen
include!("gen/music.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_music() {
        let music = MusicDefinition::get(Music::BgmNight);
        assert_eq!(music.id, Music::BgmNight);
        assert_eq!(music.volume, 180);
        assert_eq!(music.name, "bgm_night");
        assert_eq!(music.file_name().as_deref(), Some("bgm_night.hps"));
        assert_eq!(music.path().as_deref(), Some("qp/streaming/bgm_night.hps"));
    }

    #[test]
    fn test_get_none() {
        let music = MusicDefinition::get(Music::None);
        assert_eq!(music.id, Music::None);
        assert_eq!(music.volume, 255);
        assert_eq!(music.name, "none");
        assert_eq!(music.file_name(), None);
        assert_eq!(music.path(), None);
    }

    #[test]
    fn test_find() {
        assert_eq!(MusicDefinition::find("teriyaki").unwrap().id, Music::Teriyaki);
        assert!(MusicDefinition::find("foo").is_none());
        assert!(MusicDefinition::find("none").is_none());
    }
}
