use super::Object;
use crate::object::ObjectClass;
use crate::{Error, Result};
use bitflags::bitflags;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::fmt::{self, Debug, Formatter};

/// Metadata describing an item.
#[derive(Debug)]
pub struct ItemDefinition {
    /// The item's corresponding `Item`.
    pub id: Item,
    /// A unique name assigned by unplug-datagen.
    pub name: &'static str,
    /// The object corresponding to this item, if there is one.
    pub object: Option<Object>,
    /// Flags describing the item.
    pub flags: ItemFlags,
}

impl ItemDefinition {
    /// Retrieves the definition corresponding to an `Item`.
    pub fn get(id: Item) -> &'static ItemDefinition {
        &ITEMS[i16::from(id) as usize]
    }

    /// Tries to find the item definition whose name matches `name`.
    pub fn find(name: &str) -> Option<&'static ItemDefinition> {
        ITEMS.iter().find(|i| i.name == name)
    }
}

bitflags! {
    /// Flags describing an item.
    pub struct ItemFlags: u32 {
        /// The item is not actually used by the game.
        const UNUSED = 0x1;
    }
}

/// Expands an object ID name into an `Option<Object>`.
macro_rules! __impl_object_id {
    (None) => {
        None
    };
    ($object:ident) => {
        Some(Object::$object)
    };
}

// Macro used in the generated item list
macro_rules! declare_items {
    {
        $($index:literal => $id:ident { $name:literal, $object:ident $(, $flag:ident)* }),*
        $(,)*
    } => {
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(IntoPrimitive, TryFromPrimitive)]
        #[repr(i16)]
        pub enum Item {
            $($id = $index),*
        }

        pub static ITEMS: &[ItemDefinition] = &[
            $(
                ItemDefinition {
                    id: Item::$id,
                    name: $name,
                    object: __impl_object_id!($object),
                    flags: ItemFlags::from_bits_truncate(0 $(| ItemFlags::$flag.bits())*),
                }
            ),*
        ];
    }
}

impl Debug for Item {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "<{}>", ItemDefinition::get(*self).name)
    }
}

/// `TryFrom` impl for converting `Object`s to a corresponding `Item`
impl TryFrom<Object> for Item {
    type Error = Error;
    fn try_from(obj: Object) -> Result<Self> {
        if obj.class() == ObjectClass::Item {
            let id = obj.subclass() as i16;
            Item::try_from(id).or(Err(Error::NoObjectItem(obj)))
        } else {
            Err(Error::NoObjectItem(obj))
        }
    }
}

/// `TryFrom` impl for converting `Items`s to a corresponding `Object`
impl TryFrom<Item> for Object {
    type Error = Error;
    fn try_from(item: Item) -> Result<Self> {
        ItemDefinition::get(item).object.ok_or(Error::NoItemObject(item))
    }
}

// Generated using unplug-datagen
include!("gen/items.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_item() {
        let item = ItemDefinition::get(Item::Wastepaper);
        assert_eq!(item.id, Item::Wastepaper);
        assert_eq!(item.name, "wastepaper");
        assert_eq!(item.object, Some(Object::ItemKamiKuzu));
        assert!(item.flags.is_empty());
        assert_eq!(format!("{:?}", item.id), "<wastepaper>");
    }

    #[test]
    fn test_get_item_without_object() {
        let item = ItemDefinition::get(Item::Unk20);
        assert_eq!(item.id, Item::Unk20);
        assert_eq!(item.name, "unk_20");
        assert_eq!(item.object, None);
        assert_eq!(item.flags, ItemFlags::UNUSED);
        assert_eq!(format!("{:?}", item.id), "<unk_20>");
    }

    #[test]
    fn test_find_item() {
        assert_eq!(ItemDefinition::find("wastepaper").unwrap().id, Item::Wastepaper);
        assert_eq!(ItemDefinition::find("unk_20").unwrap().id, Item::Unk20);
        assert!(ItemDefinition::find("foo").is_none());
    }

    #[test]
    fn test_try_item_from_object() {
        assert_eq!(Item::try_from(Object::ItemKamiKuzu), Ok(Item::Wastepaper));
        assert_eq!(Item::try_from(Object::YogoreOil), Err(Error::NoObjectItem(Object::YogoreOil)));
    }

    #[test]
    fn test_try_object_from_item() {
        assert_eq!(Object::try_from(Item::Wastepaper), Ok(Object::ItemKamiKuzu));
        assert_eq!(Object::try_from(Item::Unk20), Err(Error::NoItemObject(Item::Unk20)));
    }
}
