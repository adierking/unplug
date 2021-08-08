use super::object::ObjectId;
use num_enum::{IntoPrimitive, TryFromPrimitive};

/// Metadata describing an item.
#[derive(Debug)]
pub struct ItemDefinition {
    /// The item's corresponding `ItemId`.
    pub id: ItemId,
    /// The object corresponding to this item, if there is one.
    pub object: Option<ObjectId>,
    /// The item's English display name (may be empty).
    pub display_name: &'static str,
}

impl ItemDefinition {
    /// Retrieves the definition corresponding to an `ItemId`.
    pub fn get(id: ItemId) -> &'static ItemDefinition {
        &ITEMS[i16::from(id) as usize]
    }
}

/// Expands an object ID name into an `Option<ObjectId>`.
macro_rules! __impl_object_id {
    (None) => {
        None
    };
    ($object:ident) => {
        Some(ObjectId::$object)
    };
}

// Macro used in the generated item list
macro_rules! declare_items {
    {
        $($index:literal => $id:ident { $object:ident, $display_name:literal }),*
        $(,)*
    } => {
        #[derive(Debug, Copy, Clone, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
        #[repr(i16)]
        pub enum ItemId {
            $($id = $index),*
        }

        pub static ITEMS: &[ItemDefinition] = &[
            $(
                ItemDefinition {
                    id: ItemId::$id,
                    object: __impl_object_id!($object),
                    display_name: $display_name,
                }
            ),*
        ];
    }
}

// Generated using unplug-datagen
include!("gen/items.inc.rs");
