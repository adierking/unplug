use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::fmt::{self, Debug};

/// Metadata describing a sample file used in a sound effect.
#[derive(Debug)]
pub struct SfxSampleDefinition {
    /// The sample's corresponding `SfxSample`.
    pub id: SfxSample,
    /// The sample's name.
    pub name: &'static str,
}

impl SfxSampleDefinition {
    /// Retrieves the definition corresponding to a `SfxSample`.
    pub fn get(id: SfxSample) -> &'static SfxSampleDefinition {
        &SFX_SAMPLES[u32::from(id) as usize]
    }
}

/// Macro used in the generated sample list.
macro_rules! declare_sfx_samples {
    {
        $($index:literal => $id:ident { $name:literal }),*
        $(,)*
    } => {
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(IntoPrimitive, TryFromPrimitive)]
        #[repr(u32)]
        pub enum SfxSample {
            $($id = $index),*
        }

        pub static SFX_SAMPLES: &[SfxSampleDefinition] = &[
            $(
                SfxSampleDefinition {
                    id: SfxSample::$id,
                    name: $name,
                }
            ),*
        ];
    }
}

impl Debug for SfxSample {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{}>", SfxSampleDefinition::get(*self).name)
    }
}

// Generated using unplug-datagen
include!("gen/sfx_samples.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_sample() {
        let sample = SfxSampleDefinition::get(SfxSample::VoiceHelpMe);
        assert_eq!(sample.id, SfxSample::VoiceHelpMe);
        assert_eq!(sample.name, "voice_help_me");
    }
}
