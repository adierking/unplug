use super::sfx_group::{SfxGroup, SfxGroupDefinition, SFX_GROUPS};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::fmt::{self, Debug};

/// ISO path to the main sound event bank.
pub const EVENT_BANK_PATH: &str = "qp/sfx_sample.sem";

/// Metadata describing a sound event.
#[derive(Debug)]
pub struct SoundEventDefinition {
    /// The event's corresponding `SoundEvent`.
    pub id: SoundEvent,
    /// The event's name.
    pub name: &'static str,
}

impl SoundEventDefinition {
    /// Retrieves the definition corresponding to a `SoundEvent`.
    pub fn get(id: SoundEvent) -> &'static SoundEventDefinition {
        &SOUND_EVENTS[id.index()]
    }
}

/// Macro used in the generated sound event list.
macro_rules! declare_sound_events {
    {
        $($index:literal => $id:ident { $name:literal }),*
        $(,)*
    } => {
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(IntoPrimitive, TryFromPrimitive)]
        #[repr(u32)]
        pub enum SoundEvent {
            $($id = $index),*
        }

        pub static SOUND_EVENTS: &[SoundEventDefinition] = &[
            $(
                SoundEventDefinition {
                    id: SoundEvent::$id,
                    name: $name,
                }
            ),*
        ];
    }
}

impl SoundEvent {
    /// Returns the event's `SfxGroup`.
    pub fn group(&self) -> SfxGroup {
        let id = u32::from(*self);
        SFX_GROUPS[(id >> 16) as usize].id
    }

    /// Calculates the event's global index.
    pub fn index(&self) -> usize {
        let group = SfxGroupDefinition::get(self.group());
        let id = u32::from(*self);
        (group.first_material + (id & 0xffff)) as usize
    }
}

impl Debug for SoundEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{}>", SoundEventDefinition::get(*self).name)
    }
}

// Generated using unplug-datagen
include!("gen/sound_events.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_sound_event() {
        for event in SOUND_EVENTS {
            let actual = SoundEventDefinition::get(event.id);
            assert_eq!(actual.id, event.id);
        }
    }

    #[test]
    fn test_get_sound_event_group() {
        assert_eq!(SoundEvent::VoiceHelpMe.group(), SfxGroup::Sample);
        assert_eq!(SoundEvent::KitchenOil.group(), SfxGroup::Kitchen);
    }
}
