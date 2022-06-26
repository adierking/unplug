use crate::private::Sealed;
use crate::Resource;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::fmt::{self, Debug};

/// Metadata describing a sample file used in a sound effect.
struct Metadata {
    /// The sample's name.
    name: &'static str,
}

/// Macro used in the generated sample list.
macro_rules! declare_sfx_samples {
    {
        $($index:literal => $id:ident { $name:literal }),*
        $(,)*
    } => {
        /// A sound effect sample ID.
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(IntoPrimitive, TryFromPrimitive)]
        #[repr(u32)]
        pub enum SfxSample {
            $($id = $index),*
        }

        const METADATA: &[Metadata] = &[
            $(
                Metadata {
                    name: $name,
                }
            ),*
        ];
    }
}

impl SfxSample {
    /// Tries to find the sample whose name matches `name`.
    pub fn find(name: &str) -> Option<Self> {
        Self::iter().find(|s| s.name() == name)
    }

    /// Returns the sample's name.
    pub fn name(self) -> &'static str {
        self.meta().name
    }

    fn meta(self) -> &'static Metadata {
        &METADATA[u32::from(self) as usize]
    }
}

impl Sealed for SfxSample {}

impl Resource for SfxSample {
    type Value = u32;
    const COUNT: usize = METADATA.len();

    fn at(index: u32) -> Self {
        Self::try_from(index).unwrap()
    }
}

impl Debug for SfxSample {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{}>", self.name())
    }
}

// Generated using unplug-datagen
include!("gen/sfx_samples.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_sample() {
        let sample = SfxSample::VoiceHelpMe;
        assert_eq!(sample.name(), "voice_help_me");
        assert_eq!(format!("{:?}", sample), "<voice_help_me>");
    }

    #[test]
    fn test_find() {
        assert_eq!(SfxSample::find("voice_help_me"), Some(SfxSample::VoiceHelpMe));
        assert_eq!(SfxSample::find("foo"), None);
    }

    #[test]
    fn test_iter() {
        let samples = SfxSample::iter().collect::<Vec<_>>();
        assert_eq!(samples.len(), 1112);
        assert_eq!(samples[0], SfxSample::RoboMotor);
        assert_eq!(samples[1], SfxSample::RoboDown);
        assert_eq!(samples[1110], SfxSample::EndingHayashitate);
        assert_eq!(samples[1111], SfxSample::EndingShort);
    }
}
