#![warn(
    absolute_paths_not_starting_with_crate,
    elided_lifetimes_in_paths,
    explicit_outlives_requirements,
    single_use_lifetimes,
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

use anyhow::{bail, Result};
use byteorder::{ReadBytesExt, BE};
use lazy_static::lazy_static;
use log::{error, info, trace};
use num_enum::TryFromPrimitive;
use regex::Regex;
use simplelog::{Color, ConfigBuilder, Level, LevelFilter, TermLogger, TerminalMode};
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
use unplug::common::ReadFrom;
use unplug::data::stage::GLOBALS_PATH;
use unplug::dvd::{ArchiveReader, DiscStream, DolHeader, OpenFile};
use unplug::globals::metadata::Item;
use unplug::globals::GlobalsReader;

const MAIN_OBJECTS_ADDR: u32 = 0x8021c70c;
const NUM_MAIN_OBJECTS: usize = 1162;

const INTERNAL_OBJECTS_ADDR: u32 = 0x80223690;
const NUM_INTERNAL_OBJECTS: usize = 36;
const INTERNAL_OBJECTS_BASE_ID: u32 = 10000;

const QP_PATH: &str = "qp.bin";

const INTERNAL_PREFIX: &str = "Internal";
const UNKNOWN_PREFIX: &str = "Unk";

const NUM_ITEMS: usize = 159;
const STRIP_ITEM_LABEL: &str = "Item";

const UNPLUG_DATA_PATH: &str = "unplug-data";
const SRC_DIR_NAME: &str = "src";
const TEST_FILE_NAME: &str = "lib.rs";
const OUTPUT_DIR_NAME: &str = "gen";

const ITEMS_FILE_NAME: &str = "items.inc.rs";
const OBJECTS_FILE_NAME: &str = "objects.inc.rs";

const GEN_HEADER: &str = "// Generated with unplug-datagen. DO NOT EDIT.\n\
                          // To regenerate: cargo run -p unplug-datagen -- <iso path>\n\n";

const OBJECTS_HEADER: &str = "declare_objects! {\n";
const OBJECTS_FOOTER: &str = "}\n";

const ITEMS_HEADER: &str = "declare_items! {\n";
const ITEMS_FOOTER: &str = "}\n";

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
        // There are two broken bottle items and they both have the display name "Broken Bottle"
        ("SoukoWineBottleA".into(), "BrokenBottleA"),
        ("SoukoWineBottleB".into(), "BrokenBottleB"),
    ].into_iter().collect();
}

fn init_logging() {
    let config = ConfigBuilder::new()
        .set_thread_level(LevelFilter::Off)
        .set_target_level(LevelFilter::Trace)
        .set_time_format_str("%T%.3f")
        .set_level_color(Level::Info, Color::Green)
        .build();
    TermLogger::init(LevelFilter::Debug, config, TerminalMode::Stderr).unwrap();
}

fn usage() {
    eprintln!("Usage: cargo run -p unplug-datagen -- <iso>");
}

/// The raw representation of an object in the executable.
#[derive(Debug, Copy, Clone, Default)]
struct RawObjectDefinition {
    model_addr: u32,
    unk_04: u32,
    unk_08: u32,
    unk_0c: u32,
    unk_10: u32,
    class: u16,
    subclass: u16,
}

impl<R: Read> ReadFrom<R> for RawObjectDefinition {
    type Error = anyhow::Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self {
            model_addr: reader.read_u32::<BE>()?,
            unk_04: reader.read_u32::<BE>()?,
            unk_08: reader.read_u32::<BE>()?,
            unk_0c: reader.read_u32::<BE>()?,
            unk_10: reader.read_u32::<BE>()?,
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
    display_name: String,
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
                Some(ItemDefinition {
                    id: object.subclass,
                    label,
                    object: Some(object.label.clone()),
                    display_name: display_name.into(),
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
            items
                .insert(i, ItemDefinition { id, label, object: None, display_name: String::new() });
        }
    }
    assert_eq!(items.len(), NUM_ITEMS);
    items
}

/// Writes the list of objects to the generated file.
fn write_objects(mut writer: impl Write, objects: &[ObjectDefinition]) -> Result<()> {
    write!(writer, "{}", GEN_HEADER)?;
    write!(writer, "{}", OBJECTS_HEADER)?;
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
    write!(writer, "{}", GEN_HEADER)?;
    write!(writer, "{}", ITEMS_HEADER)?;
    for item in items {
        let object = match &item.object {
            Some(label) => &label.0,
            _ => "None",
        };
        writeln!(
            writer,
            "    {} => {} {{ {}, \"{}\" }},",
            item.id, item.label.0, object, item.display_name
        )?;
    }
    write!(writer, "{}", ITEMS_FOOTER)?;
    writer.flush()?;
    Ok(())
}

fn run_app() -> Result<()> {
    init_logging();

    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        usage();
        return Ok(());
    }

    let test_path: PathBuf = [UNPLUG_DATA_PATH, SRC_DIR_NAME, TEST_FILE_NAME].iter().collect();
    if !test_path.exists() {
        bail!(
            "Could not locate {}. Make sure unplug-datagen is run from the workspace root.",
            test_path.display(),
        );
    }

    info!("Opening main.dol");
    let mut iso = DiscStream::open(File::open(&args[1])?)?;
    let (dol, dol_reader) = iso.open_dol()?;
    let mut dol_reader = BufReader::new(dol_reader);

    info!("Reading object tables");
    let mut objects = read_objects(&dol, &mut dol_reader, MAIN_OBJECTS_ADDR, NUM_MAIN_OBJECTS)?;
    let internal_objects =
        read_objects(&dol, &mut dol_reader, INTERNAL_OBJECTS_ADDR, NUM_INTERNAL_OBJECTS)?;
    drop(dol_reader);

    info!("Generating object data");
    for mut object in internal_objects {
        object.id += INTERNAL_OBJECTS_BASE_ID;
        object.label.0.insert_str(0, INTERNAL_PREFIX);
        objects.push(object);
    }
    deduplicate_labels(&mut objects);
    fixup_labels(&mut objects);

    info!("Opening {}", QP_PATH);
    let mut qp = ArchiveReader::open(iso.open_file_at(QP_PATH)?)?;

    info!("Reading globals.bin");
    let mut globals = GlobalsReader::open(qp.open_file_at(GLOBALS_PATH)?)?;
    let metadata = globals.read_metadata()?;

    info!("Generating item data");
    let items = build_items(&objects, &metadata.items);

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
