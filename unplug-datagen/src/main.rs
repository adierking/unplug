use anyhow::{anyhow, bail, Error, Result};
use bitflags::bitflags;
use byteorder::{ReadBytesExt, BE, LE};
use lazy_static::lazy_static;
use log::{debug, error, info, trace, warn};
use num_enum::TryFromPrimitive;
use regex::{Regex, RegexSet};
use simplelog::{Color, ColorChoice, ConfigBuilder, Level, LevelFilter, TermLogger, TerminalMode};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::convert::TryFrom;
use std::env;
use std::ffi::CString;
use std::fmt::Write as FmtWrite;
use std::fmt::{self, Debug};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process;
use time::macros::format_description;
use unplug::audio::metadata::SfxPlaylist;
use unplug::audio::transport::Brsar;
use unplug::common::{NonNoneList, ReadFrom, ReadOptionFrom, ReadSeek};
use unplug::dvd::{ArchiveReader, DiscStream, DolHeader, OpenFile};
use unplug::globals::metadata::{Atc, Item, Stage, Suit};
use unplug::globals::{GlobalsReader, Metadata};
use unplug::stage::Actor;

const MAIN_OBJECTS_ADDR: u32 = 0x8021c70c;
const NUM_MAIN_OBJECTS: usize = 1162;

const INTERNAL_OBJECTS_ADDR: u32 = 0x80223690;
const NUM_INTERNAL_OBJECTS: usize = 36;
const INTERNAL_OBJECTS_BASE_ID: i32 = 10000;

const SPAWNABLES_ADDR: u32 = 0x80223a80;
const NUM_SPAWNABLES: usize = 47;

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
const QP_STAGES_PATH: &str = "bin/e";
const QP_GLOBALS_NAME: &str = "globals.bin";

const INTERNAL_PREFIX: &str = "Internal";
const UNKNOWN_PREFIX: &str = "Unk";

const NUM_ITEMS: usize = 159;
const STRIP_ITEM_LABEL: &str = "Item";

const NPC_DOL_PATH: &str = "sys/main.dol";
const NPC_BRSAR_PATH: &str = "files/snd/cb_robo.brsar";
const NPC_OBJECTS_ADDR: u32 = 0x802f0138;

/// Names of sound banks within the ISO and their corresponding BRSAR groups. This does not include
/// `sfx_hori` because it has a bad base index and cannot be loaded.
const SFX_GROUPS: &[(&str, &str)] = &[
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
const SOUND_PLAYLIST_PATH: &str = "qp/sfx_sample.sem";

const UNPLUG_DATA_PATH: &str = "unplug-data";
const SRC_DIR_NAME: &str = "src";
const TEST_FILE_NAME: &str = "lib.rs";
const OUTPUT_DIR_NAME: &str = "gen";

const GEN_HEADER: &str = "// Generated with unplug-datagen. DO NOT EDIT.\n\
                          // To regenerate: cargo run -p unplug-datagen -- <iso path>\n\n";
const GEN_HEADER_NPC: &str = "// Generated with unplug-datagen. DO NOT EDIT.\n\
                              // To regenerate: cargo run -p unplug-datagen -- <iso path> \
                              --npc <NPC data path>\n\n";

const OBJECTS_FILE_NAME: &str = "objects.inc.rs";
const OBJECTS_HEADER: &str = "declare_objects! {\n";
const OBJECTS_FOOTER: &str = "}\n";

const SPAWNABLES_FILE_NAME: &str = "spawnables.inc.rs";
const SPAWNABLES_HEADER: &str = "declare_spawnables! {\n";
const SPAWNABLES_FOOTER: &str = "}\n";

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

const SFX_GROUPS_FILE_NAME: &str = "sfx_groups.inc.rs";
const SFX_GROUPS_HEADER: &str = "declare_sfx_groups! {\n";
const SFX_GROUPS_FOOTER: &str = "}\n";

const SFX_FILE_NAME: &str = "sfx.inc.rs";
const SFX_HEADER: &str = "declare_sfx! {\n";
const SFX_FOOTER: &str = "}\n";

const SFX_SAMPLES_FILE_NAME: &str = "sfx_samples.inc.rs";
const SFX_SAMPLES_HEADER: &str = "declare_sfx_samples! {\n";
const SFX_SAMPLES_FOOTER: &str = "}\n";

const ANIMATIONS_FILE_NAME: &str = "animations.inc.rs";
const ANIMATIONS_HEADER: &str = "declare_animations! {\n";
const ANIMATIONS_FOOTER: &str = "}\n";

const ACTORS_FILE_NAME: &str = "actors.inc.rs";
const ACTORS_HEADER: &str = "declare_actors! {\n";
const ACTORS_FOOTER: &str = "}\n";

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

    /// Strings to remove from actor names when making them into labels.
    static ref ACTOR_NAME_FIXUPS: Vec<Regex> = vec![
        Regex::new(r"^Space Hunter ").unwrap(),
        Regex::new(r"^Captain ").unwrap(),
        Regex::new(r"^The Great ").unwrap(),
        Regex::new(r"^Princess ").unwrap(),
    ];

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
    static ref DUPLICATE_SOUND_DISCARDS: RegexSet = RegexSet::new([
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
        .set_level_color(Level::Info, Some(Color::Green))
        .set_time_format_custom(format_description!(
            "[hour]:[minute]:[second].[subsecond digits:3]"
        ))
        .build();
    TermLogger::init(LevelFilter::Debug, config, TerminalMode::Stderr, ColorChoice::Auto).unwrap();
}

/// Command-line options.
struct Options {
    /// Path to the ISO to load.
    iso: PathBuf,
    /// Path to an extracted data partition from New Play Control! Chibi-Robo.
    npc: Option<PathBuf>,
}

fn parse_options() -> Result<Options> {
    let mut iso: Option<PathBuf> = None;
    let mut npc: Option<PathBuf> = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_ref() {
            "-h" | "--help" => {
                usage();
                process::exit(0);
            }
            "--npc" => {
                let path = args.next().ok_or_else(|| anyhow!("--npc: path expected"))?;
                npc = Some(PathBuf::from(path));
            }
            arg if iso.is_none() => iso = Some(PathBuf::from(arg)),
            arg => bail!("unrecognized argument: {}", arg),
        }
    }
    Ok(Options { iso: iso.ok_or_else(|| anyhow!("ISO path expected"))?, npc })
}

fn usage() {
    eprintln!("Usage: cargo run -p unplug-datagen -- <iso> [--npc <path>]");
    eprintln!("\nTo generate animation and sound names, --npc must be provided");
    eprintln!("along with a path to the data partition extracted from the Wii");
    eprintln!("release of the game (R24J01).");
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
    fn pascal_case(s: &str) -> Self {
        let mut name = String::new();
        let mut capitalize = true;
        for ch in s.chars() {
            if ch.is_alphabetic() {
                let capitalized: String = if capitalize {
                    ch.to_uppercase().collect()
                } else {
                    ch.to_lowercase().collect()
                };
                name.push_str(&capitalized);
                capitalize = false;
            } else if ch.is_ascii_digit() {
                name.push(ch);
                capitalize = true;
            } else if ch != '\'' {
                capitalize = true;
            }
        }
        Self(name)
    }

    /// Creates a `Label` from a string, changing the capitalization to snake_case and discarding
    /// unusable characters.
    fn snake_case(s: &str) -> Self {
        let mut name = String::new();
        let mut chars = s.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch.is_uppercase() {
                name.extend(ch.to_lowercase());
            } else if ch.is_alphanumeric() {
                name.push(ch);
            } else {
                continue;
            }
            if let Some(next) = chars.peek().copied() {
                let underscore = next.is_uppercase()
                    || (ch.is_alphabetic() && !next.is_alphabetic())
                    || (ch.is_ascii_digit() && !next.is_ascii_digit());
                if underscore {
                    name.push('_');
                }
            }
        }
        Self(name)
    }

    /// Appends a discriminator ID to this label.
    fn append_discriminator(&mut self, id: usize) {
        if self.0.ends_with(|c: char| c.is_ascii_digit()) {
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
    id: i32,
    label: Label,
    name: Label,
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
        let label = Label::pascal_case(&model_path);
        objects.push(ObjectDefinition {
            id: i as i32,
            label,
            name: Label::default(),
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
        object.name = Label::snake_case(&label.0);
    }
}

/// Converts an object ID to an array index.
fn object_index(object: i32) -> usize {
    if object >= INTERNAL_OBJECTS_BASE_ID {
        (object - INTERNAL_OBJECTS_BASE_ID) as usize + NUM_MAIN_OBJECTS
    } else {
        object as usize
    }
}

/// Animation name info which is written to the generated source.
#[derive(Debug, Clone)]
struct AnimationDefinition {
    object: i32,
    index: i32,
    label: Label,
    name: String,
}

/// Reads animations for `count` objects from `address` in the NPC executable.
fn read_npc_animations(
    dol: &DolHeader,
    reader: &mut (impl Read + Seek),
    address: u32,
    count: usize,
) -> Result<Vec<AnimationDefinition>> {
    let offset = dol.address_to_offset(address)? as u64;
    let mut anims = Vec::new();
    for obj_index in 0..count {
        reader.seek(SeekFrom::Start(offset + (obj_index as u64) * 0x18))?;
        let _name = reader.read_u32::<BE>()?;
        let _unk_4 = reader.read_u32::<BE>()?;
        let _unk_8 = reader.read_u32::<BE>()?;
        let _unk_c = reader.read_u32::<BE>()?;
        let anim_address = reader.read_u32::<BE>()?;
        let _unk_14 = reader.read_u32::<BE>()?;
        if anim_address != 0 {
            let anim_offset = dol.address_to_offset(anim_address)? as u64;
            let mut anim_index: usize = 0;
            loop {
                reader.seek(SeekFrom::Start(anim_offset + (anim_index as u64) * 4))?;
                let name_address = reader.read_u32::<BE>()?;
                if name_address == 0 {
                    break;
                }
                let name_offset = dol.address_to_offset(name_address)? as u64;
                reader.seek(SeekFrom::Start(name_offset))?;
                let name = CString::read_from(reader)?.into_string()?;
                anims.push(AnimationDefinition {
                    object: obj_index as i32,
                    index: anim_index as i32,
                    label: Label::pascal_case(&name),
                    name,
                });
                anim_index += 1;
            }
        }
    }
    Ok(anims)
}

fn deduplicate_animations(anims: &mut [AnimationDefinition]) {
    // Build a map of how many times each label appears
    let mut counts: HashMap<String, usize> = HashMap::new();
    for anim in anims.iter() {
        counts.entry(anim.label.0.clone()).and_modify(|c| *c += 1).or_insert(1);
    }

    let mut current_prefix = "unk".to_owned();
    for anim in anims {
        let count = counts.get(&anim.label.0).copied().unwrap_or(0);
        if count > 1 {
            let prefixed = format!("{}_{}", current_prefix, anim.name);
            anim.label = Label::pascal_case(&prefixed);
            anim.label.append_discriminator(anim.index as usize);
            anim.name = format!("{}_{}", prefixed, anim.index);
            assert!(!counts.contains_key(&anim.label.0));
        } else if let Some((prefix, _)) = anim.name.split_once('_') {
            if current_prefix != prefix {
                current_prefix = prefix.to_owned();
            }
        }
    }
}

// This is mostly copied from the main library; we can't directly use that struct because it uses
// `Object` instead of a raw ID.
#[derive(Debug, Copy, Clone)]
struct ObjectPlacement {
    id: i32,
    x: i32,
    y: i32,
    z: i32,
    _rotate_x: i32,
    _rotate_y: i32,
    _rotate_z: i32,
    scale_x: i32,
    scale_y: i32,
    scale_z: i32,
    _data: i32,
    _spawn_flag: i32,
    variant: i32,
    flags: ObjectFlags,
}

bitflags! {
    /// Bitflags which define how an object behaves. Copied from the main library.
    struct ObjectFlags: u32 {
        const SPAWN = 1 << 0;
        const OPAQUE = 1 << 1;
        const BLASTTHRU = 1 << 2;
        const RADAR = 1 << 3;
        const INTANGIBLE = 1 << 4;
        const INVISIBLE = 1 << 5;
        const TOON = 1 << 6;
        const FLASH = 1 << 7;
        const UNLIT = 1 << 8;
        const BOTCAM = 1 << 9;
        const EXPLODE = 1 << 10;
        const PUSHTHRU = 1 << 11;
        const LOWPRI = 1 << 12;
        const REFLECT = 1 << 13;
        const PUSHBLOCK = 1 << 14;
        const CULL = 1 << 15;
        const LIFT = 1 << 16;
        const CLIMB = 1 << 17;
        const CLAMBER = 1 << 18;
        const LADDER = 1 << 19;
        const ROPE = 1 << 20;
        const STAIRS = 1 << 21;
        const FALL = 1 << 22;
        const GRAB = 1 << 23;
        const INTERACT = 1 << 24;
        const TOUCH = 1 << 25;
        const ATC = 1 << 26;
        const PROJECTILE = 1 << 27;
        const UNK_28 = 1 << 28;
        const MIRROR = 1 << 29;
        const UNK_30 = 1 << 30;
        const DISABLED = 1 << 31;
    }
}

impl<R: Read + ?Sized> ReadOptionFrom<R> for ObjectPlacement {
    type Error = Error;
    fn read_option_from(reader: &mut R) -> Result<Option<Self>> {
        let id = reader.read_i32::<BE>()?;
        if id < 0 {
            return Ok(None);
        }
        Ok(Some(Self {
            id,
            x: reader.read_i32::<BE>()?,
            y: reader.read_i32::<BE>()?,
            z: reader.read_i32::<BE>()?,
            _rotate_x: reader.read_i32::<BE>()?,
            _rotate_y: reader.read_i32::<BE>()?,
            _rotate_z: reader.read_i32::<BE>()?,
            scale_x: reader.read_i32::<BE>()?,
            scale_y: reader.read_i32::<BE>()?,
            scale_z: reader.read_i32::<BE>()?,
            _data: reader.read_i32::<BE>()?,
            _spawn_flag: reader.read_i32::<BE>()?,
            variant: reader.read_i32::<BE>()?,
            flags: ObjectFlags::from_bits_truncate(reader.read_u32::<BE>()?),
        }))
    }
}

struct Spawnable {
    label: Label,
    name: Label,
    placement: ObjectPlacement,
}

/// Reads the spawnables table from the executable.
fn read_spawnables(
    dol: &DolHeader,
    reader: &mut (impl Read + Seek),
    objects: &[ObjectDefinition],
) -> Result<Vec<Spawnable>> {
    let offset = dol.address_to_offset(SPAWNABLES_ADDR)? as u64;
    reader.seek(SeekFrom::Start(offset))?;
    let mut spawnables = Vec::with_capacity(NUM_SPAWNABLES);
    let mut label_counts: HashMap<String, usize> = HashMap::new();
    for i in 0..NUM_SPAWNABLES {
        let placement = ObjectPlacement::read_option_from(reader)?.unwrap();
        let object = object_index(placement.id);
        let mut label = objects[object].label.0.clone();
        if let Some(stripped) = label.strip_prefix(INTERNAL_PREFIX) {
            label = stripped.into();
        } else if label.strip_prefix(UNKNOWN_PREFIX).is_some() {
            label = format!("{}{}", UNKNOWN_PREFIX, i);
        }
        *label_counts.entry(label.clone()).or_default() += 1;
        let name = Label::snake_case(&label);
        spawnables.push(Spawnable { label: Label(label), name, placement });
    }
    for (i, spawnable) in spawnables.iter_mut().enumerate() {
        if label_counts[&spawnable.label.0] > 1 {
            spawnable.label.append_discriminator(i);
            spawnable.name = Label::snake_case(&spawnable.label.0);
        }
    }
    Ok(spawnables)
}

/// Representation of an item which is written to the generated source.
#[derive(Debug, Clone)]
struct ItemDefinition {
    id: u16,
    label: Label,
    name: Label,
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
                    Label::pascal_case(&metadata.name.decode().unwrap())
                } else {
                    Label(object.label.0.replace(STRIP_ITEM_LABEL, ""))
                };
                let mut flags = vec![];
                if display_name.is_empty() {
                    flags.push(ITEM_FLAG_UNUSED.into());
                }
                let name = Label::snake_case(&label.0);
                Some(ItemDefinition {
                    id: object.subclass,
                    label,
                    name,
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
            let name = Label::snake_case(&label.0);
            let flags = vec![ITEM_FLAG_UNUSED.into()];
            items.insert(i, ItemDefinition { id, label, name, object: None, flags });
        }
    }
    assert_eq!(items.len(), NUM_ITEMS);
    items
}

/// Representation of an ATC which is written to the generated source.
struct AtcDefinition {
    id: u16,
    label: Label,
    name: Label,
}

/// Builds the ATC list from globals data.
fn build_atcs(globals: &[Atc]) -> Vec<AtcDefinition> {
    let mut atcs = globals
        .iter()
        .enumerate()
        .map(|(id, atc)| {
            let display_name = atc.name.decode().unwrap();
            let label = if !display_name.is_empty() {
                Label::pascal_case(&display_name)
            } else {
                Label(format!("{}{}", UNKNOWN_PREFIX, id))
            };
            let name = Label::snake_case(&label.0);
            AtcDefinition { id: id as u16, label, name }
        })
        .collect::<Vec<_>>();
    atcs[0].label = Label("None".to_owned());
    atcs[0].name = Label("none".to_owned());
    atcs
}

/// Representation of a suit which is written to the generated source.
#[derive(Debug, Clone, Default)]
struct SuitDefinition {
    id: u16,
    label: Label,
    name: Label,
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

    let mut suits = vec![SuitDefinition::default(); NUM_SUITS + 1];
    suits[0].label = Label("None".to_owned());
    suits[0].name = Label("none".to_owned());
    suits[0].item = Label("None".to_owned());
    for (&id, &item_id) in order.iter().zip(&item_ids) {
        // Get the display name and label from globals
        let display_name = globals[id as usize].name.decode()?;
        let mut label = if !display_name.is_empty() {
            Label::pascal_case(&display_name)
        } else {
            Label(format!("{}{}", UNKNOWN_PREFIX, id))
        };
        label = Label(label.0.replace(STRIP_SUIT_LABEL, ""));
        let name = Label::snake_case(&label.0);

        // Look up the item ID in the suit items array
        let item = items[item_id as usize].label.clone();
        suits[id as usize] = SuitDefinition { id, label, name, item };
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
    title: String,
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

        let mut title = String::new();
        let label = if stage.index < FIRST_DEV_STAGE {
            // Try to build the stage name based on the name and description in globals
            let metadata = &globals[stage.index as usize];
            let display_name = metadata.name.decode()?;
            let display_desc = metadata.description.decode()?;
            title = format!("{} {}", display_name.trim(), display_desc.trim()).trim().to_owned();
            if !title.is_empty() {
                Label::pascal_case(&title)
            } else {
                Label::pascal_case(&name)
            }
        } else {
            Label::pascal_case(&name)
        };
        if title.is_empty() {
            title = name.clone();
        }
        definitions.push(StageDefinition { id: stage.index, label, name, title });
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
    for (id, music) in raw_music.iter().enumerate() {
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
            None => Label::pascal_case(&name),
        };
        definitions.push(MusicDefinition { id: id as u8, label, name, volume: music.volume });
    }
    definitions[0].label = Label("None".to_owned());
    definitions[0].name = "none".to_owned();
    Ok(definitions)
}

/// Representation of an SFX group which is written to the generated source.
struct SfxGroupDefinition {
    id: i16,
    label: Label,
    first_sample: u32,
    first_material: u32,
    name: String,
}

/// Reads SFX group information from the ISO.
fn read_sfx_groups(
    disc: &mut DiscStream<impl ReadSeek>,
    playlist: &SfxPlaylist,
) -> Result<Vec<SfxGroupDefinition>> {
    let mut group_indexes: Vec<(&'static str, u32)> = vec![];
    for &(name, _) in SFX_GROUPS {
        let path = format!("{}/{}{}", SOUND_BANK_DIR, name, SOUND_BANK_EXT);
        let mut reader = disc.open_file_at(&path)?;
        reader.seek(SeekFrom::Start(0xc))?;
        let base_index = reader.read_u32::<BE>()?;
        group_indexes.push((name, base_index));
    }
    group_indexes.sort_unstable_by_key(|(_, b)| *b);

    let mut groups = vec![];
    for (id, ((name, sample_index), &material_index)) in
        group_indexes.into_iter().zip(&playlist.group_indexes).enumerate()
    {
        let label = Label::pascal_case(name.strip_prefix(SOUND_BANK_PREFIX).unwrap());
        groups.push(SfxGroupDefinition {
            id: id as i16,
            label,
            first_sample: sample_index,
            first_material: material_index,
            name: name.to_owned(),
        });
    }
    Ok(groups)
}

/// Representation of a sound effect which is written to the generated source.
struct SfxDefinition {
    id: u32,
    label: Label,
    name: String,
}

/// Builds the SFX list by matching sounds in a BRSAR with a playlist.
fn build_sfx(
    playlist: &SfxPlaylist,
    brsar: &Brsar,
    groups: &[SfxGroupDefinition],
) -> Vec<SfxDefinition> {
    #[derive(Default, Copy, Clone)]
    struct GroupState {
        next_id: u32,
        end_id: u32,
    }

    // Compute each bank's starting ID and ending ID. NPC has sounds that the GCN version doesn't,
    // so we have to make sure not to emit definitions for extra sounds.
    let mut states = vec![GroupState::default(); groups.len()];
    let mut end_index = playlist.sounds.len() as u32;
    for (id, &base) in playlist.group_indexes.iter().enumerate().rev() {
        states[id].next_id = (id as u32) << 16;
        states[id].end_id = states[id].next_id + end_index - base;
        end_index = base;
    }

    // Build a map of collection indexes to bank indexes
    let mut collection_to_group: HashMap<u32, usize> = HashMap::new();
    for (i, collection) in brsar.collections.iter().enumerate() {
        if !collection.groups.is_empty() {
            let group_index = collection.groups[0].index;
            let group = &brsar.groups[group_index as usize];
            let group_name = brsar.symbol(group.name_index);
            let bank_name = SFX_GROUPS.iter().find(|&&(_, g)| g == group_name);
            if let Some(&(name, _)) = bank_name {
                let group_def = groups.iter().find(|b| b.name == name).unwrap();
                let group_index = group_def.id as usize;
                collection_to_group.insert(i as u32, group_index);
            } else {
                warn!("No GCN group found for Wii group \"{}\"", group_name);
            }
        }
    }

    // Look up each sound's corresponding bank and give it the next ID for the bank. It turns out
    // that the order sounds are stored in the BRSAR matches the order of the sounds in the .sem.
    let mut defs = Vec::with_capacity(playlist.sounds.len());
    for sound in &brsar.sounds {
        if let Some(&bank_id) = collection_to_group.get(&sound.collection_index) {
            let bank = &mut states[bank_id];
            if bank.next_id < bank.end_id {
                // Convert sound names to lowercase. While the names in the BRSAR are in uppercase,
                // every file in the GameCube build uses lowercase, so this makes the sound file
                // names more consistent with everything else.
                let name = brsar.symbol(sound.name_index).to_lowercase();
                let label = Label::pascal_case(&name);
                defs.push(SfxDefinition { id: bank.next_id, label, name });
                bank.next_id += 1;
            }
        }
    }
    debug!("Found names for {}/{} sound effects", defs.len(), playlist.sounds.len());

    // Fill in any IDs we somehow missed
    for bank in &mut states {
        while bank.next_id < bank.end_id {
            let name = format!("unk_{:>06x}", bank.next_id);
            let label = Label::pascal_case(&name);
            defs.push(SfxDefinition { id: bank.next_id, label, name });
            bank.next_id += 1;
        }
    }
    defs.sort_unstable_by_key(|d| d.id);
    defs
}

/// Representation of a sound which is written to the generated source.
struct SfxSampleDefinition {
    id: u32,
    label: Label,
    name: String,
}

/// Builds the sample list by matching effect names.
fn build_sfx_samples(
    playlist: &SfxPlaylist,
    sfx_defs: &[SfxDefinition],
) -> Vec<SfxSampleDefinition> {
    let mut defs: Vec<SfxSampleDefinition> = Vec::with_capacity(sfx_defs.len());
    for (sfx, def) in playlist.sounds.iter().zip(sfx_defs) {
        if DUPLICATE_SOUND_DISCARDS.is_match(&def.name) {
            continue;
        }
        if let Some(id) = sfx.sample_id() {
            defs.push(SfxSampleDefinition { id, label: def.label.clone(), name: def.name.clone() });
        }
    }
    debug!("Found {} sound names", defs.len());
    defs.sort_unstable_by_key(|d| d.id);
    defs
}

fn read_stage_actors(reader: &mut (impl Read + Seek), out: &mut BTreeMap<i32, i32>) -> Result<()> {
    reader.seek(SeekFrom::Start(0x4))?;
    let objects_offset = reader.read_u32::<LE>()?;
    reader.seek(SeekFrom::Start(objects_offset as u64))?;
    let objects = NonNoneList::<ObjectPlacement>::read_from(reader)?.into_vec();

    reader.seek(SeekFrom::Start(0x24))?;
    let actors_offset = reader.read_u32::<LE>()?;
    reader.seek(SeekFrom::Start(actors_offset as u64))?;
    let actors = NonNoneList::<Actor>::read_from(reader)?;

    for actor in actors {
        if let Some(object) = objects.get(actor.obj as usize) {
            out.insert(actor.id, object.id);
        }
    }
    Ok(())
}

/// Writes the list of objects to the generated file.
fn write_objects(mut writer: impl Write, objects: &[ObjectDefinition]) -> Result<()> {
    write!(writer, "{}{}", GEN_HEADER, OBJECTS_HEADER)?;
    for object in objects {
        writeln!(
            writer,
            "    {} => {} {{ \"{}\", {:?}, {}, \"{}\" }},",
            object.id,
            object.label.0,
            object.name.0,
            object.class,
            object.subclass,
            object.model_path,
        )?;
    }
    write!(writer, "{}", OBJECTS_FOOTER)?;
    writer.flush()?;
    Ok(())
}

/// Writes the list of spawnables to the generated file.
fn write_spawnables(
    mut writer: impl Write,
    spawnables: &[Spawnable],
    objects: &[ObjectDefinition],
) -> Result<()> {
    write!(writer, "{}{}", GEN_HEADER, SPAWNABLES_HEADER)?;
    for (i, spawnable) in spawnables.iter().enumerate() {
        let placement = spawnable.placement;
        let object = object_index(placement.id);
        let object_name = &objects[object].label.0;
        writeln!(
            writer,
            "    {} => {} {{ \"{}\", {}, ({}, {}, {}), ({}, {}, {}), {}, {:?} }},",
            i,
            spawnable.label.0,
            spawnable.name.0,
            object_name,
            placement.x,
            placement.y,
            placement.z,
            placement.scale_x,
            placement.scale_y,
            placement.scale_z,
            placement.variant,
            placement.flags,
        )?;
    }
    write!(writer, "{}", SPAWNABLES_FOOTER)?;
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
        let flags: String = item.flags.iter().fold(String::new(), |mut s, f| {
            let _ = write!(s, ", {}", f);
            s
        });
        writeln!(
            writer,
            "    {} => {} {{ \"{}\", {}{} }},",
            item.id, item.label.0, item.name.0, object, flags
        )?;
    }
    write!(writer, "{}", ITEMS_FOOTER)?;
    writer.flush()?;
    Ok(())
}

/// Writes the list of ATCs to the generated file.
fn write_atcs(mut writer: impl Write, atcs: &[AtcDefinition]) -> Result<()> {
    write!(writer, "{}{}", GEN_HEADER, ATCS_HEADER)?;
    for atc in atcs {
        writeln!(writer, "    {} => {} {{ \"{}\" }},", atc.id, atc.label.0, atc.name.0)?;
    }
    write!(writer, "{}", ATCS_FOOTER)?;
    writer.flush()?;
    Ok(())
}

/// Writes the list of suits to the generated file.
fn write_suits(mut writer: impl Write, suits: &[SuitDefinition]) -> Result<()> {
    write!(writer, "{}{}", GEN_HEADER, SUITS_HEADER)?;
    for suit in suits {
        writeln!(
            writer,
            "    {} => {} {{ \"{}\", {} }},",
            suit.id, suit.label.0, suit.name.0, suit.item.0
        )?;
    }
    write!(writer, "{}", SUITS_FOOTER)?;
    writer.flush()?;
    Ok(())
}

/// Writes the list of stages to the generated file.
fn write_stages(mut writer: impl Write, stages: &[StageDefinition]) -> Result<()> {
    write!(writer, "{}{}", GEN_HEADER, STAGES_HEADER)?;
    for stage in stages {
        writeln!(
            writer,
            "    {} => {} {{ \"{}\", \"{}\" }},",
            stage.id, stage.label.0, stage.name, stage.title
        )?;
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

/// Writes the list of SFX groups to the generated file.
fn write_sfx_groups(mut writer: impl Write, groups: &[SfxGroupDefinition]) -> Result<()> {
    write!(writer, "{}{}", GEN_HEADER, SFX_GROUPS_HEADER)?;
    for group in groups {
        writeln!(
            writer,
            "    {} => {} {{ 0x{:>04x}, 0x{:>04x}, \"{}\" }},",
            group.id, group.label.0, group.first_sample, group.first_material, group.name
        )?;
    }
    write!(writer, "{}", SFX_GROUPS_FOOTER)?;
    writer.flush()?;
    Ok(())
}

/// Writes the list of sound effects to the generated file.
fn write_sfx(mut writer: impl Write, sfx_defs: &[SfxDefinition]) -> Result<()> {
    write!(writer, "{}{}", GEN_HEADER_NPC, SFX_HEADER)?;
    for sfx in sfx_defs {
        writeln!(writer, "    0x{:>06x} => {} {{ \"{}\" }},", sfx.id, sfx.label.0, sfx.name)?;
    }
    write!(writer, "{}", SFX_FOOTER)?;
    writer.flush()?;
    Ok(())
}

/// Writes the list of sounds to the generated file.
fn write_sfx_samples(mut writer: impl Write, samples: &[SfxSampleDefinition]) -> Result<()> {
    write!(writer, "{}{}", GEN_HEADER_NPC, SFX_SAMPLES_HEADER)?;
    for sound in samples {
        writeln!(writer, "    {} => {} {{ \"{}\" }},", sound.id, sound.label.0, sound.name)?;
    }
    write!(writer, "{}", SFX_SAMPLES_FOOTER)?;
    writer.flush()?;
    Ok(())
}

/// Writes the list of animations to the generated file.
fn write_animations(
    mut writer: impl Write,
    anims: &[AnimationDefinition],
    objects: &[ObjectDefinition],
) -> Result<()> {
    write!(writer, "{}{}", GEN_HEADER_NPC, ANIMATIONS_HEADER)?;
    let mut last_object = None;
    for anim in anims {
        if last_object != Some(anim.object) {
            if last_object.is_some() {
                writeln!(writer, "    }}")?;
            }
            writeln!(writer, "    {} {{", objects[anim.object as usize].label.0)?;
            last_object = Some(anim.object);
        }
        writeln!(writer, "        {} => {} {{ \"{}\" }},", anim.index, anim.label.0, anim.name,)?;
    }
    writeln!(writer, "    }}")?;
    write!(writer, "{}", ANIMATIONS_FOOTER)?;
    writer.flush()?;
    Ok(())
}

fn write_actors(
    mut writer: impl Write,
    actors: &BTreeMap<i32, i32>,
    metadata: &Metadata,
    objects: &[ObjectDefinition],
) -> Result<()> {
    write!(writer, "{}{}", GEN_HEADER, ACTORS_HEADER)?;
    for (i, actor) in metadata.actors.iter().enumerate() {
        let actor_index = i as i32;
        let actor_id = actor_index + 20000;
        let actor_name = actor.name.decode()?;
        let mut actor_short_name = actor_name.clone().into_owned();
        for fixup in &*ACTOR_NAME_FIXUPS {
            if let Cow::Owned(owned) = fixup.replace(&actor_short_name, "") {
                actor_short_name = owned;
            }
        }
        let object_id = actors.get(&actor_id).copied();
        let object = object_id.map(|i| &objects[object_index(i)]);
        let enum_label = if !actor_short_name.is_empty() {
            Label::pascal_case(&actor_short_name)
        } else {
            Label(format!("{}{}", UNKNOWN_PREFIX, actor_index))
        };
        let string_label = Label::snake_case(&enum_label.0);
        writeln!(
            writer,
            "    {} => {} {{ \"{}\", \"{}\", {} }},",
            actor_id,
            &enum_label.0,
            &string_label.0,
            actor_name,
            object.map(|o| o.label.0.as_str()).unwrap_or("None"),
        )?;
    }
    write!(writer, "{}", ACTORS_FOOTER)?;
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

    info!("Reading SFX playlist");
    let playlist = {
        let mut reader = BufReader::new(iso.open_file_at(SOUND_PLAYLIST_PATH)?);
        SfxPlaylist::read_from(&mut reader)?
    };

    info!("Reading SFX groups");
    let sfx_groups = read_sfx_groups(&mut iso, &playlist)?;

    info!("Reading globals.bin");
    let globals_path = format!("{}/{}", QP_STAGES_PATH, QP_GLOBALS_NAME);
    let metadata = {
        let mut qp = ArchiveReader::open(iso.open_file_at(QP_PATH)?)?;
        let mut globals = GlobalsReader::open(qp.open_file_at(&globals_path)?)?;
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

    info!("Reading spawnables");
    let spawnables = read_spawnables(&dol, &mut dol_reader, &objects)?;

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
    drop(dol_reader);

    let actors = {
        let mut qp = ArchiveReader::open(iso.open_file_at(QP_PATH)?)?;
        let mut results = BTreeMap::new();
        for stage in stages.iter().filter(|s| s.id < 100) {
            info!("Reading {}.bin", stage.name);
            let path = format!("{}/{}.bin", QP_STAGES_PATH, stage.name);
            let mut stage_reader = qp.open_file_at(&path)?;
            read_stage_actors(&mut stage_reader, &mut results)?;
        }
        results
    };

    let mut sfx = vec![];
    let mut sfx_samples = vec![];
    let mut animations = vec![];
    if let Some(npc_path) = options.npc {
        info!("Reading NPC sounds");
        let brsar_path = Path::new(&npc_path).join(NPC_BRSAR_PATH);
        let mut brsar_reader = BufReader::new(File::open(brsar_path)?);
        let brsar = Brsar::read_from(&mut brsar_reader)?;

        info!("Matching sound effect names");
        sfx = build_sfx(&playlist, &brsar, &sfx_groups);
        sfx_samples = build_sfx_samples(&playlist, &sfx);

        info!("Opening NPC main.dol");
        let dol_path = Path::new(&npc_path).join(NPC_DOL_PATH);
        let mut dol_reader = BufReader::new(File::open(dol_path)?);
        let dol = DolHeader::read_from(&mut dol_reader)?;

        info!("Matching animation names");
        animations =
            read_npc_animations(&dol, &mut dol_reader, NPC_OBJECTS_ADDR, NUM_MAIN_OBJECTS)?;
        deduplicate_animations(&mut animations);
    }

    let out_dir: PathBuf = [UNPLUG_DATA_PATH, SRC_DIR_NAME, OUTPUT_DIR_NAME].iter().collect();
    fs::create_dir_all(&out_dir)?;

    let objects_path = out_dir.join(OBJECTS_FILE_NAME);
    info!("Writing {}", objects_path.display());
    let objects_writer = BufWriter::new(File::create(objects_path)?);
    write_objects(objects_writer, &objects)?;

    let spawnables_path = out_dir.join(SPAWNABLES_FILE_NAME);
    info!("Writing {}", spawnables_path.display());
    let spawnables_writer = BufWriter::new(File::create(spawnables_path)?);
    write_spawnables(spawnables_writer, &spawnables, &objects)?;

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

    let actors_path = out_dir.join(ACTORS_FILE_NAME);
    info!("Writing {}", actors_path.display());
    let actors_writer = BufWriter::new(File::create(actors_path)?);
    write_actors(actors_writer, &actors, &metadata, &objects)?;

    let music_path = out_dir.join(MUSIC_FILE_NAME);
    info!("Writing {}", music_path.display());
    let music_writer = BufWriter::new(File::create(music_path)?);
    write_music(music_writer, &music)?;

    let sfx_groups_path = out_dir.join(SFX_GROUPS_FILE_NAME);
    info!("Writing {}", sfx_groups_path.display());
    let sfx_groups_writer = BufWriter::new(File::create(sfx_groups_path)?);
    write_sfx_groups(sfx_groups_writer, &sfx_groups)?;

    if !sfx.is_empty() {
        let sfx_path = out_dir.join(SFX_FILE_NAME);
        info!("Writing {}", sfx_path.display());
        let sfx_writer = BufWriter::new(File::create(sfx_path)?);
        write_sfx(sfx_writer, &sfx)?;
    }

    if !sfx_samples.is_empty() {
        let sfx_samples_path = out_dir.join(SFX_SAMPLES_FILE_NAME);
        info!("Writing {}", sfx_samples_path.display());
        let sfx_samples_writer = BufWriter::new(File::create(sfx_samples_path)?);
        write_sfx_samples(sfx_samples_writer, &sfx_samples)?;
    }

    if !animations.is_empty() {
        let animations_path = out_dir.join(ANIMATIONS_FILE_NAME);
        info!("Writing {}", animations_path.display());
        let animations_writer = BufWriter::new(File::create(animations_path)?);
        write_animations(animations_writer, &animations, &objects)?;
    }

    Ok(())
}

fn main() {
    process::exit(match run_app() {
        Ok(()) => 0,
        Err(err) => {
            error!("Fatal: {:#}", err);
            1
        }
    });
}
