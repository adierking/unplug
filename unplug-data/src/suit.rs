use crate::private::Sealed;
use crate::resource::{Resource, ResourceIterator};
use crate::{Error, Item, Result};
use num_enum::{IntoPrimitive, TryFromPrimitive};

/// The total number of suits.
pub const NUM_SUITS: usize = 9;

/// Metadata describing a suit.
struct Metadata {
    /// A unique name assigned by unplug-datagen.
    name: &'static str,
    /// The item corresponding to the suit, if there is one.
    item: Option<Item>,
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

        const METADATA: &[Metadata] = &[
            $(
                Metadata {
                    name: $name,
                    item: __impl_item_id!($item),
                }
            ),*
        ];
    };
}

impl Suit {
    /// Returns an iterator over all suit IDs.
    pub fn iter() -> ResourceIterator<Self> {
        ResourceIterator::new()
    }

    /// Tries to find the suit whose name matches `name`.
    pub fn find(name: &str) -> Option<Self> {
        // skip(1) to ignore None
        Self::iter().skip(1).find(|s| s.name() == name)
    }

    /// Returns a unique name for the suit assigned by unplug-datagen.
    pub fn name(self) -> &'static str {
        self.meta().name
    }

    /// Returns the item corresponding to the suit, if there is one.
    pub fn item(self) -> Option<Item> {
        self.meta().item
    }

    fn meta(self) -> &'static Metadata {
        &METADATA[i16::from(self) as usize]
    }
}

impl Sealed for Suit {}

impl Resource for Suit {
    const COUNT: usize = NUM_SUITS;
    fn at(index: usize) -> Self {
        Suit::try_from(index as i16).unwrap()
    }
}

/// `From` impl for converting `Suit`s to a corresponding `Item`
impl TryFrom<Suit> for Item {
    type Error = Error;
    fn try_from(suit: Suit) -> Result<Self> {
        suit.item().ok_or(Error::NoSuitItem(suit))
    }
}

/// `TryFrom` impl for converting `Item`s to a corresponding `Suit`
impl TryFrom<Item> for Suit {
    type Error = Error;
    fn try_from(item: Item) -> Result<Self> {
        Suit::iter().find(|s| s.item() == Some(item)).ok_or(Error::NoItemSuit(item))
    }
}

// Generated using unplug-datagen
include!("gen/suits.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_suit() {
        let suit = Suit::Pajamas;
        assert_eq!(suit.name(), "pajamas");
        assert_eq!(suit.item(), Some(Item::Pajamas));
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
        assert_eq!(Suit::find("frog"), Some(Suit::Frog));
        assert_eq!(Suit::find("foo"), None);
        assert_eq!(Suit::find("none"), None);
    }

    #[test]
    fn test_iter() {
        let suits = Suit::iter().collect::<Vec<_>>();
        assert_eq!(suits.len(), NUM_SUITS);
        assert_eq!(suits[0], Suit::None);
        assert_eq!(suits[1], Suit::DrakeRedcrest);
        assert_eq!(suits[7], Suit::Pajamas);
        assert_eq!(suits[8], Suit::SuperChibiRobo);
    }
}
