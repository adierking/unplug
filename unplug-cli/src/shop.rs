use crate::context::Context;
use crate::id::IdString;
use crate::io::OutputRedirect;
use crate::opt::{ShopCommand, ShopExportOpt, ShopImportOpt};
use anyhow::{bail, Error, Result};
use lazy_static::lazy_static;
use log::{error, info, warn};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::ser::{Formatter, Serializer};
use std::collections::HashSet;
use std::convert::TryFrom;
use std::fs::File;
use std::io::{self, BufReader, BufWriter};
use unplug::data::stage::{Stage, StageDefinition};
use unplug::data::{Atc, Item};
use unplug::globals::{GlobalsBuilder, Metadata};
use unplug::shop::{Requirement, Shop, Slot, NUM_SLOTS};

const FLAG_PREFIX: &str = "flag";
lazy_static! {
    static ref FLAG_REGEX: Regex = Regex::new(r"^flag\((\d+)\)$").unwrap();
}

/// Formatter specifically designed for making shop data look clean. Hacky and probably doesn't work
/// well with anything other than an array of `ShopModel`s. The main difference between this and the
/// default pretty formatting is that the `requires` arrays get written on one line.
struct ShopFormatter {
    array_level: usize,
}

impl ShopFormatter {
    fn new() -> Self {
        Self { array_level: 0 }
    }
}

impl Formatter for ShopFormatter {
    fn begin_array<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.array_level += 1;
        writer.write_all(b"[")
    }

    fn end_array<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        self.array_level -= 1;
        if self.array_level == 0 {
            writer.write_all(b"\n]")
        } else {
            writer.write_all(b"]")
        }
    }

    fn begin_array_value<W>(&mut self, writer: &mut W, first: bool) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        if !first {
            writer.write_all(b",")?;
        }
        if self.array_level == 1 {
            writer.write_all(b"\n  ")?;
        } else if !first {
            writer.write_all(b" ")?;
        }
        Ok(())
    }

    fn begin_object<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        writer.write_all(b"{")
    }

    fn end_object<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        writer.write_all(b"\n  }")
    }

    fn begin_object_key<W>(&mut self, writer: &mut W, first: bool) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        if first {
            writer.write_all(b"\n    ")
        } else {
            writer.write_all(b",\n    ")
        }
    }

    fn begin_object_value<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + io::Write,
    {
        writer.write_all(b": ")
    }
}

/// Parses a requirement string into a `Requirement`.
fn parse_requirement(s: &str) -> Result<Requirement> {
    // Requirement strings can take 3 different forms. We try each one in order:
    // 1. "flag(INDEX)"
    // 2. ATC ID
    // 3. Item ID
    if let Some(captures) = FLAG_REGEX.captures(s) {
        let flag_str = captures.get(1).unwrap().as_str();
        let flag = match flag_str.parse::<i32>() {
            Ok(flag) => flag,
            Err(_) => bail!("Invalid flag index: {}", flag_str),
        };
        Ok(Requirement::HaveFlag(flag))
    } else if let Ok(atc) = Atc::try_from_id(s) {
        Ok(Requirement::HaveAtc(atc))
    } else if let Ok(item) = Item::try_from_id(s) {
        Ok(Requirement::HaveItem(item))
    } else {
        bail!("Invalid requirement: \"{}\"", s);
    }
}

/// A slot as represented in the exported file.
#[derive(Serialize, Deserialize)]
struct SlotModel {
    /// Item ID string
    item: Option<String>,
    /// Item price
    price: i16,
    /// Item limit (1-10)
    limit: i16,
    /// Item requirements
    requires: Vec<String>,
}

impl SlotModel {
    /// Creates a new `SlotModel` from `slot` with `price`.
    fn with_slot_and_price(slot: &Slot, price: i16) -> Self {
        let mut requires = vec![];
        for requirement in &slot.requirements {
            match requirement {
                Requirement::HaveItem(item) => requires.push(item.to_id().to_owned()),
                Requirement::HaveAtc(atc) => requires.push(atc.to_id().to_owned()),
                Requirement::HaveFlag(flag) => requires.push(format!("{}({})", FLAG_PREFIX, flag)),
                _ => warn!("Unsupported requirement: {:?}", requirement),
            }
        }
        requires.sort_unstable();
        Self { item: slot.item.map(|i| i.to_id().to_owned()), price, limit: slot.limit, requires }
    }

    /// Creates a new `SlotModel` from `slot`, retrieving the price from `globals`.
    fn with_slot_and_globals(slot: &Slot, globals: &Metadata) -> Self {
        let price = if let Some(item) = slot.item {
            let index = i16::from(item) as usize;
            globals.items[index].price
        } else {
            0
        };
        Self::with_slot_and_price(slot, price)
    }
}

impl TryFrom<&SlotModel> for Slot {
    type Error = Error;
    fn try_from(model: &SlotModel) -> Result<Self> {
        let item = if let Some(item_str) = &model.item {
            match Item::try_from_id(item_str) {
                Ok(item) => Some(item),
                Err(_) => bail!("Invalid item ID: \"{}\"", item_str),
            }
        } else {
            None
        };
        let mut requirements = HashSet::new();
        for requirement in &model.requires {
            requirements.insert(parse_requirement(requirement)?);
        }
        Ok(Self { item, limit: model.limit, requirements })
    }
}

pub fn command(ctx: Context, opt: ShopCommand) -> Result<()> {
    match opt {
        ShopCommand::Export(opt) => command_export(ctx, opt),
        ShopCommand::Import(opt) => command_import(ctx, opt),
    }
}

pub fn command_export(ctx: Context, opt: ShopExportOpt) -> Result<()> {
    let mut ctx = ctx.open_read()?;
    let out = BufWriter::new(OutputRedirect::new(opt.output)?);

    info!("Reading global metadata");
    let mut globals = ctx.read_globals()?;
    let metadata = globals.read_metadata()?;

    info!("Reading script globals");
    let libs = globals.read_libs()?;

    let chibi_house = StageDefinition::get(Stage::ChibiHouse);
    info!("Reading {}.bin", chibi_house.name);
    let stage = ctx.read_stage(&libs, Stage::ChibiHouse)?;

    info!("Parsing shop code");
    let shop = Shop::parse(&stage.script)?;

    info!("Writing to JSON");
    let slots: Vec<_> =
        shop.slots().iter().map(|i| SlotModel::with_slot_and_globals(i, &metadata)).collect();
    if opt.compact {
        serde_json::to_writer(out, &slots)?;
    } else {
        let mut serializer = Serializer::with_formatter(out, ShopFormatter::new());
        slots.serialize(&mut serializer)?;
    }

    Ok(())
}

pub fn command_import(ctx: Context, opt: ShopImportOpt) -> Result<()> {
    let mut ctx = ctx.open_read_write()?;
    info!("Reading input JSON");
    let json = BufReader::new(File::open(opt.input)?);
    let models: Vec<SlotModel> = serde_json::from_reader(json)?;

    let mut slots: Vec<Slot> = vec![];
    let mut items = HashSet::new();
    let mut has_duplicate = false;
    for model in &models {
        let slot = Slot::try_from(model)?;
        if let Some(item) = slot.item {
            if !items.insert(item) {
                error!("{} appears in the shop more than once", item.to_id());
                has_duplicate = true;
            }
        }
        slots.push(slot);
    }
    if has_duplicate {
        bail!("duplicate shop items are not allowed");
    }

    if slots.len() > NUM_SLOTS {
        warn!(
            "The input file has too many slots ({} > {}). Excess slots will be discarded.",
            slots.len(),
            NUM_SLOTS
        );
        slots.truncate(NUM_SLOTS);
    }

    info!("Reading global metadata");
    let mut globals = ctx.read_globals()?;
    let mut metadata = globals.read_metadata()?;

    info!("Reading script globals");
    let libs = globals.read_libs()?;

    let chibi_house = StageDefinition::get(Stage::ChibiHouse);
    info!("Reading {}.bin", chibi_house.name);
    let mut stage = ctx.read_stage(&libs, Stage::ChibiHouse)?;

    info!("Compiling new shop code");
    for (slot, model) in slots.iter().zip(&models) {
        if let Some(item) = slot.item {
            let index = i16::from(item) as usize;
            metadata.items[index].price = model.price;
        }
    }
    let shop = Shop::with_slots(slots);
    shop.compile(&mut stage.script)?;

    info!("Updating game files");
    ctx.begin_update()
        .write_globals(GlobalsBuilder::new().base(&mut globals).metadata(&metadata))?
        .write_stage(Stage::ChibiHouse, stage)?
        .commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Convenience macro for initializing HashSets
    macro_rules! set {
        [$($value:expr),* $(,)*] => {
            vec![$($value),*].into_iter().collect::<::std::collections::HashSet<_>>()
        };
    }

    #[test]
    fn test_parse_requirement() -> Result<()> {
        assert_eq!(parse_requirement("hot-rod")?, Requirement::HaveItem(Item::HotRod));
        assert_eq!(parse_requirement("toothbrush")?, Requirement::HaveAtc(Atc::Toothbrush));
        assert!(parse_requirement("HoT-rOd").is_err());
        assert!(parse_requirement("maid-outfit").is_err());
        assert!(parse_requirement("").is_err());

        assert_eq!(parse_requirement("flag(123)")?, Requirement::HaveFlag(123));
        assert!(parse_requirement("FlAg(123)").is_err());
        assert!(parse_requirement("flag(-1)").is_err());
        assert!(parse_requirement("flag()").is_err());
        assert!(parse_requirement("xXxflag(123)xXx").is_err());
        Ok(())
    }

    #[test]
    fn test_slot_model_from_slot() {
        let slot = Slot {
            item: Some(Item::HotRod),
            limit: 5,
            requirements: set![
                Requirement::HaveItem(Item::SpaceScrambler),
                Requirement::HaveAtc(Atc::Toothbrush),
                Requirement::HaveFlag(123),
            ],
        };
        let model = SlotModel::with_slot_and_price(&slot, 42);
        assert_eq!(model.item, Some("hot-rod".into()));
        assert_eq!(model.price, 42);
        assert_eq!(model.limit, 5);
        assert_eq!(model.requires, vec!["flag(123)", "space-scrambler", "toothbrush"]);
    }

    #[test]
    fn test_slot_model_from_slot_empty() {
        let slot = Slot { item: None, limit: 0, requirements: set![] };
        let model = SlotModel::with_slot_and_price(&slot, 0);
        assert_eq!(model.item, None);
        assert_eq!(model.price, 0);
        assert_eq!(model.limit, 0);
        assert!(model.requires.is_empty());
    }

    #[test]
    fn test_slot_from_slot_model() {
        let model = SlotModel {
            item: Some("hot-rod".into()),
            price: 42,
            limit: 5,
            requires: vec!["flag(123)".into(), "space-scrambler".into(), "toothbrush".into()],
        };
        let slot = Slot::try_from(&model).unwrap();
        assert_eq!(slot.item, Some(Item::HotRod));
        assert_eq!(slot.limit, 5);
        assert_eq!(
            slot.requirements,
            set![
                Requirement::HaveItem(Item::SpaceScrambler),
                Requirement::HaveAtc(Atc::Toothbrush),
                Requirement::HaveFlag(123),
            ]
        );
    }

    #[test]
    fn test_slot_from_slot_model_empty() {
        let model = SlotModel { item: None, price: 0, limit: 0, requires: vec![] };
        let slot = Slot::try_from(&model).unwrap();
        assert_eq!(slot.item, None);
        assert_eq!(slot.limit, 0);
        assert!(slot.requirements.is_empty());
    }
}
