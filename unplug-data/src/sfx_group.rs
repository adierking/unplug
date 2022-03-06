use num_enum::{IntoPrimitive, TryFromPrimitive};

const BANK_DIR: &str = "qp";
const BANK_EXT: &str = ".ssm";

/// Metadata describing a sound effect group.
#[derive(Debug)]
pub struct SfxGroupDefinition {
    /// The group's corresponding `SfxGroup`.
    pub id: SfxGroup,
    /// The index of the group's first sample file within the sample banks.
    pub first_sample: u32,
    /// The index of the group's first sound material within the SFX playlist.
    pub first_material: u32,
    /// The name of the sound bank without the directory or file extension.
    pub name: &'static str,
}

impl SfxGroupDefinition {
    /// Retrieves the definition corresponding to a `SfxGroup`.
    pub fn get(id: SfxGroup) -> &'static SfxGroupDefinition {
        &SFX_GROUPS[u16::from(id) as usize]
    }

    /// Gets the path to group's corresponding bank file within the ISO.
    pub fn bank_path(&self) -> String {
        format!("{}/{}{}", BANK_DIR, self.name, BANK_EXT)
    }
}

/// Macro used in the generated sound bank list.
macro_rules! declare_sfx_groups {
    {
        $($index:literal => $id:ident { $sbase:literal, $pbase:literal, $name:literal }),*
        $(,)*
    } => {
        #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(IntoPrimitive, TryFromPrimitive)]
        #[repr(u16)]
        pub enum SfxGroup {
            $($id = $index),*
        }

        pub static SFX_GROUPS: &[SfxGroupDefinition] = &[
            $(
                SfxGroupDefinition {
                    id: SfxGroup::$id,
                    first_sample: $sbase,
                    first_material: $pbase,
                    name: $name,
                }
            ),*
        ];
    }
}

// Generated using unplug-datagen
include!("gen/sfx_groups.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_sound_group() {
        let group = SfxGroupDefinition::get(SfxGroup::Ufo);
        assert_eq!(group.id, SfxGroup::Ufo);
        assert_eq!(group.first_sample, 0x2f9);
        assert_eq!(group.first_material, 0x2fe);
        assert_eq!(group.name, "sfx_ufo");
        assert_eq!(group.bank_path(), "qp/sfx_ufo.ssm");
    }
}
