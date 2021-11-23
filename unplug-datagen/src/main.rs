#![allow(clippy::trivial_regex)]
#![warn(
    absolute_paths_not_starting_with_crate,
    elided_lifetimes_in_paths,
    explicit_outlives_requirements,
    trivial_casts,
    trivial_numeric_casts,
    unconditional_recursion,
    unreachable_patterns,
    unreachable_pub,
    unused_import_braces,
    unused_lifetimes,
    unused_must_use,
    unused_qualifications,
    variant_size_differences
)]

use anyhow::{anyhow, bail, Error, Result};
use byteorder::{ReadBytesExt, BE};
use lazy_static::lazy_static;
use log::{debug, error, info, trace, warn};
use num_enum::TryFromPrimitive;
use regex::{Regex, RegexSet};
use simplelog::{Color, ColorChoice, ConfigBuilder, Level, LevelFilter, TermLogger, TerminalMode};
use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::env;
use std::ffi::CString;
use std::fmt::Write as FmtWrite;
use std::fmt::{self, Debug};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::process;
use unplug::audio::metadata::sem::{Command, EventBank};
use unplug::audio::transport::Brsar;
use unplug::common::{NonNoneList, ReadFrom, ReadOptionFrom};
use unplug::data::stage::GLOBALS_PATH;
use unplug::dvd::{ArchiveReader, DiscStream, DolHeader, OpenFile};
use unplug::globals::metadata::{Atc, Item, Stage, Suit};
use unplug::globals::GlobalsReader;

const MAIN_OBJECTS_ADDR: u32 = 0x8021c70c;
const NUM_MAIN_OBJECTS: usize = 1162;

const INTERNAL_OBJECTS_ADDR: u32 = 0x80223690;
const NUM_INTERNAL_OBJECTS: usize = 36;
const INTERNAL_OBJECTS_BASE_ID: u32 = 10000;

const NUM_SUITS: usize = 8;
const SUIT_ITEMS_ADDR: u32 = 0x8020a17c;
const SUIT_ORDER_ADDR: u32 = 0x800bc318;
const ADDI_OPCODE: u32 = 14;
const STRIP_SUIT_LABEL: &str = "Suit";

const STAGE_LIST_ADDR: u32 = 0x802244e4;
const FIRST_DEV_STAGE: i32 = 100;

const MUSIC_LIST_ADDR: u32 = 0x802108a8;
const NUM_MUSIC: usize = 109;
const MUSIC_EXT: &str = ".hps";

const QP_PATH: &str = "qp.bin";

const INTERNAL_PREFIX: &str = "Internal";
const UNKNOWN_PREFIX: &str = "Unk";

const NUM_ITEMS: usize = 159;
const STRIP_ITEM_LABEL: &str = "Item";

/// Names of sound banks within the ISO and their corresponding BRSAR groups. This does not include
/// `sfx_hori` because it has a bad base index and cannot be loaded.
const SOUND_BANKS: &[(&str, &str)] = &[
    ("sfx_army", "GROUP_ARMY"),
    ("sfx_bb", "GROUP_BB"),
    ("sfx_concert", "GROUP_CONCERT"),
    ("sfx_ending", "GROUP_ENDING"),
    ("sfx_gicco", "GROUP_GICCO"),
    ("sfx_hock", "GROUP_HOCK"),
    ("sfx_jennyroom", "GROUP_JENNYROOM"),
    ("sfx_kaeru", "GROUP_KAERU"),
    ("sfx_kitchen", "GROUP_KITCHEN"),
    ("sfx_manual", "GROUP_MANUAL"),
    ("sfx_martial", "GROUP_MARTIAL"),
    ("sfx_papamama", "GROUP_PAPAMAMA"),
    ("sfx_pipe", "GROUP_PIPE"),
    ("sfx_sample", "GROUP_DEF"),
    ("sfx_sanpoo", "GROUP_SANPOO"),
    ("sfx_souko", "GROUP_SOUKO"),
    ("sfx_stage02", "GROUP_STAGE02"),
    ("sfx_stage05", "GROUP_STAGE05"),
    ("sfx_stage07", "GROUP_STAGE07"),
    ("sfx_trex", "GROUP_TREX"),
    ("sfx_ufo", "GROUP_UFO"),
    ("sfx_uraniwa", "GROUP_URANIWA"),
    ("sfx_uraniwa_ambient1", "GROUP_URANIWA_AMBIENT1"),
    ("sfx_uraniwa_ambient2", "GROUP_URANIWA_AMBIENT2"),
    ("sfx_uraniwa_ambient3", "GROUP_URANIWA_AMBIENT3"),
];
const SOUND_BANK_DIR: &str = "qp";
const SOUND_BANK_PREFIX: &str = "sfx_";
const SOUND_BANK_EXT: &str = ".ssm";
const SOUND_EVENTS_PATH: &str = "qp/sfx_sample.sem";

const UNPLUG_DATA_PATH: &str = "unplug-data";
const SRC_DIR_NAME: &str = "src";
const TEST_FILE_NAME: &str = "lib.rs";
const OUTPUT_DIR_NAME: &str = "gen";

const GEN_HEADER: &str = "// Generated with unplug-datagen. DO NOT EDIT.\n\
                          // To regenerate: cargo run -p unplug-datagen -- <iso path>\n\n";
const GEN_HEADER_BRSAR: &str = "// Generated with unplug-datagen. DO NOT EDIT.\n\
                                // To regenerate: cargo run -p unplug-datagen -- <iso path> \
                                --brsar <cb_robo.brsar path>\n\n";

const OBJECTS_FILE_NAME: &str = "objects.inc.rs";
const OBJECTS_HEADER: &str = "declare_objects! {\n";
const OBJECTS_FOOTER: &str = "}\n";

const ITEMS_FILE_NAME: &str = "items.inc.rs";
const ITEMS_HEADER: &str = "declare_items! {\n";
const ITEMS_FOOTER: &str = "}\n";
const ITEM_FLAG_UNUSED: &str = "UNUSED";

const ATCS_FILE_NAME: &str = "atcs.inc.rs";
const ATCS_HEADER: &str = "declare_atcs! {\n";
const ATCS_FOOTER: &str = "}\n";

const SUITS_FILE_NAME: &str = "suits.inc.rs";
const SUITS_HEADER: &str = "declare_suits! {\n";
const SUITS_FOOTER: &str = "}\n";

const STAGES_FILE_NAME: &str = "stages.inc.rs";
const STAGES_HEADER: &str = "declare_stages! {\n";
const STAGES_FOOTER: &str = "}\n";

const MUSIC_FILE_NAME: &str = "music.inc.rs";
const MUSIC_HEADER: &str = "declare_music! {\n";
const MUSIC_FOOTER: &str = "}\n";

const SOUND_BANKS_FILE_NAME: &str = "sound_banks.inc.rs";
const SOUND_BANKS_HEADER: &str = "declare_sound_banks! {\n";
const SOUND_BANKS_FOOTER: &str = "}\n";

const SOUND_EVENTS_FILE_NAME: &str = "sound_events.inc.rs";
const SOUND_EVENTS_HEADER: &str = "declare_sound_events! {\n";
const SOUND_EVENTS_FOOTER: &str = "}\n";

const SOUNDS_FILE_NAME: &str = "sounds.inc.rs";
const SOUNDS_HEADER: &str = "declare_sounds! {\n";
const SOUNDS_FOOTER: &str = "}\n";

lazy_static! {
    /// Each object's label will be matched against these regexes in order. The first match found
    /// will be replaced by the associated string.
    static ref OBJECT_LABEL_FIXUPS: Vec<(Regex, &'static str)> = vec![
        // The timers are all named the same except for their item ID, so map them to something more
        // useful (and which doesn't start with a number when "All" is removed)
        (Regex::new(r"^All1DayTimer87$").unwrap(), "ItemTimer5"),
        (Regex::new(r"^All1DayTimer89$").unwrap(), "ItemTimer10"),
        (Regex::new(r"^All1DayTimer90$").unwrap(), "ItemTimer15"),

        // Fix objects that collide with objects of different classes
        (Regex::new(r"^ChibihouseDenti2_48$").unwrap(), "ItemChibiHouseDenti2"),
        (Regex::new(r"^ChibihouseDenti2_1106$").unwrap(), "ChibiHouseDenti2"),

        // Make several names less verbose
        (Regex::new(r"^All").unwrap(), ""),
        (Regex::new(r"^ItemItem").unwrap(), "Item"),
        (Regex::new(r"^ItemTonpyItem").unwrap(), "ItemTonpy"),
        (Regex::new(r"^JennyJenny").unwrap(), "Jenny"),
        (Regex::new(r"^LivingLiving").unwrap(), "Living"),
        (Regex::new(r"^TestTest").unwrap(), "Test"),
        (Regex::new(r"TitleIconIcon").unwrap(), "TitleIcon"),
        (Regex::new(r"^UfoUfo").unwrap(), "Ufo"),

        // Clean up capitalization
        (Regex::new(r"Chibihouse").unwrap(), "ChibiHouse"),
    ];

    /// Items whose object labels match these will have their labels overridden.
    static ref ITEM_LABEL_OVERRIDES: HashMap<String, &'static str> = vec![
        // Make capitalization nicer
        ("ItemDenchi3".into(), "AABattery"),
        // Make this shorter and fix spelling :)
        ("ItemGoggle".into(), "Snorkel"),
        // There are two broken bottle items and they both have the display name "Broken Bottle"
        ("SoukoWineBottleA".into(), "BrokenBottleA"),
        ("SoukoWineBottleB".into(), "BrokenBottleB"),
    ].into_iter().collect();

    /// Find-and-replace pairs for music paths
    static ref MUSIC_PATH_FIXUPS: Vec<(Regex, &'static str)> = vec![
        // Fix capitalization
        (Regex::new("/nwing.hps$").unwrap(), "/Nwing.hps"),
    ];

    /// Music files whose names match these will have their labels overridden.
    static ref MUSIC_LABEL_OVERRIDES: HashMap<String, &'static str> = vec![
        ("Nwing".into(), "NWing"),
        ("toyrex".into(), "ToyRex"),
        ("UFOBGM".into(), "UfoBgm"),
    ].into_iter().collect();

    /// Regexes matching sound names to discard because they are duplicates.
    static ref DUPLICATE_SOUND_DISCARDS: RegexSet = RegexSet::new(&[
        r"^none$",
        r"^robo_waik$",
        r"^robo_charge2$",
        r"^system_ng$",
        r"^robo_syringe_in$",
        r"^npc_army_foot\d$",
    ]).unwrap();
}

fn init_logging() {
    let config = ConfigBuilder::new()
        .set_thread_level(LevelFilter::Off)
        .set_target_level(LevelFilter::Trace)
        .set_time_format_str("%T%.3f")
        .set_level_color(Level::Info, Some(Color::Green))
        .build();
    TermLogger::init(LevelFilter::Debug, config, TerminalMode::Stderr, ColorChoice::Auto).unwrap();
}

/// Command-line options.
struct Options {
    /// Path to the ISO to load.
    iso: PathBuf,
    /// Path to cb_robo.brsar.
    brsar: Option<PathBuf>,
}

fn parse_options() -> Result<Options> {
    let mut iso: Option<PathBuf> = None;
    let mut brsar: Option<PathBuf> = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_ref() {
            "-h" | "--help" => {
                usage();
                process::exit(0);
            }
            "--brsar" => {
                let path = args.next().ok_or_else(|| anyhow!("--brsar: path expected"))?;
                brsar = Some(PathBuf::from(path));
            }
            arg if iso.is_none() => iso = Some(PathBuf::from(arg)),
            arg => bail!("unrecognized argument: {}", arg),
        }
    }
    Ok(Options { iso: iso.ok_or_else(|| anyhow!("ISO path expected"))?, brsar })
}

fn usage() {
    eprintln!("Usage: cargo run -p unplug-datagen -- <iso> [--brsar <path>]\n");
    eprintln!("To generate sound names, --brsar must be provided with a path to cb_robo.brsar");
    eprintln!("from the Wii release of the game (R24J01).\n");
}

/// The raw representation of an object in the executable.
#[derive(Debug, Copy, Clone, Default)]
struct RawObjectDefinition {
    model_addr: u32,
    _unk_04: u32,
    _unk_08: u32,
    _unk_0c: u32,
    _unk_10: u32,
    class: u16,
    subclass: u16,
}

impl<R: Read + ?Sized> ReadFrom<R> for RawObjectDefinition {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self {
            model_addr: reader.read_u32::<BE>()?,
            _unk_04: reader.read_u32::<BE>()?,
            _unk_08: reader.read_u32::<BE>()?,
            _unk_0c: reader.read_u32::<BE>()?,
            _unk_10: reader.read_u32::<BE>()?,
            class: reader.read_u16::<BE>()?,
            subclass: reader.read_u16::<BE>()?,
        })
    }
}

/// An enum label.
#[derive(Clone, Default)]
struct Label(String);

impl Label {
    /// Creates a `Label` from a string, changing the capitalization to PascalCase and discarding
    /// unusable characters.
    fn from_string_lossy(path: &str) -> Self {
        let mut name = String::new();
        let mut capitalize = true;
        for ch in path.chars() {
            if ch.is_alphabetic() {
                let capitalized: String = if capitalize {
                    ch.to_uppercase().collect()
                } else {
                    ch.to_lowercase().collect()
                };
                name.push_str(&capitalized);
                capitalize = false;
            } else if ch.is_digit(10) {
                name.push(ch);
                capitalize = true;
            } else if ch != '\'' {
                capitalize = true;
            }
        }
        Self(name)
    }

    /// Appends a discriminator ID to this label.
    fn append_discriminator(&mut self, id: usize) {
        if self.0.ends_with(|c: char| c.is_digit(10)) {
            self.0.push('_');
        }
        write!(self.0, "{}", id).unwrap();
    }
}

impl Debug for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Object class IDs.
#[derive(Debug, Copy, Clone, PartialEq, Eq, TryFromPrimitive)]
#[repr(u16)]
enum ObjectClass {
    Camera = 0,
    Light = 1,
    Chr = 2,
    Map = 3,
    Actor2 = 4,
    Actor3 = 5,
    Sound = 6,
    Coin = 7,
    Item = 8,
    Leticker = 9,
    ActorToy = 10,
    ActorJenny = 11,
    ActorMama = 12,
    ActorPapa = 13,
    ActorTao = 14,
    ActorDeca = 15,
    Army = 16,
    Spider = 17,
    SpiderSmall = 18,
    SpiderBoss = 19,
    Dust = 20,
    HBox = 21,
    Free = 22,
    Unk17 = 23,
    Plug = 24,
}

/// Representation of an object which is written to the generated source.
#[derive(Debug, Clone)]
struct ObjectDefinition {
    id: u32,
    label: Label,
    model_path: String,
    class: ObjectClass,
    subclass: u16,
}

/// Reads `count` objects from `address` in the executable.
fn read_objects(
    dol: &DolHeader,
    reader: &mut (impl Read + Seek),
    address: u32,
    count: usize,
) -> Result<Vec<ObjectDefinition>> {
    let offset = dol.address_to_offset(address)? as u64;
    reader.seek(SeekFrom::Start(offset))?;
    let mut raw_objects = vec![RawObjectDefinition::default(); count];
    RawObjectDefinition::read_all_from(reader, &mut raw_objects)?;

    let mut objects: Vec<ObjectDefinition> = Vec::with_capacity(count);
    for (i, raw) in raw_objects.iter().enumerate() {
        let model_path = read_model_path(dol, reader, raw.model_addr)?;
        let label = Label::from_string_lossy(&model_path);
        objects.push(ObjectDefinition {
            id: i as u32,
            label,
            model_path,
            class: ObjectClass::try_from(raw.class)?,
            subclass: raw.subclass,
        });
    }
    Ok(objects)
}

/// Reads an object's model path from the executable.
fn read_model_path(
    dol: &DolHeader,
    reader: &mut (impl Read + Seek),
    address: u32,
) -> Result<String> {
    // Some objects don't have a model
    if address == 0 {
        return Ok("".into());
    }

    // Go to the address, then read the address of the string
    let model_offset = dol.address_to_offset(address)?;
    reader.seek(SeekFrom::Start(model_offset as u64))?;
    let path_addr = reader.read_u32::<BE>()?;
    if path_addr == 0 {
        return Ok("".into());
    }

    let path_offset = dol.address_to_offset(path_addr)?;
    reader.seek(SeekFrom::Start(path_offset as u64))?;
    Ok(CString::read_from(reader)?.into_string()?)
}

/// Appends discriminators to objects which do not have unique labels.
fn deduplicate_labels(objects: &mut [ObjectDefinition]) {
    // Build a map of how many times each label appears
    let mut counts: HashMap<String, usize> = HashMap::new();
    for object in objects.iter() {
        counts.entry(object.label.0.clone()).and_modify(|c| *c += 1).or_insert(1);
    }

    for (i, object) in objects.iter_mut().enumerate() {
        let count = counts.get(&object.label.0).copied().unwrap_or(0);
        if object.label.0.is_empty() || count > 1 {
            // If the object has a subclass, use that as the discriminator instead of the ID
            let id = if object.subclass > 0 { object.subclass as usize } else { i };

            // Also generate names for unnamed objects using their class
            if object.label.0.is_empty() {
                write!(object.label.0, "{}{:?}", UNKNOWN_PREFIX, object.class).unwrap();
            }

            object.label.append_discriminator(id);
            assert!(!counts.contains_key(&object.label.0));
        }
    }
}

/// Renames object labels using the rules in `OBJECT_LABEL_FIXUPS`.
fn fixup_labels(objects: &mut [ObjectDefinition]) {
    for object in objects.iter_mut() {
        let label = &mut object.label;
        for (regex, replacement) in OBJECT_LABEL_FIXUPS.iter() {
            if let Cow::Owned(replaced) = regex.replace(&label.0, *replacement) {
                trace!("fixup_labels: {} -> {}", label.0, replaced);
                label.0 = replaced;
                break;
            }
        }
    }
}

/// Representation of an item which is written to the generated source.
#[derive(Debug, Clone)]
struct ItemDefinition {
    id: u16,
    label: Label,
    object: Option<Label>,
    flags: Vec<String>,
}

/// Builds the item list from object definition and globals data.
fn build_items(objects: &[ObjectDefinition], globals: &[Item]) -> Vec<ItemDefinition> {
    // Map items from the object table so we know their corresponding objects
    let mut items: Vec<ItemDefinition> = objects
        .iter()
        .filter_map(|object| {
            if object.class == ObjectClass::Item {
                // Generate item labels from several different sources:
                // 1. The label override defined for the item, if any
                // 2. The item's display name, if it has one
                // 3. The object's label with "Item" removed
                let metadata = &globals[object.subclass as usize];
                let display_name = metadata.name.decode().unwrap();
                let label = if let Some(&label) = ITEM_LABEL_OVERRIDES.get(&object.label.0) {
                    Label(label.into())
                } else if !display_name.is_empty() {
                    Label::from_string_lossy(&metadata.name.decode().unwrap())
                } else {
                    Label(object.label.0.replace(STRIP_ITEM_LABEL, ""))
                };
                let mut flags = vec![];
                if display_name.is_empty() {
                    flags.push(ITEM_FLAG_UNUSED.into());
                }
                Some(ItemDefinition {
                    id: object.subclass,
                    label,
                    object: Some(object.label.clone()),
                    flags,
                })
            } else {
                None
            }
        })
        .collect();

    // Insert missing items
    items.sort_unstable_by_key(|i| i.id);
    for i in 0..NUM_ITEMS {
        let id = i as u16;
        if items[i].id != id {
            let label = Label(format!("{}{}", UNKNOWN_PREFIX, id));
            let flags = vec![ITEM_FLAG_UNUSED.into()];
            items.insert(i, ItemDefinition { id, label, object: None, flags });
        }
    }
    assert_eq!(items.len(), NUM_ITEMS);
    items
}

/// Representation of an ATC which is written to the generated source.
struct AtcDefinition {
    id: u16,
    label: Label,
}

/// Builds the ATC list from globals data.
fn build_atcs(globals: &[Atc]) -> Vec<AtcDefinition> {
    globals
        .iter()
        .enumerate()
        .skip(1) // ATC IDs start from 1
        .map(|(id, atc)| {
            let display_name = atc.name.decode().unwrap();
            let label = if !display_name.is_empty() {
                Label::from_string_lossy(&display_name)
            } else {
                Label(format!("{}{}", UNKNOWN_PREFIX, id))
            };
            AtcDefinition { id: id as u16, label }
        })
        .collect()
}

/// Representation of a suit which is written to the generated source.
#[derive(Debug, Clone, Default)]
struct SuitDefinition {
    id: u16,
    label: Label,
    item: Label,
}

/// Reads suit info from main.dol and builds the suit list.
fn read_suits(
    dol: &DolHeader,
    reader: &mut (impl Read + Seek),
    globals: &[Suit],
    items: &[ItemDefinition],
) -> Result<Vec<SuitDefinition>> {
    // So this is kinda annoying, basically the suit-to-item mapping isn't in order by suit ID but
    // rather by the order they appear in the UI. We have to scan the UI initialization code to look
    // for `li` instructions (which are really just `addi`) that load the suit IDs.
    let order_offset = dol.address_to_offset(SUIT_ORDER_ADDR)? as u64;
    reader.seek(SeekFrom::Start(order_offset))?;
    let mut order = [0; NUM_SUITS];
    for index in order.iter_mut() {
        let li = loop {
            let op = reader.read_u32::<BE>()?;
            if op >> 26 == ADDI_OPCODE {
                break op;
            }
        };
        *index = (li & 0xffff) as u16;
    }

    // Now we can actually read the item IDs. Again, these are ordered by UI position.
    let items_offset = dol.address_to_offset(SUIT_ITEMS_ADDR)? as u64;
    reader.seek(SeekFrom::Start(items_offset))?;
    let mut item_ids = [0; NUM_SUITS];
    reader.read_u16_into::<BE>(&mut item_ids)?;

    let mut suits = vec![SuitDefinition::default(); NUM_SUITS];
    for (&id, &item_id) in order.iter().zip(&item_ids) {
        // Get the display name and label from globals
        let display_name = globals[id as usize].name.decode()?;
        let mut label = if !display_name.is_empty() {
            Label::from_string_lossy(&display_name)
        } else {
            Label(format!("{}{}", UNKNOWN_PREFIX, id))
        };
        label = Label(label.0.replace(STRIP_SUIT_LABEL, ""));

        // Look up the item ID in the suit items array
        let item = items[item_id as usize].label.clone();

        // Suit IDs start from 1
        suits[id as usize - 1] = SuitDefinition { id, label, item };
    }
    Ok(suits)
}

// The raw representation of a stage in the executable.
#[derive(Debug, Copy, Clone)]
struct RawStageDefinition {
    name_addr: u32,
    index: i32,
    _flags: u32,
}

impl<R: Read + ?Sized> ReadFrom<R> for RawStageDefinition {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self {
            name_addr: reader.read_u32::<BE>()?,
            index: reader.read_i32::<BE>()?,
            _flags: reader.read_u32::<BE>()?,
        })
    }
}

impl<R: Read> ReadOptionFrom<R> for RawStageDefinition {
    type Error = Error;
    fn read_option_from(reader: &mut R) -> Result<Option<Self>> {
        // The last stage in each list has a null address
        let stage = RawStageDefinition::read_from(reader)?;
        Ok(if stage.name_addr != 0 { Some(stage) } else { None })
    }
}

/// Representation of a stage which is written to the generated source.
struct StageDefinition {
    id: i32,
    label: Label,
    name: String,
}

/// Reads the stage list from the executable.
fn read_stages(
    dol: &DolHeader,
    reader: &mut (impl Read + Seek),
    globals: &[Stage],
) -> Result<Vec<StageDefinition>> {
    // The stage list has a null-terminated list of main stages followed by a null-terminated list
    // of developer stages.
    let stages_offset = dol.address_to_offset(STAGE_LIST_ADDR)? as u64;
    reader.seek(SeekFrom::Start(stages_offset))?;
    let mut stages = NonNoneList::<RawStageDefinition>::read_from(reader)?.into_vec();
    let dev_stages = NonNoneList::<RawStageDefinition>::read_from(reader)?.into_vec();
    // Correct the dev stage indexes because they're -1 in the executable
    stages.extend(
        dev_stages
            .into_iter()
            .enumerate()
            .map(|(i, stage)| RawStageDefinition { index: FIRST_DEV_STAGE + (i as i32), ..stage }),
    );

    let mut definitions: Vec<StageDefinition> = vec![];
    for stage in stages {
        let name_offset = dol.address_to_offset(stage.name_addr)? as u64;
        reader.seek(SeekFrom::Start(name_offset))?;
        let name = CString::read_from(reader)?.into_string()?;

        let label = if stage.index < FIRST_DEV_STAGE {
            // Try to build the stage name based on the name and description in globals
            let metadata = &globals[stage.index as usize];
            let display_name = metadata.name.decode()?;
            let display_desc = metadata.description.decode()?;
            if !display_name.is_empty() || !display_desc.is_empty() {
                Label::from_string_lossy(&format!("{} {}", display_name, display_desc))
            } else {
                Label::from_string_lossy(&name)
            }
        } else {
            Label::from_string_lossy(&name)
        };

        definitions.push(StageDefinition { id: stage.index, label, name });
    }
    Ok(definitions)
}

// The raw representation of a music file in the executable.
#[derive(Default, Copy, Clone)]
struct RawMusicDefinition {
    path_addr: u32,
    volume: u8,
}

impl<R: Read + ?Sized> ReadFrom<R> for RawMusicDefinition {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let result = Self { path_addr: reader.read_u32::<BE>()?, volume: reader.read_u8()? };
        let _padding = reader.read_u24::<BE>()?;
        Ok(result)
    }
}

/// Representation of a music file which is written to the generated source.
struct MusicDefinition {
    id: u8,
    label: Label,
    name: String,
    volume: u8,
}

/// Reads the music list from the executable.
fn read_music(dol: &DolHeader, reader: &mut (impl Read + Seek)) -> Result<Vec<MusicDefinition>> {
    let music_offset = dol.address_to_offset(MUSIC_LIST_ADDR)? as u64;
    reader.seek(SeekFrom::Start(music_offset))?;
    let mut raw_music = [RawMusicDefinition::default(); NUM_MUSIC];
    RawMusicDefinition::read_all_from(reader, &mut raw_music)?;

    let mut definitions: Vec<MusicDefinition> = vec![];
    // Music IDs start at 1
    for (id, music) in raw_music.iter().enumerate().skip(1) {
        let path_offset = dol.address_to_offset(music.path_addr)? as u64;
        reader.seek(SeekFrom::Start(path_offset))?;
        let mut path = CString::read_from(reader)?.into_string()?;

        // Fix up the path if necessary
        for (find, replace) in MUSIC_PATH_FIXUPS.iter() {
            if let Cow::Owned(new) = find.replace(&path, *replace) {
                path = new;
                break;
            }
        }

        // Make the label based on the filename
        let filename = path.rsplit('/').next().unwrap();
        let name = filename.strip_suffix(MUSIC_EXT).unwrap().to_owned();
        let label = match MUSIC_LABEL_OVERRIDES.get(&name) {
            Some(&label) => Label(label.to_owned()),
            None => Label::from_string_lossy(&name),
        };
        definitions.push(MusicDefinition { id: id as u8, label, name, volume: music.volume });
    }
    Ok(definitions)
}

/// Representation of a sound bank which is written to the generated source.
struct SoundBankDefinition {
    id: i16,
    label: Label,
    sound_base: u32,
    event_base: u32,
    name: String,
}

/// Reads sound bank information from the ISO.
fn read_sound_banks(
    disc: &mut DiscStream<impl Read + Seek>,
    events: &EventBank,
) -> Result<Vec<SoundBankDefinition>> {
    let mut bank_bases: Vec<(&'static str, u32)> = vec![];
    for &(name, _) in SOUND_BANKS {
        let path = format!("{}/{}{}", SOUND_BANK_DIR, name, SOUND_BANK_EXT);
        let mut reader = disc.open_file_at(&path)?;
        reader.seek(SeekFrom::Start(0xc))?;
        let base_index = reader.read_u32::<BE>()?;
        bank_bases.push((name, base_index));
    }
    bank_bases.sort_unstable_by_key(|(_, b)| *b);

    let mut banks = vec![];
    for (id, ((name, sound_base), &event_base)) in
        bank_bases.into_iter().zip(&events.group_bases).enumerate()
    {
        let label = Label::from_string_lossy(name.strip_prefix(SOUND_BANK_PREFIX).unwrap());
        banks.push(SoundBankDefinition {
            id: id as i16,
            label,
            sound_base,
            event_base,
            name: name.to_owned(),
        })
    }
    Ok(banks)
}

/// Representation of a sound event which is written to the generated source.
struct SoundEventDefinition {
    id: u32,
    label: Label,
    name: String,
}

/// Builds the sound event list by matching sounds in a BRSAR with an event bank.
fn build_sound_events(
    events: &EventBank,
    brsar: &Brsar,
    banks: &[SoundBankDefinition],
) -> Vec<SoundEventDefinition> {
    #[derive(Default, Copy, Clone)]
    struct BankState {
        next_id: u32,
        end_id: u32,
    }

    // Compute each bank's starting ID and ending ID. NPC has sounds that the GCN version doesn't,
    // so we have to make sure not to emit definitions for extra sounds.
    let mut states = vec![BankState::default(); banks.len()];
    let mut end_event = events.events.len() as u32;
    for (id, &base) in events.group_bases.iter().enumerate().rev() {
        states[id].next_id = (id as u32) << 16;
        states[id].end_id = states[id].next_id + end_event - base;
        end_event = base;
    }

    // Build a map of collection indexes to bank indexes
    let mut collection_to_bank: HashMap<u32, usize> = HashMap::new();
    for (i, collection) in brsar.collections.iter().enumerate() {
        if !collection.groups.is_empty() {
            let group_index = collection.groups[0].index;
            let group = &brsar.groups[group_index as usize];
            let group_name = brsar.symbol(group.name_index);
            let bank_name = SOUND_BANKS.iter().find(|&&(_, g)| g == group_name);
            if let Some(&(name, _)) = bank_name {
                let bank_def = banks.iter().find(|b| b.name == name).unwrap();
                let bank_index = bank_def.id as usize;
                collection_to_bank.insert(i as u32, bank_index);
            } else {
                warn!("No bank found for group \"{}\"", group_name);
            }
        }
    }

    // Look up each sound's corresponding bank and give it the next ID for the bank. It turns out
    // that the order sounds are stored in the BRSAR matches the order of the events in the .sem.
    let mut defs = Vec::with_capacity(events.events.len());
    for sound in &brsar.sounds {
        if let Some(&bank_id) = collection_to_bank.get(&sound.collection_index) {
            let bank = &mut states[bank_id];
            if bank.next_id < bank.end_id {
                // Convert sound names to lowercase. While the names in the BRSAR are in uppercase,
                // every file in the GameCube build uses lowercase, so this makes the sound file
                // names more consistent with everything else.
                let name = brsar.symbol(sound.name_index).to_lowercase();
                let label = Label::from_string_lossy(&name);
                defs.push(SoundEventDefinition { id: bank.next_id, label, name });
                bank.next_id += 1;
            }
        }
    }
    debug!("Found names for {}/{} sound events", defs.len(), events.events.len());

    // Fill in any IDs we somehow missed
    for bank in &mut states {
        while bank.next_id < bank.end_id {
            let name = format!("unk_{:>06x}", bank.next_id);
            let label = Label::from_string_lossy(&name);
            defs.push(SoundEventDefinition { id: bank.next_id, label, name });
            bank.next_id += 1;
        }
    }
    defs.sort_unstable_by_key(|d| d.id);
    defs
}

/// Representation of a sound which is written to the generated source.
struct SoundDefinition {
    id: u32,
    label: Label,
    name: String,
}

/// Builds the sound list by matching event names.
fn build_sounds(events: &EventBank, event_defs: &[SoundEventDefinition]) -> Vec<SoundDefinition> {
    let mut defs: Vec<SoundDefinition> = Vec::with_capacity(event_defs.len());
    for (event, def) in events.events.iter().zip(event_defs) {
        if DUPLICATE_SOUND_DISCARDS.is_match(&def.name) {
            continue;
        }
        if let Some(action) = event.actions.iter().find(|a| a.command == Command::Sound) {
            defs.push(SoundDefinition {
                id: action.data as u32,
                label: def.label.clone(),
                name: def.name.clone(),
            });
        }
    }
    debug!("Found {} sound names", defs.len());
    defs.sort_unstable_by_key(|d| d.id);
    defs
}

/// Writes the list of objects to the generated file.
fn write_objects(mut writer: impl Write, objects: &[ObjectDefinition]) -> Result<()> {
    write!(writer, "{}{}", GEN_HEADER, OBJECTS_HEADER)?;
    for object in objects {
        writeln!(
            writer,
            "    {} => {} {{ {:?}, {}, \"{}\" }},",
            object.id, object.label.0, object.class, object.subclass, object.model_path
        )?;
    }
    write!(writer, "{}", OBJECTS_FOOTER)?;
    writer.flush()?;
    Ok(())
}

/// Writes the list of items to the generated file.
fn write_items(mut writer: impl Write, items: &[ItemDefinition]) -> Result<()> {
    write!(writer, "{}{}", GEN_HEADER, ITEMS_HEADER)?;
    for item in items {
        let object = match &item.object {
            Some(label) => &label.0,
            _ => "None",
        };
        let flags: String = item.flags.iter().map(|f| format!(", {}", f)).collect();
        writeln!(writer, "    {} => {} {{ {}{} }},", item.id, item.label.0, object, flags)?;
    }
    write!(writer, "{}", ITEMS_FOOTER)?;
    writer.flush()?;
    Ok(())
}

/// Writes the list of ATCs to the generated file.
fn write_atcs(mut writer: impl Write, atcs: &[AtcDefinition]) -> Result<()> {
    write!(writer, "{}{}", GEN_HEADER, ATCS_HEADER)?;
    for atc in atcs {
        writeln!(writer, "    {} => {},", atc.id, atc.label.0)?;
    }
    write!(writer, "{}", ATCS_FOOTER)?;
    writer.flush()?;
    Ok(())
}

/// Writes the list of suits to the generated file.
fn write_suits(mut writer: impl Write, suits: &[SuitDefinition]) -> Result<()> {
    write!(writer, "{}{}", GEN_HEADER, SUITS_HEADER)?;
    for suit in suits {
        writeln!(writer, "    {} => {} {{ {} }},", suit.id, suit.label.0, suit.item.0)?;
    }
    write!(writer, "{}", SUITS_FOOTER)?;
    writer.flush()?;
    Ok(())
}

/// Writes the list of stages to the generated file.
fn write_stages(mut writer: impl Write, stages: &[StageDefinition]) -> Result<()> {
    write!(writer, "{}{}", GEN_HEADER, STAGES_HEADER)?;
    for stage in stages {
        writeln!(writer, "    {} => {} {{ \"{}\" }},", stage.id, stage.label.0, stage.name)?;
    }
    write!(writer, "{}", STAGES_FOOTER)?;
    writer.flush()?;
    Ok(())
}

/// Writes the music list to the generated file.
fn write_music(mut writer: impl Write, music: &[MusicDefinition]) -> Result<()> {
    write!(writer, "{}{}", GEN_HEADER, MUSIC_HEADER)?;
    for m in music {
        writeln!(writer, "    {} => {} {{ {}, \"{}\" }},", m.id, m.label.0, m.volume, m.name)?;
    }
    write!(writer, "{}", MUSIC_FOOTER)?;
    writer.flush()?;
    Ok(())
}

/// Writes the list of sound banks to the generated file.
fn write_sound_banks(mut writer: impl Write, banks: &[SoundBankDefinition]) -> Result<()> {
    write!(writer, "{}{}", GEN_HEADER, SOUND_BANKS_HEADER)?;
    for bank in banks {
        writeln!(
            writer,
            "    {} => {} {{ 0x{:>04x}, 0x{:>04x}, \"{}\" }},",
            bank.id, bank.label.0, bank.sound_base, bank.event_base, bank.name
        )?;
    }
    write!(writer, "{}", SOUND_BANKS_FOOTER)?;
    writer.flush()?;
    Ok(())
}

/// Writes the list of sound events to the generated file.
fn write_sound_events(mut writer: impl Write, events: &[SoundEventDefinition]) -> Result<()> {
    write!(writer, "{}{}", GEN_HEADER_BRSAR, SOUND_EVENTS_HEADER)?;
    for event in events {
        writeln!(writer, "    0x{:>06x} => {} {{ \"{}\" }},", event.id, event.label.0, event.name)?;
    }
    write!(writer, "{}", SOUND_EVENTS_FOOTER)?;
    writer.flush()?;
    Ok(())
}

/// Writes the list of sounds to the generated file.
fn write_sounds(mut writer: impl Write, sounds: &[SoundDefinition]) -> Result<()> {
    write!(writer, "{}{}", GEN_HEADER_BRSAR, SOUNDS_HEADER)?;
    for sound in sounds {
        writeln!(writer, "    {} => {} {{ \"{}\" }},", sound.id, sound.label.0, sound.name)?;
    }
    write!(writer, "{}", SOUNDS_FOOTER)?;
    writer.flush()?;
    Ok(())
}

fn run_app() -> Result<()> {
    init_logging();

    let options = match parse_options() {
        Ok(o) => o,
        Err(err) => {
            usage();
            error!("Fatal: {:#}", err);
            process::exit(1);
        }
    };

    let test_path: PathBuf = [UNPLUG_DATA_PATH, SRC_DIR_NAME, TEST_FILE_NAME].iter().collect();
    if !test_path.exists() {
        bail!(
            "Could not locate {}. Make sure unplug-datagen is run from the workspace root.",
            test_path.display(),
        );
    }

    info!("Opening ISO");
    let mut iso = DiscStream::open(File::open(&options.iso)?)?;

    info!("Reading sound events");
    let events = {
        let mut reader = BufReader::new(iso.open_file_at(SOUND_EVENTS_PATH)?);
        EventBank::read_from(&mut reader)?
    };

    info!("Reading sound banks");
    let banks = read_sound_banks(&mut iso, &events)?;

    let mut sound_events = vec![];
    let mut sounds = vec![];
    if let Some(brsar_path) = options.brsar {
        info!("Reading BRSAR");
        let mut brsar_reader = BufReader::new(File::open(&brsar_path)?);
        let brsar = Brsar::read_from(&mut brsar_reader)?;
        info!("Matching sound event names");
        sound_events = build_sound_events(&events, &brsar, &banks);
        sounds = build_sounds(&events, &sound_events);
    }

    let metadata = {
        info!("Opening {}", QP_PATH);
        let mut qp = ArchiveReader::open(iso.open_file_at(QP_PATH)?)?;
        info!("Reading globals.bin");
        let mut globals = GlobalsReader::open(qp.open_file_at(GLOBALS_PATH)?)?;
        globals.read_metadata()?
    };

    info!("Opening main.dol");
    let (dol, dol_reader) = iso.open_dol()?;
    let mut dol_reader = BufReader::new(dol_reader);

    info!("Reading object tables");
    let mut objects = read_objects(&dol, &mut dol_reader, MAIN_OBJECTS_ADDR, NUM_MAIN_OBJECTS)?;
    let internal_objects =
        read_objects(&dol, &mut dol_reader, INTERNAL_OBJECTS_ADDR, NUM_INTERNAL_OBJECTS)?;

    info!("Generating object data");
    for mut object in internal_objects {
        object.id += INTERNAL_OBJECTS_BASE_ID;
        object.label.0.insert_str(0, INTERNAL_PREFIX);
        objects.push(object);
    }
    deduplicate_labels(&mut objects);
    fixup_labels(&mut objects);

    info!("Generating item data");
    let items = build_items(&objects, &metadata.items);

    info!("Generating ATC data");
    let atcs = build_atcs(&metadata.atcs);

    info!("Reading suit data");
    let suits = read_suits(&dol, &mut dol_reader, &metadata.suits, &items)?;

    info!("Reading stage data");
    let stages = read_stages(&dol, &mut dol_reader, &metadata.stages)?;

    info!("Reading music data");
    let music = read_music(&dol, &mut dol_reader)?;

    let out_dir: PathBuf = [UNPLUG_DATA_PATH, SRC_DIR_NAME, OUTPUT_DIR_NAME].iter().collect();
    fs::create_dir_all(&out_dir)?;

    let objects_path = out_dir.join(OBJECTS_FILE_NAME);
    info!("Writing {}", objects_path.display());
    let objects_writer = BufWriter::new(File::create(objects_path)?);
    write_objects(objects_writer, &objects)?;

    let items_path = out_dir.join(ITEMS_FILE_NAME);
    info!("Writing {}", items_path.display());
    let items_writer = BufWriter::new(File::create(items_path)?);
    write_items(items_writer, &items)?;

    let atcs_path = out_dir.join(ATCS_FILE_NAME);
    info!("Writing {}", atcs_path.display());
    let atcs_writer = BufWriter::new(File::create(atcs_path)?);
    write_atcs(atcs_writer, &atcs)?;

    let suits_path = out_dir.join(SUITS_FILE_NAME);
    info!("Writing {}", suits_path.display());
    let suits_writer = BufWriter::new(File::create(suits_path)?);
    write_suits(suits_writer, &suits)?;

    let stages_path = out_dir.join(STAGES_FILE_NAME);
    info!("Writing {}", stages_path.display());
    let stages_writer = BufWriter::new(File::create(stages_path)?);
    write_stages(stages_writer, &stages)?;

    let music_path = out_dir.join(MUSIC_FILE_NAME);
    info!("Writing {}", music_path.display());
    let music_writer = BufWriter::new(File::create(music_path)?);
    write_music(music_writer, &music)?;

    let sound_banks_path = out_dir.join(SOUND_BANKS_FILE_NAME);
    info!("Writing {}", sound_banks_path.display());
    let sound_banks_writer = BufWriter::new(File::create(sound_banks_path)?);
    write_sound_banks(sound_banks_writer, &banks)?;

    if !sound_events.is_empty() {
        let sound_events_path = out_dir.join(SOUND_EVENTS_FILE_NAME);
        info!("Writing {}", sound_events_path.display());
        let sound_events_writer = BufWriter::new(File::create(sound_events_path)?);
        write_sound_events(sound_events_writer, &sound_events)?;
    }

    if !sounds.is_empty() {
        let sounds_path = out_dir.join(SOUNDS_FILE_NAME);
        info!("Writing {}", sounds_path.display());
        let sounds_writer = BufWriter::new(File::create(sounds_path)?);
        write_sounds(sounds_writer, &sounds)?;
    }

    Ok(())
}

fn main() {
    process::exit(match run_app() {
        Ok(_) => 0,
        Err(err) => {
            error!("Fatal: {:#}", err);
            1
        }
    });
}
