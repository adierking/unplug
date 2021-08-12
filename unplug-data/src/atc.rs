use num_enum::{IntoPrimitive, TryFromPrimitive};

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
        &ATCS[i16::from(id) as usize]
    }
}

// Macro used in the generated ATC list
macro_rules! declare_atcs {
    {
        $($index:literal => $id:ident { $display_name:literal }),*
        $(,)*
    } => {
        #[derive(Debug, Copy, Clone, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
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
}
