use num_enum::{IntoPrimitive, TryFromPrimitive};

/// The number of real (non-dev) stages.
pub const NUM_REAL_STAGES: i32 = 30;
/// The number of dev stages (shun, hori, ahk, etc.)
pub const NUM_DEV_STAGES: i32 = 9;
/// The total number of stages.
pub const NUM_STAGES: i32 = NUM_REAL_STAGES + NUM_DEV_STAGES;

/// The directory where stages are stored in qp.bin.
pub const STAGE_DIR: &str = "bin/e";
/// The path where globals.bin is stored in qp.bin.
pub const GLOBALS_PATH: &str = "bin/e/globals.bin";

#[derive(Debug)]
pub struct StageDefinition {
    pub id: StageId,
    /// The stage's index, used for various things. Only real stages have this; dev stages use `-1`.
    pub index: i32,
    /// The stage's filename without the directory or extension.
    pub name: &'static str,
}

impl StageDefinition {
    /// Returns the path to the stage in qp.bin.
    pub fn path(&self) -> String {
        format!("{}/{}.bin", STAGE_DIR, self.name)
    }

    /// Returns `true` if this is a dev stage (shun, hori, ahk, etc.).
    pub fn is_dev(&self) -> bool {
        self.index == -1
    }
}

macro_rules! declare_stages {
    {
        $($val:literal => $name:ident { $id:ident, $index:literal, $fname:literal }),*
        $(,)*
    } => {
        #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[derive(IntoPrimitive, TryFromPrimitive)]
        #[repr(i32)]
        pub enum StageId {
            $($id = $val),*
        }

        $(
            pub static $name: StageDefinition = StageDefinition {
                id: StageId::$id,
                index: $index,
                name: $fname,
            };
        )*

        impl StageDefinition {
            pub fn get(id: StageId) -> &'static StageDefinition {
                match id {
                    $(StageId::$id => &$name,)*
                }
            }
        }

        pub static STAGES: &'static [StageId] = &[
            $(StageId::$id),*
        ];
    }
}

declare_stages! {
    0 => DEBUG { Debug, 0, "stage00" },
    1 => KITCHEN { Kitchen, 1, "stage01" },
    2 => FOYER { Foyer, 2, "stage02" },
    3 => BASEMENT { Basement, 3, "stage03" },
    4 => JENNY { Jenny, 4, "stage04" },
    5 => CHIBI_HOUSE { ChibiHouse, 5, "stage05" },
    6 => BEDROOM { Bedroom, 6, "stage06" },
    7 => LIVING_ROOM { LivingRoom, 7, "stage07" },
    8 => STAGE_08 { Stage08, 8, "stage08" },
    9 => BACKYARD { Backyard, 9, "stage09" },
    10 => CREDITS { Credits, 10, "stage10" },
    11 => DRAIN { Drain, 11, "stage11" },
    12 => STAGE_12 { Stage12, 12, "stage12" },
    13 => CHIBI_MANUAL { ChibiManual, 13, "stage13" },
    14 => BIRTHDAY { Birthday, 14, "stage14" },
    15 => STAGE_15 { Stage15, 15, "stage15" },
    16 => UFO { Ufo, 16, "stage16" },
    17 => STAGE_17 { Stage17, 17, "stage17" },
    18 => BEDROOM_PAST { BedroomPast, 18, "stage18" },
    19 => STAGE_19 { Stage19, 19, "stage19" },
    20 => STAGE_20 { Stage20, 20, "stage20" },
    21 => STAGE_21 { Stage21, 21, "stage21" },
    22 => MOTHER_SPIDER { MotherSpider, 22, "stage22" },
    23 => STAGE_23 { Stage23, 23, "stage23" },
    24 => STAGE_24 { Stage24, 24, "stage24" },
    25 => STAGE_25 { Stage25, 25, "stage25" },
    26 => STAGE_26 { Stage26, 26, "stage26" },
    27 => STAGE_27 { Stage27, 27, "stage27" },
    28 => STAGE_28 { Stage28, 28, "stage28" },
    29 => STAGE_29 { Stage29, 29, "stage29" },

    100 => SHUN { Shun, -1, "shun" },
    101 => HORI { Hori, -1, "hori" },
    102 => AHK { Ahk, -1, "ahk" },
    103 => JUNKO { Junko, -1, "junko" },
    104 => SAYOKO { Sayoko, -1, "sayoko" },
    105 => MORY { Mory, -1, "mory" },
    106 => RYOSUKE { Ryosuke, -1, "ryosuke" },
    107 => TAKANABE { Takanabe, -1, "takanabe" },
    108 => MARIKO { Mariko, -1, "mariko" },
}
