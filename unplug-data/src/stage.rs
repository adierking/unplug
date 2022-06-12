use crate::private::Sealed;
use crate::resource::{Resource, ResourceIterator};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::fmt::{self, Debug, Formatter};

/// The number of main (non-dev) stages.
pub const NUM_MAIN_STAGES: usize = 30;
/// The number of dev stages (shun, hori, ahk, etc.)
pub const NUM_DEV_STAGES: usize = 9;
/// The ID of the first dev stage.
pub const FIRST_DEV_STAGE: i32 = 100;
/// The total number of stages.
pub const NUM_STAGES: usize = NUM_MAIN_STAGES + NUM_DEV_STAGES;

/// The path where globals.bin is stored in qp.bin.
pub const GLOBALS_PATH: &str = "bin/e/globals.bin";

/// The directory where stages are stored.
const STAGE_DIR: &str = "bin/e";
/// The stage file extension.
const STAGE_EXT: &str = ".bin";

/// Metadata describing a stage.
struct Metadata {
    /// The corresponding stage ID.
    id: Stage,
    /// The name of the stage file without the filename or extension.
    name: &'static str,
    /// The stage's title in the English version of the game.
    title: &'static str,
}

macro_rules! declare_stages {
    {
        $($val:literal => $id:ident { $name:literal, $title:literal }),*
        $(,)*
    } => {
        /// A stage ID.
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(IntoPrimitive, TryFromPrimitive)]
        #[repr(i32)]
        pub enum Stage {
            $($id = $val),*
        }

        const METADATA: &'static [Metadata] = &[
            $(
                Metadata {
                    id: Stage::$id,
                    name: $name,
                    title: $title,
                }
            ),*
        ];
    }
}

impl Stage {
    /// Returns an iterator over all stage IDs.
    pub fn iter() -> ResourceIterator<Self> {
        ResourceIterator::new()
    }

    /// Tries to find the stage whose name matches `name`.
    pub fn find(name: &str) -> Option<Stage> {
        Self::iter().find(|s| s.name() == name)
    }

    /// Returns the name of the stage file without the filename or extension.
    pub fn name(self) -> &'static str {
        self.meta().name
    }

    /// Returns the stage's title in the English version of the game.
    pub fn title(self) -> &'static str {
        self.meta().title
    }

    /// Gets the path to the stage file within the ISO.
    pub fn path(self) -> String {
        format!("{}/{}{}", STAGE_DIR, self.name(), STAGE_EXT)
    }

    /// Returns `true` if this is a dev stage (shun, hori, ahk, etc.).
    pub fn is_dev(self) -> bool {
        i32::from(self) >= FIRST_DEV_STAGE
    }

    fn meta(self) -> &'static Metadata {
        let index = i32::from(self);
        let dev_index = index - FIRST_DEV_STAGE;
        if dev_index >= 0 {
            &METADATA[dev_index as usize + NUM_MAIN_STAGES]
        } else {
            &METADATA[index as usize]
        }
    }
}

impl Debug for Stage {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "<{}>", self.name())
    }
}

impl Sealed for Stage {}

impl Resource for Stage {
    const COUNT: usize = NUM_STAGES;
    fn at(index: usize) -> Self {
        METADATA[index].id
    }
}

// Generated using unplug-datagen
include!("gen/stages.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_stage_main() {
        let stage = Stage::LivingRoom;
        assert_eq!(stage.name(), "stage07");
        assert_eq!(stage.title(), "Living Room");
        assert_eq!(stage.path(), "bin/e/stage07.bin");
        assert!(!stage.is_dev());
        assert_eq!(format!("{:?}", stage), "<stage07>");
    }

    #[test]
    fn test_get_stage_dev() {
        let stage = Stage::Ahk;
        assert_eq!(stage.name(), "ahk");
        assert_eq!(stage.path(), "bin/e/ahk.bin");
        assert!(stage.is_dev());
        assert_eq!(format!("{:?}", stage), "<ahk>");
    }

    #[test]
    fn test_find_stage() {
        assert_eq!(Stage::find("stage07"), Some(Stage::LivingRoom));
        assert_eq!(Stage::find("ahk"), Some(Stage::Ahk));
        assert_eq!(Stage::find("stage"), None);
    }

    #[test]
    fn test_iter() {
        let stages = Stage::iter().collect::<Vec<_>>();
        assert_eq!(stages.len(), NUM_STAGES);
        assert_eq!(stages[0], Stage::Debug);
        assert_eq!(stages[29], Stage::Stage29);
        assert_eq!(stages[30], Stage::Shun);
        assert_eq!(stages[38], Stage::Mariko);
    }
}
