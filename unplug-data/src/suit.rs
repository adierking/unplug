use crate::{Error, Item, Result};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::convert::TryFrom;

/// Metadata describing a suit.
#[derive(Debug)]
pub struct SuitDefinition {
    /// The suit's corresponding `Suit`.
    pub id: Suit,
    /// A unique name assigned by unplug-datagen.
    pub name: &'static str,
    /// The item corresponding to the suit, if there is one.
    pub item: Option<Item>,
}

impl SuitDefinition {
    /// Retrieves the definition corresponding to a `Suit`.
    pub fn get(id: Suit) -> &'static Self {
        &SUITS[i16::from(id) as usize]
    }

    /// Tries to find the suit whose name matches `name`.
    pub fn find(name: &str) -> Option<&'static Self> {
        // skip(1) to ignore None
        SUITS.iter().skip(1).find(|s| s.name == name)
    }
}

/// Expands an item ID name into an `Option<Item>`.
macro_rules! __impl_item_id {
    (None) => {
        None
    };
    ($name:ident) => {
        Some(Item::$name)
    };
}

// Macro used in the generated suit list
macro_rules! declare_suits {
    {
        $($index:literal => $id:ident { $name:literal, $item:ident }),*
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
                    name: $name,
                    item: __impl_item_id!($item),
                }
            ),*
        ];
    };
}

/// `From` impl for converting `Suit`s to a corresponding `Item`
impl TryFrom<Suit> for Item {
    type Error = Error;
    fn try_from(suit: Suit) -> Result<Self> {
        SuitDefinition::get(suit).item.ok_or(Error::NoSuitItem(suit))
    }
}

/// `TryFrom` impl for converting `Item`s to a corresponding `Suit`
impl TryFrom<Item> for Suit {
    type Error = Error;
    fn try_from(item: Item) -> Result<Self> {
        SUITS.iter().find(|s| s.item == Some(item)).map(|s| s.id).ok_or(Error::NoItemSuit(item))
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
        assert_eq!(suit.name, "pajamas");
        assert_eq!(suit.item, Some(Item::Pajamas));
    }

    #[test]
    fn test_try_item_from_suit() {
        assert_eq!(Item::try_from(Suit::Pajamas), Ok(Item::Pajamas));
        assert_eq!(Item::try_from(Suit::None), Err(Error::NoSuitItem(Suit::None)));
    }

    #[test]
    fn test_try_suit_from_item() {
        assert_eq!(Suit::try_from(Item::Pajamas), Ok(Suit::Pajamas));
        assert_eq!(Suit::try_from(Item::HotRod), Err(Error::NoItemSuit(Item::HotRod)));
    }

    #[test]
    fn test_find_suit() {
        assert_eq!(SuitDefinition::find("frog").unwrap().id, Suit::Frog);
        assert!(SuitDefinition::find("foo").is_none());
        assert!(SuitDefinition::find("none").is_none());
    }
}
