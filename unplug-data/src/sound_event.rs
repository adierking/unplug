use super::sound_bank::SOUND_BANKS;
use num_enum::{IntoPrimitive, TryFromPrimitive};

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
        #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
    /// Calculates the event's global index.
    pub fn index(&self) -> usize {
        let id = u32::from(*self);
        let bank = &SOUND_BANKS[(id >> 16) as usize];
        (bank.event_base + (id & 0xffff)) as usize
    }
}

// Generated using unplug-datagen
include!("gen/sound_events.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_sound_event() {
        let event = SoundEventDefinition::get(SoundEvent::VoiceHelpMe);
        assert_eq!(event.id, SoundEvent::VoiceHelpMe);
        assert_eq!(event.name, "VOICE_HELP_ME");

        let event = SoundEventDefinition::get(SoundEvent::NpcSanpooLaugh);
        assert_eq!(event.id, SoundEvent::NpcSanpooLaugh);
        assert_eq!(event.name, "NPC_SANPOO_LAUGH");
    }
}
