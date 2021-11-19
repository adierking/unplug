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
    pub fn get(id: Music) -> &'static MusicDefinition {
        &MUSIC[u8::from(id) as usize - 1]
    }

    /// Gets the path to the music file within the ISO.
    pub fn path(&self) -> String {
        format!("{}/{}{}", MUSIC_DIR, self.name, MUSIC_EXT)
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
        assert_eq!(music.path(), "qp/streaming/bgm_night.hps");
    }
}
