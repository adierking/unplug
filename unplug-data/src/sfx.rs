use super::sfx_group::{SfxGroup, SfxGroupDefinition, SFX_GROUPS};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::fmt::{self, Debug};

/// ISO path to the playlist file.
pub const PLAYLIST_PATH: &str = "qp/sfx_sample.sem";

/// Metadata describing a sound effect.
#[derive(Debug)]
pub struct SfxDefinition {
    /// The effect's corresponding `Sfx`.
    pub id: Sfx,
    /// The effect's name.
    pub name: &'static str,
}

impl SfxDefinition {
    /// Retrieves the definition corresponding to a `Sfx`.
    pub fn get(id: Sfx) -> &'static SfxDefinition {
        &SFX[id.material_index()]
    }
}

/// Macro used in the generated SFX list.
macro_rules! declare_sfx {
    {
        $($index:literal => $id:ident { $name:literal }),*
        $(,)*
    } => {
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(IntoPrimitive, TryFromPrimitive)]
        #[repr(u32)]
        pub enum Sfx {
            $($id = $index),*
        }

        pub static SFX: &[SfxDefinition] = &[
            $(
                SfxDefinition {
                    id: Sfx::$id,
                    name: $name,
                }
            ),*
        ];
    }
}

impl Sfx {
    /// Returns the sound effect's `SfxGroup`.
    pub fn group(&self) -> SfxGroup {
        let id = u32::from(*self);
        SFX_GROUPS[(id >> 16) as usize].id
    }

    /// Returns the sound effect's material index within the playlist.
    pub fn material_index(&self) -> usize {
        let group = SfxGroupDefinition::get(self.group());
        let id = u32::from(*self);
        (group.first_material + (id & 0xffff)) as usize
    }
}

impl Debug for Sfx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{}>", SfxDefinition::get(*self).name)
    }
}

// Generated using unplug-datagen
include!("gen/sfx.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_sfx() {
        for sfx in SFX {
            let actual = SfxDefinition::get(sfx.id);
            assert_eq!(actual.id, sfx.id);
        }
    }

    #[test]
    fn test_get_sfx_group() {
        assert_eq!(Sfx::VoiceHelpMe.group(), SfxGroup::Sample);
        assert_eq!(Sfx::KitchenOil.group(), SfxGroup::Kitchen);
    }
}
