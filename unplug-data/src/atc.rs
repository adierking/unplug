use crate::private::Sealed;
use crate::{Error, Item, Resource, Result};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use phf::phf_map;
use std::convert::TryFrom;
use std::fmt::{self, Debug, Formatter};
use unicase::UniCase;

/// Metadata describing an attachment (ATC).
struct Metadata {
    /// A unique name assigned by unplug-datagen.
    name: &'static str,
}

// Macro used in the generated ATC list
macro_rules! declare_atcs {
    {
        $($index:literal => $id:ident { $name:tt }),*
        $(,)*
    } => {
        /// An attachment (ATC) ID.
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(IntoPrimitive, TryFromPrimitive)]
        #[repr(i16)]
        pub enum Atc {
            $($id = $index),*
        }

        const METADATA: &[Metadata] = &[
            $(Metadata {
                name: $name,
            }),*
        ];

        static LOOKUP: phf::Map<UniCase<&'static str>, Atc> = phf_map! {
            $(UniCase::ascii($name) => Atc::$id),*
        };
    };
}

impl Atc {
    #[inline]
    fn meta(self) -> &'static Metadata {
        &METADATA[i16::from(self) as usize]
    }
}

impl Sealed for Atc {}

impl Resource for Atc {
    type Value = i16;
    const COUNT: usize = METADATA.len();

    #[inline]
    fn at(index: i16) -> Self {
        Atc::try_from(index).unwrap()
    }

    #[inline]
    fn name(self) -> &'static str {
        self.meta().name
    }

    #[inline]
    fn is_none(self) -> bool {
        self == Atc::None
    }

    fn find(name: impl AsRef<str>) -> Option<Self> {
        LOOKUP.get(&UniCase::ascii(name.as_ref())).copied()
    }
}

/// `TryFrom` impl for converting `Item`s to a corresponding `Atc`
impl TryFrom<Item> for Atc {
    type Error = Error;
    fn try_from(item: Item) -> Result<Self> {
        Ok(match item {
            Item::ChibiBlaster => Self::ChibiBlaster,
            Item::ChibiRadar => Self::ChibiRadar,
            Item::Toothbrush => Self::Toothbrush,
            Item::Spoon => Self::Spoon,
            Item::Mug => Self::Mug,
            Item::Squirter => Self::Squirter,
            _ => return Err(Error::NoItemAtc(item)),
        })
    }
}

/// `TryFrom` impl for converting `Atc`s to a corresponding `Item`
impl TryFrom<Atc> for Item {
    type Error = Error;
    fn try_from(atc: Atc) -> Result<Self> {
        Ok(match atc {
            Atc::ChibiBlaster => Self::ChibiBlaster,
            Atc::ChibiRadar => Self::ChibiRadar,
            Atc::Toothbrush => Self::Toothbrush,
            Atc::Spoon => Self::Spoon,
            Atc::Mug => Self::Mug,
            Atc::Squirter => Self::Squirter,
            _ => return Err(Error::NoAtcItem(atc)),
        })
    }
}

impl Debug for Atc {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "<{}>", self.name())
    }
}

impl Default for Atc {
    fn default() -> Self {
        Self::None
    }
}

// Generated using unplug-datagen
include!("gen/atcs.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_atc() {
        let atc = Atc::Toothbrush;
        assert!(atc.is_some());
        assert_eq!(atc.name(), "toothbrush");
        assert_eq!(format!("{:?}", atc), "<toothbrush>");
    }

    #[test]
    fn test_get_none() {
        let atc = Atc::None;
        assert!(atc.is_none());
        assert_eq!(atc.name(), "none");
    }

    #[test]
    fn test_try_atc_from_item() {
        assert_eq!(Atc::try_from(Item::Toothbrush), Ok(Atc::Toothbrush));
        assert_eq!(Atc::try_from(Item::HotRod), Err(Error::NoItemAtc(Item::HotRod)));
    }

    #[test]
    fn test_try_item_from_atc() {
        assert_eq!(Item::try_from(Atc::Toothbrush), Ok(Item::Toothbrush));
        assert_eq!(Item::try_from(Atc::ChibiCopter), Err(Error::NoAtcItem(Atc::ChibiCopter)));
    }

    #[test]
    fn test_find_atc() {
        assert_eq!(Atc::find("toothbrush"), Some(Atc::Toothbrush));
        assert_eq!(Atc::find("ToOtHbRuSh"), Some(Atc::Toothbrush));
        assert_eq!(Atc::find("none"), Some(Atc::None));
        assert_eq!(Atc::find("foo"), None);
    }

    #[test]
    fn test_iter() {
        let atcs = Atc::iter().collect::<Vec<_>>();
        assert_eq!(atcs.len(), 9);
        assert_eq!(atcs[0], Atc::None);
        assert_eq!(atcs[1], Atc::ChibiCopter);
        assert_eq!(atcs[7], Atc::Squirter);
        assert_eq!(atcs[8], Atc::Unk8);
    }
}
