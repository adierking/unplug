use crate::private::Sealed;
use crate::{Object, ObjectFlags, RawObjectPlacement, Resource};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use phf::phf_map;
use std::fmt::{self, Debug, Formatter};
use unicase::UniCase;

// Metadata describing a spawnable.
struct Metadata {
    /// A unique name assigned by unplug-datagen.
    name: &'static str,
    /// The spawnable's placement template.
    template: RawObjectPlacement,
}

// Macro used in the generated spawnables list to handle const flag expressions
macro_rules! __impl_flags {
    ($($flag:ident)|*) => {
        __impl_flags!(@eval 0, $($flag)|*)
    };
    (@eval $base:expr, $flag:ident $(| $flags:ident)*) => {
        __impl_flags!(@eval ($base | ObjectFlags::$flag.bits()), $($flags)|*)
    };
    (@eval $base:expr,) => {
        ObjectFlags::from_bits_truncate($base)
    };
}

// Macro used in the generated spawnables list
macro_rules! declare_spawnables {
    {
        $($index:literal => $id:ident {
            $name:tt,
            $object:ident,
            ($x:literal, $y:literal, $z:literal),
            ($sx:literal, $sy:literal, $sz:literal),
            $variant:literal,
            $($flag:ident)|*
        }),*
        $(,)*
    } => {
        /// An object which can be spawned via the engine or the `born()` command.
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(IntoPrimitive, TryFromPrimitive)]
        #[repr(i32)]
        pub enum Spawnable {
            $($id = $index),*
        }

        const METADATA: &[Metadata] = &[
            $(
                Metadata {
                    name: $name,
                    template: RawObjectPlacement {
                        id: Object::$object,
                        x: $x,
                        y: $y,
                        z: $z,
                        rotate_x: 0,
                        rotate_y: 0,
                        rotate_z: 0,
                        scale_x: $sx,
                        scale_y: $sy,
                        scale_z: $sz,
                        data: 0,
                        spawn_flag: 0,
                        variant: $variant,
                        flags: __impl_flags!($($flag)|*),
                    },
                }
            ),*
        ];

        static LOOKUP: phf::Map<UniCase<&'static str>, Spawnable> = phf_map! {
            $(UniCase::ascii($name) => Spawnable::$id),*
        };
    };
}

impl Spawnable {
    /// Returns the spawnable's placement template.
    #[inline]
    pub fn template(self) -> &'static RawObjectPlacement {
        &self.meta().template
    }

    #[inline]
    fn meta(self) -> &'static Metadata {
        &METADATA[i32::from(self) as usize]
    }
}

impl Debug for Spawnable {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "<{}>", self.name())
    }
}

impl Sealed for Spawnable {}

impl Resource for Spawnable {
    type Value = i32;
    const COUNT: usize = METADATA.len();

    #[inline]
    fn at(index: i32) -> Self {
        Self::try_from(index).unwrap()
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

// Generated using unplug-datagen
include!("gen/spawnables.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get() {
        let spawnable = Spawnable::CbRobo;
        assert_eq!(spawnable.name(), "cb_robo");
        assert_eq!(
            spawnable.template().flags,
            ObjectFlags::SPAWN | ObjectFlags::UNLIT | ObjectFlags::BOTCAM | ObjectFlags::REFLECT
        );
        assert_eq!(format!("{:?}", spawnable), "<cb_robo>");
    }

    #[test]
    fn test_find() {
        assert_eq!(Spawnable::find("cb_robo"), Some(Spawnable::CbRobo));
        assert_eq!(Spawnable::find("Cb_RoBo"), Some(Spawnable::CbRobo));
        assert_eq!(Spawnable::find("foo"), None);
    }

    #[test]
    fn test_iter() {
        let spawnables = Spawnable::iter().collect::<Vec<_>>();
        assert_eq!(spawnables.len(), 47);
        assert_eq!(spawnables[0], Spawnable::TitleIconBb);
        assert_eq!(spawnables[1], Spawnable::TitleIconArmy);
        assert_eq!(spawnables[45], Spawnable::UsMaruModel);
        assert_eq!(spawnables[46], Spawnable::UsBatuModel);
    }
}
