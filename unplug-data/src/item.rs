use super::Object;
use bitflags::bitflags;
use num_enum::{IntoPrimitive, TryFromPrimitive};

/// Metadata describing an item.
#[derive(Debug)]
pub struct ItemDefinition {
    /// The item's corresponding `Item`.
    pub id: Item,
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
}

bitflags! {
    /// Flags describing an item.
    pub struct ItemFlags: u32 {
        /// The item does not have a known purpose.
        const UNKNOWN = 0x1;
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
        $($index:literal => $id:ident { $object:ident, $flags:expr }),*
        $(,)*
    } => {
        #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(IntoPrimitive, TryFromPrimitive)]
        #[repr(i16)]
        pub enum Item {
            $($id = $index),*
        }

        pub static ITEMS: &[ItemDefinition] = &[
            $(
                ItemDefinition {
                    id: Item::$id,
                    object: __impl_object_id!($object),
                    flags: $flags,
                }
            ),*
        ];
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
        assert_eq!(item.object, Some(Object::ItemKamiKuzu));
    }

    #[test]
    fn test_get_item_without_object() {
        let item = ItemDefinition::get(Item::Unk20);
        assert_eq!(item.id, Item::Unk20);
        assert_eq!(item.object, None);
    }
}
