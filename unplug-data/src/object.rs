use crate::private::Sealed;
use crate::Resource;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use phf::phf_map;
use std::fmt::{self, Debug, Formatter};
use unicase::UniCase;

/// Metadata describing an object.
struct Metadata {
    /// The corresponding object ID.
    id: Object,
    /// A unique name assigned by unplug-datagen.
    name: &'static str,
    /// The object's engine class.
    class: ObjectClass,
    /// A subclass value meaningful to the engine class.
    subclass: u16,
    /// The object's model path.
    path: &'static str,
}

/// Object engine classes.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ObjectClass {
    Camera,
    Light,
    Chr,
    Map,
    Actor2,
    Actor3,
    Sound,
    Coin,
    Item,
    Leticker,
    ActorToy,
    ActorJenny,
    ActorMama,
    ActorPapa,
    ActorTao,
    ActorDeca,
    Army,
    Spider,
    SpiderSmall,
    SpiderBoss,
    Dust,
    HBox,
    Free,
    Unk23,
    Plug,
}

// Macro used in the generated object list
macro_rules! declare_objects {
    {
        $($index:literal => $id:ident { $name:tt, $class:ident, $subclass:literal, $path:literal }),*
        $(,)*
    } => {
        /// An object ID.
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(IntoPrimitive, TryFromPrimitive)]
        #[repr(i32)]
        pub enum Object {
            $($id = $index),*
        }

        const METADATA: &[Metadata] = &[
            $(
                Metadata {
                    id: Object::$id,
                    name: $name,
                    class: ObjectClass::$class,
                    subclass: $subclass,
                    path: $path,
                }
            ),*
        ];

        static LOOKUP: phf::Map<UniCase<&'static str>, Object> = phf_map! {
            $(UniCase::ascii($name) => Object::$id),*
        };
    }
}

impl Object {
    /// The total number of non-internal objects.
    pub const MAIN_COUNT: usize = 1162;
    /// The total number of internal objects.
    pub const INTERNAL_COUNT: usize = 36;
    /// The ID that internal objects start at.
    pub const INTERNAL_BASE: i32 = 10000;

    /// Returns the object's engine class.
    #[inline]
    pub fn class(self) -> ObjectClass {
        self.meta().class
    }

    /// Returns a subclass value meaningful to the engine class.
    #[inline]
    pub fn subclass(self) -> u16 {
        self.meta().subclass
    }

    /// Returns the object's model path inside qp.bin.
    #[inline]
    pub fn qp_path(self) -> &'static str {
        self.meta().path
    }

    fn meta(self) -> &'static Metadata {
        let id = i32::from(self);
        let internal = id - Self::INTERNAL_BASE;
        if internal >= 0 {
            &METADATA[internal as usize + Self::MAIN_COUNT]
        } else {
            &METADATA[id as usize]
        }
    }
}

impl Debug for Object {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "<{}>", self.name())
    }
}

impl Sealed for Object {}

impl Resource for Object {
    type Value = i32;
    const COUNT: usize = Self::MAIN_COUNT + Self::INTERNAL_COUNT;

    #[inline]
    fn at(index: i32) -> Self {
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

// Generated using unplug-datagen
include!("gen/objects.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_regular_object() {
        let object = Object::NpcTonpy;
        assert_eq!(object.name(), "npc_tonpy");
        assert_eq!(object.class(), ObjectClass::ActorToy); // telly is a toy CONFIRMED
        assert_eq!(object.subclass(), 10);
        assert_eq!(object.qp_path(), "npc/tonpy");
        assert_eq!(format!("{:?}", object), "<npc_tonpy>");
    }

    #[test]
    fn test_get_internal_object() {
        let object = Object::InternalExclamation;
        assert_eq!(object.name(), "internal_exclamation");
        assert_eq!(object.class(), ObjectClass::Free);
        assert_eq!(object.subclass(), 0);
        assert_eq!(object.qp_path(), "exclamation");
        assert_eq!(format!("{:?}", object), "<internal_exclamation>");
    }

    #[test]
    fn test_find() {
        assert_eq!(Object::find("npc_tonpy"), Some(Object::NpcTonpy));
        assert_eq!(Object::find("NpC_tOnPy"), Some(Object::NpcTonpy));
        assert_eq!(Object::find("internal_exclamation"), Some(Object::InternalExclamation));
        assert_eq!(Object::find("InTeRnAl_ExClAmAtIoN"), Some(Object::InternalExclamation));
        assert_eq!(Object::find("foo"), None);
    }

    #[test]
    fn test_iter() {
        let objects = Object::iter().collect::<Vec<_>>();
        assert_eq!(objects.len(), 1198);
        assert_eq!(objects[0], Object::CbRobo);
        assert_eq!(objects[1161], Object::NpcSunUfo);
        assert_eq!(objects[1162], Object::InternalTitleIconBb);
        assert_eq!(objects[1197], Object::InternalUsBatuModel);
    }
}
