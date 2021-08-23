use crate::item::ItemId;
use crate::{Error, Result};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::convert::TryFrom;

/// Metadata describing an attachment (ATC).
#[derive(Debug)]
pub struct AtcDefinition {
    /// The attachment's corresponding `AtcId`.
    pub id: AtcId,
    /// The attachment's English display name (may be empty).
    pub display_name: &'static str,
}

impl AtcDefinition {
    /// Retrieves the definition corresponding to an `AtcId`.
    pub fn get(id: AtcId) -> &'static AtcDefinition {
        &ATCS[i16::from(id) as usize - 1]
    }
}

// Macro used in the generated ATC list
macro_rules! declare_atcs {
    {
        $($index:literal => $id:ident { $display_name:literal }),*
        $(,)*
    } => {
        #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(IntoPrimitive, TryFromPrimitive)]
        #[repr(i16)]
        pub enum AtcId {
            $($id = $index),*
        }

        pub static ATCS: &[AtcDefinition] = &[
            $(
                AtcDefinition {
                    id: AtcId::$id,
                    display_name: $display_name,
                }
            ),*
        ];
    };
}

/// `TryFrom` impl for converting `ItemId`s to a corresponding `AtcId`
impl TryFrom<ItemId> for AtcId {
    type Error = Error;
    fn try_from(item: ItemId) -> Result<Self> {
        Ok(match item {
            ItemId::ChibiBlaster => Self::ChibiBlaster,
            ItemId::ChibiRadar => Self::ChibiRadar,
            ItemId::Toothbrush => Self::Toothbrush,
            ItemId::Spoon => Self::Spoon,
            ItemId::Mug => Self::Mug,
            ItemId::Squirter => Self::Squirter,
            _ => return Err(Error::NoItemAtc(item)),
        })
    }
}

/// `TryFrom` impl for converting `AtcId`s to a corresponding `ItemId`
impl TryFrom<AtcId> for ItemId {
    type Error = Error;
    fn try_from(atc: AtcId) -> Result<Self> {
        Ok(match atc {
            AtcId::ChibiBlaster => Self::ChibiBlaster,
            AtcId::ChibiRadar => Self::ChibiRadar,
            AtcId::Toothbrush => Self::Toothbrush,
            AtcId::Spoon => Self::Spoon,
            AtcId::Mug => Self::Mug,
            AtcId::Squirter => Self::Squirter,
            _ => return Err(Error::NoAtcItem(atc)),
        })
    }
}

// Generated using unplug-datagen
include!("gen/atcs.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_atc() {
        let atc = AtcDefinition::get(AtcId::Toothbrush);
        assert_eq!(atc.id, AtcId::Toothbrush);
        assert_eq!(atc.display_name, "Toothbrush");
    }

    #[test]
    fn test_try_atc_from_item() {
        assert_eq!(AtcId::try_from(ItemId::Toothbrush), Ok(AtcId::Toothbrush));
        assert_eq!(AtcId::try_from(ItemId::HotRod), Err(Error::NoItemAtc(ItemId::HotRod)));
    }

    #[test]
    fn test_try_item_from_atc() {
        assert_eq!(ItemId::try_from(AtcId::Toothbrush), Ok(ItemId::Toothbrush));
        assert_eq!(ItemId::try_from(AtcId::ChibiCopter), Err(Error::NoAtcItem(AtcId::ChibiCopter)));
    }
}
