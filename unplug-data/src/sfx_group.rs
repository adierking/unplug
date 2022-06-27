use crate::private::Sealed;
use crate::Resource;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use phf::phf_map;
use std::fmt::{self, Debug, Formatter};
use unicase::UniCase;

const BANK_DIR: &str = "qp";
const BANK_EXT: &str = ".ssm";

/// Metadata describing a sound effect group.
struct Metadata {
    /// The index of the group's first sample file within the sample banks.
    first_sample: u32,
    /// The index of the group's first sound material within the SFX playlist.
    first_material: u32,
    /// The name of the sound bank without the directory or file extension.
    name: &'static str,
}

/// Macro used in the generated sound bank list.
macro_rules! declare_sfx_groups {
    {
        $($index:literal => $id:ident { $sbase:literal, $pbase:literal, $name:tt }),*
        $(,)*
    } => {
        /// A sound effect group ID.
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(IntoPrimitive, TryFromPrimitive)]
        #[repr(u16)]
        pub enum SfxGroup {
            $($id = $index),*
        }

        const METADATA: &[Metadata] = &[
            $(
                Metadata {
                    first_sample: $sbase,
                    first_material: $pbase,
                    name: $name,
                }
            ),*
        ];

        static LOOKUP: phf::Map<UniCase<&'static str>, SfxGroup> = phf_map! {
            $(UniCase::ascii($name) => SfxGroup::$id),*
        };
    }
}

impl SfxGroup {
    /// Returns the index of the group's first sample file within the sample banks.
    #[inline]
    pub fn first_sample(self) -> u32 {
        self.meta().first_sample
    }

    /// Returns the index of the group's first sound material within the SFX playlist.
    #[inline]
    pub fn first_material(self) -> u32 {
        self.meta().first_material
    }

    /// Returns the name of the group's bank file on disc.
    pub fn file_name(&self) -> String {
        format!("{}{}", self.meta().name, BANK_EXT)
    }

    /// Returns the path to the group's bank file on disc.
    pub fn disc_path(&self) -> String {
        format!("{}/{}{}", BANK_DIR, self.meta().name, BANK_EXT)
    }

    #[inline]
    fn meta(self) -> &'static Metadata {
        &METADATA[u16::from(self) as usize]
    }
}

impl Sealed for SfxGroup {}

impl Resource for SfxGroup {
    type Value = u16;
    const COUNT: usize = METADATA.len();

    #[inline]
    fn at(index: u16) -> Self {
        SfxGroup::try_from(index).unwrap()
    }

    #[inline]
    fn name(self) -> &'static str {
        self.meta().name
    }

    #[inline]
    fn is_none(self) -> bool {
        false
    }

    fn find(name: impl AsRef<str>) -> Option<Self> {
        LOOKUP.get(&UniCase::ascii(name.as_ref())).copied()
    }
}

impl Debug for SfxGroup {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "<{}>", self.name())
    }
}

// Generated using unplug-datagen
include!("gen/sfx_groups.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_sound_group() {
        let group = SfxGroup::Ufo;
        assert_eq!(group.name(), "sfx_ufo");
        assert_eq!(group.first_sample(), 0x2f9);
        assert_eq!(group.first_material(), 0x2fe);
        assert_eq!(group.file_name(), "sfx_ufo.ssm");
        assert_eq!(group.disc_path(), "qp/sfx_ufo.ssm");
        assert_eq!(format!("{:?}", group), "<sfx_ufo>");
    }

    #[test]
    fn test_find() {
        assert_eq!(SfxGroup::find("sfx_ufo"), Some(SfxGroup::Ufo));
        assert_eq!(SfxGroup::find("SfX_uFo"), Some(SfxGroup::Ufo));
        assert_eq!(SfxGroup::find("foo"), None);
    }

    #[test]
    fn test_iter() {
        let groups = SfxGroup::iter().collect::<Vec<_>>();
        assert_eq!(groups.len(), 25);
        assert_eq!(groups[0], SfxGroup::Sample);
        assert_eq!(groups[1], SfxGroup::Stage07);
        assert_eq!(groups[23], SfxGroup::Concert);
        assert_eq!(groups[24], SfxGroup::Ending);
    }
}
