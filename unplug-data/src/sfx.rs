use crate::private::Sealed;
use crate::{Resource, SfxGroup};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use phf::phf_map;
use std::fmt::{self, Debug};
use unicase::UniCase;

/// Metadata describing a sound effect.
struct Metadata {
    /// The corresponding sound effect ID.
    id: Sfx,
    /// The effect's name.
    name: &'static str,
}

/// Macro used in the generated SFX list.
macro_rules! declare_sfx {
    {
        $($index:literal => $id:ident { $name:tt }),*
        $(,)*
    } => {
        /// A sound effect ID.
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(IntoPrimitive, TryFromPrimitive)]
        #[repr(u32)]
        pub enum Sfx {
            $($id = $index),*
        }

        const METADATA: &[Metadata] = &[
            $(
                Metadata {
                    id: Sfx::$id,
                    name: $name,
                }
            ),*
        ];

        static LOOKUP: phf::Map<UniCase<&'static str>, Sfx> = phf_map! {
            $(UniCase::ascii($name) => Sfx::$id),*
        };
    }
}

impl Sfx {
    /// Path to the playlist file on the disc.
    pub const DISC_PLAYLIST_PATH: &'static str = "qp/sfx_sample.sem";

    /// Returns the sound effect's group ID.
    pub fn group(self) -> SfxGroup {
        let id = u32::from(self);
        SfxGroup::try_from((id >> 16) as u16).unwrap()
    }

    /// Returns the sound effect's material index within the playlist.
    pub fn material_index(self) -> usize {
        let id = u32::from(self);
        (self.group().first_material() + (id & 0xffff)) as usize
    }

    #[inline]
    fn meta(self) -> &'static Metadata {
        &METADATA[self.material_index()]
    }
}

impl Sealed for Sfx {}

impl Resource for Sfx {
    type Value = u32;
    const COUNT: usize = METADATA.len();

    #[inline]
    fn at(index: u32) -> Self {
        METADATA[index as usize].id
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

impl Debug for Sfx {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{}>", self.name())
    }
}

// Generated using unplug-datagen
include!("gen/sfx.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get() {
        let sfx = Sfx::KitchenOil;
        assert_eq!(sfx.name(), "kitchen_oil");
        assert_eq!(sfx.group(), SfxGroup::Kitchen);
        assert_eq!(sfx.material_index(), 582);
        assert_eq!(format!("{:?}", sfx), "<kitchen_oil>");
    }

    #[test]
    fn test_material_index() {
        for (i, meta) in METADATA.iter().enumerate() {
            assert_eq!(meta.id.material_index(), i);
        }
    }

    #[test]
    fn test_find() {
        assert_eq!(Sfx::find("kitchen_oil"), Some(Sfx::KitchenOil));
        assert_eq!(Sfx::find("KiTcHeN_oIl"), Some(Sfx::KitchenOil));
        assert_eq!(Sfx::find("none"), Some(Sfx::None));
        assert_eq!(Sfx::find("foo"), None);
    }

    #[test]
    fn test_iter() {
        let sfx = Sfx::iter().collect::<Vec<_>>();
        assert_eq!(sfx.len(), 1120);
        assert_eq!(sfx[0], Sfx::None);
        assert_eq!(sfx[1], Sfx::RoboDown);
        assert_eq!(sfx[1118], Sfx::EndingHayashitate);
        assert_eq!(sfx[1119], Sfx::EndingShort);
    }
}
