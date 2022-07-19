use crate::private::Sealed;
use crate::Resource;
use bitflags::bitflags;
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

bitflags! {
    /// Bitflags which define how an object behaves.
    pub struct ObjectFlags: u32 {
        /// The object spawns when the stage loads.
        const SPAWN = 1 << 0;
        /// The object can obscure the player without showing any silhouette.
        const OPAQUE = 1 << 1;
        /// The object allows blaster projectiles to pass through it.
        const BLASTTHRU = 1 << 2;
        /// The radar will point to the object if it is nearby.
        const RADAR = 1 << 3;
        /// The object is not solid and other objects can pass through it.
        const INTANGIBLE = 1 << 4;
        /// The object is drawn fully transparent. This isn't the same as the object not rendering
        /// at all because it may still obscure some objects and shadows.
        const INVISIBLE = 1 << 5;
        /// The object is lit using a toon effect.
        const TOON = 1 << 6;
        /// The object flashes like an item.
        const FLASH = 1 << 7;
        /// Possibly not an accurate name, need to look into this more
        const UNLIT = 1 << 8;
        /// The object always shows in the utilibot camera window.
        const BOTCAM = 1 << 9;
        /// The object can be destroyed with the blaster.
        const EXPLODE = 1 << 10;
        /// The object allows other objects to be pushed through it. Some object classes permit this
        /// by default.
        const PUSHTHRU = 1 << 11;
        /// The object will not be prioritized in interactions. If this is not set and the player
        /// presses A close to the object, they will automatically walk up and interact with it.
        const LOWPRI = 1 << 12;
        /// The object shows in the floor reflection.
        const REFLECT = 1 << 13;
        /// The object blocks other objects from being pushed through it. Some object classes block
        /// pushing by default.
        const PUSHBLOCK = 1 << 14;
        /// The object is culled when not being looked at. Doesn't work well with large objects.
        const CULL = 1 << 15;
        /// The player can lift the object up.
        const LIFT = 1 << 16;
        /// The player can climb on the object.
        const CLIMB = 1 << 17;
        /// The player can clamber up to surfaces on the object.
        const CLAMBER = 1 << 18;
        /// The player can climb up the object as a ladder.
        const LADDER = 1 << 19;
        /// The player can climb up the object as a rope.
        const ROPE = 1 << 20;
        /// The object is a staircase (i.e. it has internal ledges). The object's data value
        /// indicates the height of each step.
        const STAIRS = 1 << 21;
        /// The object will fall if it is pushed off a ledge.
        const FALL = 1 << 22;
        /// The player can grab the object and push/pull it.
        const GRAB = 1 << 23;
        /// The object can be interacted with by walking up to it and pressing A.
        const INTERACT = 1 << 24;
        /// The object responds to being touched by the player.
        const TOUCH = 1 << 25;
        /// The object responds to attachments.
        const ATC = 1 << 26;
        /// The object responds to projectiles.
        const PROJECTILE = 1 << 27;
        /// Unknown, used only by a few objects
        const UNK_28 = 1 << 28;
        /// The object shows in mirrors.
        const MIRROR = 1 << 29;
        /// Unknown, not used by any stages
        const UNK_30 = 1 << 30;
        /// The object is disabled and cannot be spawned.
        const DISABLED = 1 << 31;
    }
}

/// See `ObjectPlacement` in the main library.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct RawObjectPlacement {
    pub id: Object,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub rotate_x: i32,
    pub rotate_y: i32,
    pub rotate_z: i32,
    pub scale_x: i32,
    pub scale_y: i32,
    pub scale_z: i32,
    pub data: i32,
    pub spawn_flag: i32,
    pub variant: i32,
    pub flags: ObjectFlags,
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
