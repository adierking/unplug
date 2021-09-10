use num_enum::{IntoPrimitive, TryFromPrimitive};

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
        #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

// Generated using unplug-datagen
include!("gen/sounds.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_sound() {
        let sound = SoundDefinition::get(Sound::VoiceHelpMe);
        assert_eq!(sound.id, Sound::VoiceHelpMe);
        assert_eq!(sound.name, "VOICE_HELP_ME");
    }
}
