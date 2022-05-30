use anyhow::{bail, Result};
use unplug::event::msg::{Color, Icon, Voice};

/// Trait for an enum which can be converted to/from ID strings.
pub trait IdString: Sized {
    fn to_id(&self) -> &'static str;

    fn try_from_id(id: &str) -> Result<Self>;
}

/// Macro for quickly implementing `IdString` for several enums.
macro_rules! id_strings {
    {
        $($enum:path {
            $($name:ident = $str:literal),*
            $(,)*
        })*
    } => {
        $(impl IdString for $enum {
            fn to_id(&self) -> &'static str {
                match self {
                    $(Self::$name => $str,)*
                }
            }

            fn try_from_id(id: &str) -> Result<Self> {
                Ok(match id {
                    $($str => Self::$name,)*
                    _ => bail!("Unrecognized {} name: {}", stringify!($enum), id),
                })
            }
        })*
    }
}

id_strings! {
    Color {
        White = "white",
        Gray = "gray",
        DarkGray = "dark-gray",
        Cyan = "cyan",
        Lime = "lime",
        Blue = "blue",
        Magenta = "magenta",
        Red = "red",
        Yellow = "yellow",
        Orange = "orange",
        Reset = "reset",
    }

    Icon {
        Analog = "analog",
        Up = "up",
        Right = "right",
        Down = "down",
        Left = "left",
        A = "a",
        B = "b",
        C = "c",
        X = "x",
        Y = "y",
        Z = "z",
        L = "l",
        R = "r",
        Start = "start",
        Moolah = "moolah",
        Yes = "yes",
        No = "no",
    }

    Voice {
        None = "none",
        Telly = "telly",
        Frog = "frog",
        Jenny = "jenny",
        Papa = "papa",
        Mama = "mama",
        Unk5 = "unk5",
        Unk6 = "unk6",
        Drake = "drake",
        Captain = "captain",
        Soldier = "soldier",
        Peekoe = "peekoe",
        Sophie = "sophie",
        News1 = "news1",
        Sarge = "sarge",
        JennyFrog = "jenny-frog",
        Primo = "primo",
        Prongs = "prongs",
        Et = "et",
        Funky = "funky",
        Dinah = "dinah",
        Pitts = "pitts",
        Mort = "mort",
        Sunshine = "sunshine",
        SunshineHungry = "sunshine-hungry",
        DinahToothless = "dinah-toothless",
        Fred = "fred",
        Freida = "freida",
        Tao = "tao",
        Ufo = "ufo",
        Underwater = "underwater",
        Eggplant = "eggplant",
        Phillies = "phillies",
        Gebah = "gebah",
        News2 = "news2",
    }
}
