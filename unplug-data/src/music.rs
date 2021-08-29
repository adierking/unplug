use num_enum::{IntoPrimitive, TryFromPrimitive};

/// Metadata describing a music file.
#[derive(Debug)]
pub struct MusicDefinition {
    /// The music's corresponding `Music`.
    pub id: Music,
    /// The music's volume (0-255).
    pub volume: u8,
    /// The path to the music file within the ISO.
    pub path: &'static str,
}

impl MusicDefinition {
    /// Retrieves the definition corresponding to a `Music`.
    pub fn get(id: Music) -> &'static MusicDefinition {
        &MUSIC[u8::from(id) as usize - 1]
    }
}

/// Macro used in the generated music list.
macro_rules! declare_music {
    {
        $($index:literal => $id:ident { $volume:literal, $path:literal }),*
        $(,)*
    } => {
        #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
                    path: $path,
                }
            ),*
        ];
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
        assert_eq!(music.path, "qp/streaming/bgm_night.hps");
    }
}
