use crate::{Error, Item, Result};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::convert::TryFrom;
use std::fmt::{self, Debug, Formatter};

/// Metadata describing an attachment (ATC).
#[derive(Debug)]
pub struct AtcDefinition {
    /// The attachment's corresponding `Atc`.
    pub id: Atc,
    /// A unique name assigned by unplug-datagen.
    pub name: &'static str,
}

impl AtcDefinition {
    /// Retrieves the definition corresponding to an `Atc`.
    pub fn get(id: Atc) -> &'static AtcDefinition {
        &ATCS[i16::from(id) as usize - 1]
    }

    /// Tries to find the ATC definition whose name matches `name`.
    pub fn find(name: &str) -> Option<&'static AtcDefinition> {
        ATCS.iter().find(|a| a.name == name)
    }
}

// Macro used in the generated ATC list
macro_rules! declare_atcs {
    {
        $($index:literal => $id:ident { $name:literal }),*
        $(,)*
    } => {
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(IntoPrimitive, TryFromPrimitive)]
        #[repr(i16)]
        pub enum Atc {
            $($id = $index),*
        }

        pub static ATCS: &[AtcDefinition] = &[
            $(AtcDefinition {
                id: Atc::$id,
                name: $name,
            }),*
        ];
    };
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
        write!(f, "<{}>", AtcDefinition::get(*self).name)
    }
}

// Generated using unplug-datagen
include!("gen/atcs.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_atc() {
        let atc = AtcDefinition::get(Atc::Toothbrush);
        assert_eq!(atc.id, Atc::Toothbrush);
        assert_eq!(atc.name, "toothbrush");
        assert_eq!(format!("{:?}", atc.id), "<toothbrush>");
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
        assert_eq!(AtcDefinition::find("toothbrush").unwrap().id, Atc::Toothbrush);
        assert!(AtcDefinition::find("foo").is_none());
    }
}
