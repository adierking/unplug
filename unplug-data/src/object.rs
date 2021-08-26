use num_enum::{IntoPrimitive, TryFromPrimitive};

/// The total number of non-internal objects.
pub const NUM_OBJECTS: usize = 1162;

/// The ID that internal objects start at.
const INTERNAL_OBJECT_BASE: usize = 10000;

#[derive(Debug)]
pub struct ObjectDefinition {
    pub id: Object,
    pub class: ObjectClass,
    pub subclass: u16,
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
        $($index:literal => $id:ident { $class:ident, $subclass:literal, $path:literal }),*
        $(,)*
    } => {
        #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(IntoPrimitive, TryFromPrimitive)]
        #[repr(i32)]
        pub enum Object {
            $($id = $index),*
        }

        pub static OBJECTS: &[ObjectDefinition] = &[
            $(
                ObjectDefinition {
                    id: Object::$id,
                    class: ObjectClass::$class,
                    subclass: $subclass,
                    path: $path,
                }
            ),*
        ];
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
        assert_eq!(object.class, ObjectClass::ActorToy); // telly is a toy CONFIRMED
        assert_eq!(object.subclass, 10);
        assert_eq!(object.path, "npc/tonpy");
    }

    #[test]
    fn test_get_internal_object() {
        let object = ObjectDefinition::get(Object::InternalExclamation);
        assert_eq!(object.id, Object::InternalExclamation);
        assert_eq!(object.class, ObjectClass::Free);
        assert_eq!(object.subclass, 0);
        assert_eq!(object.path, "exclamation");
    }
}
