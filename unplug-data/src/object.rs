use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::fmt::{self, Debug, Formatter};

/// The total number of non-internal objects.
pub const NUM_OBJECTS: usize = 1162;

/// The ID that internal objects start at.
const INTERNAL_OBJECT_BASE: usize = 10000;

#[derive(Debug)]
pub struct ObjectDefinition {
    /// The object's corresponding `Object`.
    pub id: Object,
    /// A unique name assigned by unplug-datagen.
    pub name: &'static str,
    /// The object's engine class.
    pub class: ObjectClass,
    /// A subclass value meaningful to the engine class.
    pub subclass: u16,
    /// The object's model path.
    pub path: &'static str,
}

impl ObjectDefinition {
    /// Retrieves the definition corresponding to an `Object`.
    pub fn get(id: Object) -> &'static ObjectDefinition {
        let mut index = i32::from(id) as usize;
        if index >= INTERNAL_OBJECT_BASE {
            index = index - INTERNAL_OBJECT_BASE + NUM_OBJECTS;
        }
        &OBJECTS[index]
    }

    /// Tries to find the object definition whose name matches `name`.
    pub fn find(name: &str) -> Option<&'static ObjectDefinition> {
        OBJECTS.iter().find(|o| o.name == name)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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
        $($index:literal => $id:ident { $name:literal, $class:ident, $subclass:literal, $path:literal }),*
        $(,)*
    } => {
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(IntoPrimitive, TryFromPrimitive)]
        #[repr(i32)]
        pub enum Object {
            $($id = $index),*
        }

        pub static OBJECTS: &[ObjectDefinition] = &[
            $(
                ObjectDefinition {
                    id: Object::$id,
                    name: $name,
                    class: ObjectClass::$class,
                    subclass: $subclass,
                    path: $path,
                }
            ),*
        ];
    }
}

impl Debug for Object {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "<{}>", ObjectDefinition::get(*self).name)
    }
}

// Generated using unplug-datagen
include!("gen/objects.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_regular_object() {
        let object = ObjectDefinition::get(Object::NpcTonpy);
        assert_eq!(object.id, Object::NpcTonpy);
        assert_eq!(object.name, "npc_tonpy");
        assert_eq!(object.class, ObjectClass::ActorToy); // telly is a toy CONFIRMED
        assert_eq!(object.subclass, 10);
        assert_eq!(object.path, "npc/tonpy");
        assert_eq!(format!("{:?}", object.id), "<npc_tonpy>");
    }

    #[test]
    fn test_get_internal_object() {
        let object = ObjectDefinition::get(Object::InternalExclamation);
        assert_eq!(object.id, Object::InternalExclamation);
        assert_eq!(object.name, "internal_exclamation");
        assert_eq!(object.class, ObjectClass::Free);
        assert_eq!(object.subclass, 0);
        assert_eq!(object.path, "exclamation");
        assert_eq!(format!("{:?}", object.id), "<internal_exclamation>");
    }

    #[test]
    fn test_find_object() {
        assert_eq!(ObjectDefinition::find("npc_tonpy").unwrap().id, Object::NpcTonpy);
        assert_eq!(
            ObjectDefinition::find("internal_exclamation").unwrap().id,
            Object::InternalExclamation
        );
        assert!(ObjectDefinition::find("foo").is_none());
    }
}
