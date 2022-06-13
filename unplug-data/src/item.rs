use super::Object;
use crate::object::ObjectClass;
use crate::private::Sealed;
use crate::resource::{Resource, ResourceIterator};
use crate::{Error, Result};
use bitflags::bitflags;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::fmt::{self, Debug, Formatter};

/// The total number of items.
pub const NUM_ITEMS: usize = 159;

/// Metadata describing an item.
#[derive(Debug)]
struct Metadata {
    /// A unique name assigned by unplug-datagen.
    name: &'static str,
    /// The object corresponding to this item, if there is one.
    object: Option<Object>,
    /// Flags describing the item.
    flags: ItemFlags,
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
        /// An item ID.
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(IntoPrimitive, TryFromPrimitive)]
        #[repr(i16)]
        pub enum Item {
            $($id = $index),*
        }

        const METADATA: &[Metadata] = &[
            $(
                Metadata {
                    name: $name,
                    object: __impl_object_id!($object),
                    flags: ItemFlags::from_bits_truncate(0 $(| ItemFlags::$flag.bits())*),
                }
            ),*
        ];
    }
}

impl Item {
    /// Returns an iterator over all item IDs.
    pub fn iter() -> ResourceIterator<Self> {
        ResourceIterator::new()
    }

    /// Tries to find the item definition whose name matches `name`.
    pub fn find(name: &str) -> Option<Self> {
        Self::iter().find(|i| i.name() == name)
    }

    /// Returns a unique name for the item assigned by unplug-datagen.
    pub fn name(self) -> &'static str {
        self.meta().name
    }

    /// Returns the object corresponding to this item, if there is one.
    pub fn object(self) -> Option<Object> {
        self.meta().object
    }

    /// Returns flags describing the item.
    pub fn flags(self) -> ItemFlags {
        self.meta().flags
    }

    fn meta(self) -> &'static Metadata {
        &METADATA[i16::from(self) as usize]
    }
}

impl Sealed for Item {}

impl Resource for Item {
    const COUNT: usize = NUM_ITEMS;
    fn at(index: usize) -> Self {
        Item::try_from(index as i16).unwrap()
    }
}

impl Debug for Item {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "<{}>", self.name())
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
        item.object().ok_or(Error::NoItemObject(item))
    }
}

// Generated using unplug-datagen
include!("gen/items.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_item() {
        let item = Item::Wastepaper;
        assert_eq!(item.name(), "wastepaper");
        assert_eq!(item.object(), Some(Object::ItemKamiKuzu));
        assert_eq!(item.flags(), ItemFlags::empty());
        assert_eq!(format!("{:?}", item), "<wastepaper>");
    }

    #[test]
    fn test_get_item_without_object() {
        let item = Item::Unk20;
        assert_eq!(item.name(), "unk_20");
        assert_eq!(item.object(), None);
        assert_eq!(item.flags(), ItemFlags::UNUSED);
        assert_eq!(format!("{:?}", item), "<unk_20>");
    }

    #[test]
    fn test_find_item() {
        assert_eq!(Item::find("wastepaper"), Some(Item::Wastepaper));
        assert_eq!(Item::find("unk_20"), Some(Item::Unk20));
        assert_eq!(Item::find("foo"), None);
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

    #[test]
    fn test_iter() {
        let items = Item::iter().collect::<Vec<_>>();
        assert_eq!(items.len(), NUM_ITEMS);
        assert_eq!(items[0], Item::FrogRing);
        assert_eq!(items[1], Item::Pen);
        assert_eq!(items[157], Item::WhiteFlowers);
        assert_eq!(items[158], Item::ChibiBattery);
    }
}
