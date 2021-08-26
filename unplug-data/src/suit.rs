use crate::{Error, Item, Result};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::convert::TryFrom;

/// Metadata describing a suit.
#[derive(Debug)]
pub struct SuitDefinition {
    /// The suit's corresponding `Suit`.
    pub id: Suit,
    /// The item corresponding to the suit.
    pub item: Item,
    /// The suit's English display name (may be empty).
    pub display_name: &'static str,
}

impl SuitDefinition {
    /// Retrieves the definition corresponding to a `Suit`.
    pub fn get(id: Suit) -> &'static SuitDefinition {
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
        pub enum Suit {
            $($id = $index),*
        }

        pub static SUITS: &[SuitDefinition] = &[
            $(
                SuitDefinition {
                    id: Suit::$id,
                    item: Item::$item,
                    display_name: $display_name,
                }
            ),*
        ];
    };
}

/// `From` impl for converting `Suit`s to a corresponding `Item`
impl From<Suit> for Item {
    fn from(suit: Suit) -> Self {
        SuitDefinition::get(suit).item
    }
}

/// `TryFrom` impl for converting `Item`s to a corresponding `Suit`
impl TryFrom<Item> for Suit {
    type Error = Error;
    fn try_from(item: Item) -> Result<Self> {
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
        let suit = SuitDefinition::get(Suit::Pajamas);
        assert_eq!(suit.id, Suit::Pajamas);
        assert_eq!(suit.item, Item::Pajamas);
        assert_eq!(suit.display_name, "Pajamas");
    }

    #[test]
    fn test_item_from_suit() {
        assert_eq!(Item::from(Suit::Pajamas), Item::Pajamas);
    }

    #[test]
    fn test_try_suit_from_item() {
        assert_eq!(Suit::try_from(Item::Pajamas), Ok(Suit::Pajamas));
        assert_eq!(Suit::try_from(Item::HotRod), Err(Error::NoItemSuit(Item::HotRod)));
    }
}
