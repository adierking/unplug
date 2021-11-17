use num_enum::{IntoPrimitive, TryFromPrimitive};

pub const NUM_SOUND_BANKS: usize = 25;

const BANK_DIR: &str = "qp";
const BANK_EXT: &str = ".ssm";

/// Metadata describing a sound bank.
#[derive(Debug)]
pub struct SoundBankDefinition {
    /// The bank's corresponding `SoundBank`.
    pub id: SoundBank,
    /// The base index for sounds in the bank.
    pub sound_base: u32,
    /// The base index for sound events in the bank.
    pub event_base: u32,
    /// The name of the sound bank without the directory or file extension.
    pub name: &'static str,
}

impl SoundBankDefinition {
    /// Retrieves the definition corresponding to a `SoundBank`.
    pub fn get(id: SoundBank) -> &'static SoundBankDefinition {
        &SOUND_BANKS[u16::from(id) as usize]
    }

    /// Gets the path to the bank file within the ISO.
    pub fn path(&self) -> String {
        format!("{}/{}{}", BANK_DIR, self.name, BANK_EXT)
    }
}

/// Macro used in the generated sound bank list.
macro_rules! declare_sound_banks {
    {
        $($index:literal => $id:ident { $sbase:literal, $ebase:literal, $name:literal }),*
        $(,)*
    } => {
        #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(IntoPrimitive, TryFromPrimitive)]
        #[repr(u16)]
        pub enum SoundBank {
            $($id = $index),*
        }

        pub static SOUND_BANKS: &[SoundBankDefinition] = &[
            $(
                SoundBankDefinition {
                    id: SoundBank::$id,
                    sound_base: $sbase,
                    event_base: $ebase,
                    name: $name,
                }
            ),*
        ];
    }
}

// Generated using unplug-datagen
include!("gen/sound_banks.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_sound_bank() {
        let bank = SoundBankDefinition::get(SoundBank::Ufo);
        assert_eq!(bank.id, SoundBank::Ufo);
        assert_eq!(bank.sound_base, 0x2f9);
        assert_eq!(bank.event_base, 0x2fe);
        assert_eq!(bank.name, "sfx_ufo");
        assert_eq!(bank.path(), "qp/sfx_ufo.ssm");
    }
}
