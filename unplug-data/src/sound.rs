use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::fmt::{self, Debug};

/// Metadata describing a sound.
#[derive(Debug)]
pub struct SoundDefinition {
    /// The sound's corresponding `Sound`.
    pub id: Sound,
    /// The sound's name.
    pub name: &'static str,
}

impl SoundDefinition {
    /// Retrieves the definition corresponding to a `Sound`.
    pub fn get(id: Sound) -> &'static SoundDefinition {
        &SOUNDS[u32::from(id) as usize]
    }
}

/// Macro used in the generated sound list.
macro_rules! declare_sounds {
    {
        $($index:literal => $id:ident { $name:literal }),*
        $(,)*
    } => {
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(IntoPrimitive, TryFromPrimitive)]
        #[repr(u32)]
        pub enum Sound {
            $($id = $index),*
        }

        pub static SOUNDS: &[SoundDefinition] = &[
            $(
                SoundDefinition {
                    id: Sound::$id,
                    name: $name,
                }
            ),*
        ];
    }
}

impl Debug for Sound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{}>", SoundDefinition::get(*self).name)
    }
}

// Generated using unplug-datagen
include!("gen/sounds.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_sound() {
        let sound = SoundDefinition::get(Sound::VoiceHelpMe);
        assert_eq!(sound.id, Sound::VoiceHelpMe);
        assert_eq!(sound.name, "voice_help_me");
    }
}
