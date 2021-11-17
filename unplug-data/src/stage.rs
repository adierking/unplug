use num_enum::{IntoPrimitive, TryFromPrimitive};

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
#[derive(Debug)]
pub struct StageDefinition {
    /// The stage's ID.
    pub id: Stage,
    /// The the name of the stage file without the filename or extension.
    pub name: &'static str,
}

impl StageDefinition {
    /// Retrieves the definition corresponding to a `Stage`.
    pub fn get(id: Stage) -> &'static StageDefinition {
        let index = i32::from(id);
        if index >= FIRST_DEV_STAGE {
            &STAGES[(index - FIRST_DEV_STAGE) as usize + NUM_MAIN_STAGES]
        } else {
            &STAGES[index as usize]
        }
    }

    /// Gets the path to the stage file within the ISO.
    pub fn path(&self) -> String {
        format!("{}/{}{}", STAGE_DIR, self.name, STAGE_EXT)
    }

    /// Tries to find the stage definition whose name matches `name`.
    pub fn find(name: &str) -> Option<&'static StageDefinition> {
        STAGES.iter().find(|s| s.name == name)
    }

    /// Returns `true` if this is a dev stage (shun, hori, ahk, etc.).
    pub fn is_dev(&self) -> bool {
        i32::from(self.id) >= FIRST_DEV_STAGE
    }
}

macro_rules! declare_stages {
    {
        $($val:literal => $id:ident { $name:literal }),*
        $(,)*
    } => {
        #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(IntoPrimitive, TryFromPrimitive)]
        #[repr(i32)]
        pub enum Stage {
            $($id = $val),*
        }

        pub static STAGES: &'static [StageDefinition] = &[
            $(
                StageDefinition {
                    id: Stage::$id,
                    name: $name,
                }
            ),*
        ];
    }
}

// Generated using unplug-datagen
include!("gen/stages.inc.rs");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_stage_main() {
        let stage = StageDefinition::get(Stage::LivingRoom);
        assert_eq!(stage.id, Stage::LivingRoom);
        assert_eq!(stage.name, "stage07");
        assert_eq!(stage.path(), "bin/e/stage07.bin");
        assert!(!stage.is_dev());
    }

    #[test]
    fn test_get_stage_dev() {
        let stage = StageDefinition::get(Stage::Ahk);
        assert_eq!(stage.id, Stage::Ahk);
        assert_eq!(stage.name, "ahk");
        assert_eq!(stage.path(), "bin/e/ahk.bin");
        assert!(stage.is_dev());
    }

    #[test]
    fn test_find_stage() {
        assert_eq!(StageDefinition::find("stage07").unwrap().id, Stage::LivingRoom);
        assert_eq!(StageDefinition::find("ahk").unwrap().id, Stage::Ahk);
        assert!(StageDefinition::find("stage").is_none());
    }
}
