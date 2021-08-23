use super::{Requirement, Shop, Slot};
use crate::common::*;
use crate::id::IdString;
use crate::io::OutputRedirect;
use crate::opt::ExportShopOpt;
use anyhow::Result;
use log::{info, warn};
use serde::{Deserialize, Serialize};
use serde_json::ser::{Formatter, Serializer};
use std::io::{self, BufWriter};
use unplug::{data::stage::CHIBI_HOUSE, globals::Metadata};

const FLAG_PREFIX: &str = "flag";

/// Formatter specifically designed for making shop data look clean. Hacky and probably doesn't work
/// well with anything other than an array of `ShopDef`s. The main difference between this and the
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

/// A slot as represented in shop JSON.
#[derive(Serialize, Deserialize)]
struct SlotDef {
    item: Option<String>,
    price: i16,
    limit: i16,
    requires: Vec<String>,
}

impl SlotDef {
    /// Creates a new `SlotDef` from `slot` with `price`.
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

    /// Creates a new `SlotDef` from `slot`, retrieving the price from `globals`.
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

pub fn export_shop(opt: ExportShopOpt) -> Result<()> {
    let out = BufWriter::new(OutputRedirect::new(opt.output)?);

    let mut iso = open_iso_optional(opt.container.iso.as_ref())?;
    let mut qp = open_qp_required(iso.as_mut(), &opt.container)?;

    info!("Reading global metadata");
    let mut globals = read_globals_qp(&mut qp)?;
    let metadata = globals.read_metadata()?;

    info!("Reading script globals");
    let libs = globals.read_libs()?;

    info!("Reading stage file");
    let stage = read_stage_qp(&mut qp, CHIBI_HOUSE.name, &libs)?;

    info!("Parsing shop code");
    let shop = Shop::parse(&stage.script)?;

    info!("Writing to JSON");
    let slots: Vec<_> =
        shop.slots().iter().map(|i| SlotDef::with_slot_and_globals(i, &metadata)).collect();
    if opt.compact {
        serde_json::to_writer(out, &slots)?;
    } else {
        let mut serializer = Serializer::with_formatter(out, ShopFormatter::new());
        slots.serialize(&mut serializer)?;
    }

    Ok(())
}
