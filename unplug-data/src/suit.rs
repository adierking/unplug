use crate::private::Sealed;
use crate::{Error, Item, Resource, Result};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use phf::phf_map;
use unicase::UniCase;

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
        $($index:literal => $id:ident { $name:tt, $item:ident }),*
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

        static LOOKUP: phf::Map<UniCase<&'static str>, Suit> = phf_map! {
            $(UniCase::ascii($name) => Suit::$id),*
        };
    };
}

impl Suit {
    /// Returns the item corresponding to the suit, if there is one.
    #[inline]
    pub fn item(self) -> Option<Item> {
        self.meta().item
    }

    #[inline]
    fn meta(self) -> &'static Metadata {
        &METADATA[i16::from(self) as usize]
    }
}

impl Sealed for Suit {}

impl Resource for Suit {
    type Value = i16;
    const COUNT: usize = METADATA.len();

    #[inline]
    fn at(index: i16) -> Self {
        Suit::try_from(index).unwrap()
    }

    #[inline]
    fn name(self) -> &'static str {
        self.meta().name
    }

    #[inline]
    fn is_none(self) -> bool {
        self == Suit::None
    }

    fn find(name: impl AsRef<str>) -> Option<Self> {
        LOOKUP.get(&UniCase::ascii(name.as_ref())).copied()
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
        assert!(suit.is_some());
        assert_eq!(suit.name(), "pajamas");
        assert_eq!(suit.item(), Some(Item::Pajamas));
    }

    #[test]
    fn test_get_none() {
        let suit = Suit::None;
        assert!(suit.is_none());
        assert_eq!(suit.name(), "none");
        assert_eq!(suit.item(), None);
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
        assert_eq!(Suit::find("FrOg"), Some(Suit::Frog));
        assert_eq!(Suit::find("none"), Some(Suit::None));
        assert_eq!(Suit::find("foo"), None);
    }

    #[test]
    fn test_iter() {
        let suits = Suit::iter().collect::<Vec<_>>();
        assert_eq!(suits.len(), 9);
        assert_eq!(suits[0], Suit::None);
        assert_eq!(suits[1], Suit::DrakeRedcrest);
        assert_eq!(suits[7], Suit::Pajamas);
        assert_eq!(suits[8], Suit::SuperChibiRobo);
    }
}
