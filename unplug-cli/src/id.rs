use anyhow::{bail, Result};
use unplug::data::atc::AtcId;
use unplug::data::item::ItemId;
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
                    _ => bail!("Unrecognized {} ID: {}", stringify!($enum), id),
                })
            }
        })*
    }
}

id_strings! {
    AtcId {
        ChibiCopter = "chibi-copter",
        ChibiBlaster = "chibi-blaster",
        ChibiRadar = "chibi-radar",
        Toothbrush = "toothbrush",
        Spoon = "spoon",
        Mug = "mug",
        Squirter = "squirter",
        Unk8 = "unk8",
    }

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

    ItemId {
        FrogRing = "frog-ring",
        Pen = "pen",
        Unk2 = "unk2",
        Unk3 = "unk3",
        Unk4 = "unk4",
        Unk5 = "unk5",
        Unk6 = "unk6",
        Unk7 = "unk7",
        GigaBattery = "giga-battery",
        Unk9 = "unk9",
        Unk10 = "unk10",
        Unk11 = "unk11",
        Unk12 = "unk12",
        Unk13 = "unk13",
        DogBone = "dog-bone",
        Unk15 = "unk15",
        Toothbrush = "toothbrush",
        Capsule17 = "capsule17",
        Wastepaper = "wastepaper",
        CookieCrumbs = "cookie-crumbs",
        Unk20 = "unk20",
        Spoon = "spoon",
        Mug = "mug",
        Rose = "rose",
        DrakeRedcrestSuit = "drake-redcrest-suit",
        TaoSuit = "tao-suit",
        FrogSuit = "frog-suit",
        Capsule27 = "capsule27",
        Capsule28 = "capsule28",
        Capsule29 = "capsule29",
        Pajamas = "pajamas",
        SuperChibiRoboSuit = "super-chibi-robo-suit",
        TraumaSuit = "trauma-suit",
        Unk33 = "unk33",
        GhostSuit = "ghost-suit",
        TonpyA = "tonpy-a",
        TonpyB = "tonpy-b",
        TonpyC = "tonpy-c",
        Unk38 = "unk38",
        Unk39 = "unk39",
        Unk40 = "unk40",
        TreasureMapA = "treasure-map-a",
        TreasureMapB = "treasure-map-b",
        TreasureMapC = "treasure-map-c",
        SugarCube = "sugar-cube",
        Cookie = "cookie",
        Unk46 = "unk46",
        Unk47 = "unk47",
        GigaCharger = "giga-charger",
        PopperTrash = "popper-trash",
        EmptyBottle = "empty-bottle",
        BrokenBottleA = "broken-bottle-a",
        BrokenBottleB = "broken-bottle-b",
        ChargeChip = "charge-chip",
        RangeChip = "range-chip",
        ToyReceipt = "toy-receipt",
        Squirter = "squirter",
        MomsLetter = "moms-letter",
        DBattery = "d-battery",
        CBattery = "c-battery",
        AABattery = "aa-battery",
        RedShoe = "red-shoe",
        AlienEarChip = "alien-ear-chip",
        Unk63 = "unk63",
        Unk64 = "unk64",
        Unk65 = "unk65",
        Unk66 = "unk66",
        Unk67 = "unk67",
        Unk68 = "unk68",
        Unk69 = "unk69",
        TonpyE = "tonpy-e",
        TonpyF = "tonpy-f",
        TonpyG = "tonpy-g",
        TonpyH = "tonpy-h",
        TonpyI = "tonpy-i",
        TonpyJ = "tonpy-j",
        TonpyK = "tonpy-k",
        TonpyL = "tonpy-l",
        TonpyM = "tonpy-m",
        TonpyN = "tonpy-n",
        TonpyO = "tonpy-o",
        TonpyP = "tonpy-p",
        TonpyQ = "tonpy-q",
        TonpyR = "tonpy-r",
        TonpyS = "tonpy-s",
        TonpyT = "tonpy-t",
        TonpyU = "tonpy-u",
        Timer5 = "timer5",
        DinahTeeth = "dinah-teeth",
        Timer10 = "timer10",
        Timer15 = "timer15",
        Twig = "twig",
        PinkFlowerSeed = "pink-flower-seed",
        BlueFlowerSeed = "blue-flower-seed",
        WhiteFlowerSeed = "white-flower-seed",
        BbPen = "bb-pen",
        Ink = "ink",
        DriedFlower = "dried-flower",
        Gunpowder = "gunpowder",
        SnorkleGoggles = "snorkle-goggles",
        LoveLetter = "love-letter",
        DogTags = "dog-tags",
        TicketStub = "ticket-stub",
        Bandage = "bandage",
        Block = "block",
        DrakeRedcrestAlbum = "drake-redcrest-album",
        RecordFf = "record-ff",
        SpaceScrambler = "space-scrambler",
        FreeRangersPhoto = "free-rangers-photo",
        GigaRobosLeftLeg = "giga-robos-left-leg",
        CircuitSchematic = "circuit-schematic",
        GigaBatteryFull = "giga-battery-full",
        SmallHandkerchief = "small-handkerchief",
        PinkFlower = "pink-flower",
        TheScurvySplinter = "the-scurvy-splinter",
        OldBoxers = "old-boxers",
        OutdatedScarf = "outdated-scarf",
        BlueBlock = "blue-block",
        PurpleBlock = "purple-block",
        WhiteBlock = "white-block",
        GreenBlock = "green-block",
        YellowBlock = "yellow-block",
        RedBlock = "red-block",
        HotRod = "hot-rod",
        DadsWeddingRing = "dads-wedding-ring",
        Weeds = "weeds",
        BlockLayout = "block-layout",
        PassedOutFrog = "passed-out-frog",
        Unk128 = "unk128",
        Unk129 = "unk129",
        RoukaTaoBag = "tao-bag",
        ChibiBlaster = "chibi-blaster",
        ChibiRadar = "chibi-radar",
        Tamagotchi = "tamagotchi",
        Primopuel = "primopuel",
        EmptyCan = "empty-can",
        CandyWrapper = "candy-wrapper",
        CandyBag = "candy-bag",
        CookieBox = "cookie-box",
        SuperEggplant = "super-eggplant",
        LegendaryFlowerSeed = "legendary-flower-seed",
        Unk141 = "unk141",
        Unk142 = "unk142",
        Unk143 = "unk143",
        Unk144 = "unk144",
        Unk145 = "unk145",
        Unk146 = "unk146",
        RedCrayon = "red-crayon",
        BlueCrayon = "blue-crayon",
        YellowCrayon = "yellow-crayon",
        GreenCrayon = "green-crayon",
        PurpleCrayon = "purple-crayon",
        NectarFlowerSeed = "nectar-flower-seed",
        Philly = "philly",
        FreakyPhil = "freaky-phil",
        FunkySeed = "funky-seed",
        BlueFlowers = "blue-flowers",
        WhiteFlowers = "white-flowers",
        ChibiBattery = "chibi-battery",
    }

    Voice {
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
