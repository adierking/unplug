use num_enum::{IntoPrimitive, TryFromPrimitive};

/// The total number of non-internal objects.
pub const NUM_OBJECTS: usize = 1162;

#[derive(Debug)]
pub struct ObjectDefinition {
    pub id: ObjectId,
    pub class: ObjectClass,
    pub subclass: u16,
    pub path: &'static str,
}

impl ObjectDefinition {
    /// Retrieves the definition corresponding to an `ObjectId`.
    pub fn get(id: ObjectId) -> &'static ObjectDefinition {
        &OBJECTS[i32::from(id) as usize]
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
        #[derive(Debug, Copy, Clone, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
        #[repr(i32)]
        pub enum ObjectId {
            $($id = $index),*
        }

        pub static OBJECTS: &[ObjectDefinition] = &[
            $(
                ObjectDefinition {
                    id: ObjectId::$id,
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
