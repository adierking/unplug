use crate::item::ItemId;
use crate::{Error, Result};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::convert::TryFrom;

/// Metadata describing a suit.
#[derive(Debug)]
pub struct SuitDefinition {
    /// The suit's corresponding `SuitId`.
    pub id: SuitId,
    /// The item corresponding to the suit.
    pub item: ItemId,
    /// The suit's English display name (may be empty).
    pub display_name: &'static str,
}

impl SuitDefinition {
    /// Retrieves the definition corresponding to a `SuitId`.
    pub fn get(id: SuitId) -> &'static SuitDefinition {
        &SUITS[i16::from(id) as usize - 1]
    }
}

// Macro used in the generated suit list
macro_rules! declare_suits {
    {
        $($index:literal => $id:ident { $item:ident, $display_name:literal }),*
        $(,)*
    } => {
        #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(IntoPrimitive, TryFromPrimitive)]
        #[repr(i16)]
        pub enum SuitId {
            $($id = $index),*
        }

        pub static SUITS: &[SuitDefinition] = &[
            $(
                SuitDefinition {
                    id: SuitId::$id,
                    item: ItemId::$item,
                    display_name: $display_name,
                }
            ),*
        ];
    };
}

/// `From` impl for converting `SuitId`s to a corresponding `ItemId`
impl From<SuitId> for ItemId {
    fn from(suit: SuitId) -> Self {
        SuitDefinition::get(suit).item
    }
}

/// `TryFrom` impl for converting `ItemId`s to a corresponding `SuitId`
impl TryFrom<ItemId> for SuitId {
    type Error = Error;
    fn try_from(item: ItemId) -> Result<Self> {
        if let Some(suit) = SUITS.iter().find(|s| s.item == item) {
            Ok(suit.id)
        } else {
            Err(Error::NoItemSuit(item))
        }
    }
}

// Generated using unplug-datagen
include!("gen/suits.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_suit() {
        let suit = SuitDefinition::get(SuitId::Pajamas);
        assert_eq!(suit.id, SuitId::Pajamas);
        assert_eq!(suit.item, ItemId::Pajamas);
        assert_eq!(suit.display_name, "Pajamas");
    }

    #[test]
    fn test_item_from_suit() {
        assert_eq!(ItemId::from(SuitId::Pajamas), ItemId::Pajamas);
    }

    #[test]
    fn test_try_suit_from_item() {
        assert_eq!(SuitId::try_from(ItemId::Pajamas), Ok(SuitId::Pajamas));
        assert_eq!(SuitId::try_from(ItemId::HotRod), Err(Error::NoItemSuit(ItemId::HotRod)));
    }
}
