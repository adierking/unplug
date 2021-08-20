mod parser;
use parser::*;
use unplug::event::Script;

use crate::common::*;
use crate::opt::ShopTestOpt;
use anyhow::{bail, Result};
use log::{debug, info};
use std::collections::HashSet;
use unplug::data::atc::AtcId;
use unplug::data::item::ItemId;
use unplug::data::stage::CHIBI_HOUSE;
use unplug::event::analysis::Label;
use unplug::event::{BlockId, Command, Ip, SetExpr};

const NUM_SLOTS: usize = 20;
const SHOP_ITEM_FIRST: usize = 600;
const SHOP_ITEM_LAST: usize = SHOP_ITEM_FIRST + NUM_SLOTS - 1;
const SHOP_COUNT_FIRST: usize = SHOP_ITEM_LAST + 1;
const SHOP_COUNT_LAST: usize = SHOP_COUNT_FIRST + NUM_SLOTS - 1;

fn find_shop_setup(script: &Script) -> Result<BlockId> {
    // There should only be one subroutine in the script which builds the shop, and our goal is to
    // find it. We can use the analyzer output to greatly limit our search space - the routine will
    // be marked as killing the shop vars. Every routine which calls the shop routine will also
    // technically kill the shop vars, so if we sort by the number of killed labels then the first
    // one is likely to be our goal.
    let layout = script.layout().expect("missing script layout");
    let mut candidates: Vec<_> = layout
        .subroutine_effects()
        .iter()
        .filter(|(_, e)| e.killed.contains(&Label::Variable(SHOP_ITEM_FIRST as i16)))
        .map(|(block, e)| (*block, e))
        .collect();
    candidates.sort_unstable_by_key(|(_, e)| e.killed.len());
    for (block, _) in candidates {
        debug!("Checking candidate block {:?}", block);
        if is_shop_setup(script, block, &mut HashSet::new()) {
            return Ok(block);
        }
    }
    bail!("could not locate shop setup subroutine");
}

fn is_shop_setup(script: &Script, block: BlockId, visited: &mut HashSet<BlockId>) -> bool {
    if !visited.insert(block) {
        return false;
    }
    let code = script.block(block).code().unwrap();
    for command in &code.commands {
        if let Command::Set(arg) = command {
            if let SetExpr::Variable(e) = &arg.target {
                let index = e.value().unwrap_or(0) as usize;
                if (SHOP_ITEM_FIRST..SHOP_ITEM_LAST).contains(&index) {
                    return true;
                }
            }
        }
    }
    if let Some(Ip::Block(next_block)) = code.next_block {
        if is_shop_setup(script, next_block, visited) {
            return true;
        }
        if let Some(Ip::Block(else_block)) = code.else_block {
            return is_shop_setup(script, else_block, visited);
        }
    }
    false
}

/// A requirement for an item to be visible in the shop.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum Requirement {
    /// The player must have an item.
    HaveItem(ItemId),
    /// The player must not have an item.
    MissingItem(ItemId),
    /// The player must have an ATC.
    HaveAtc(AtcId),
    /// The player must not have an ATC.
    MissingAtc(AtcId),
    /// A flag must be set.
    HaveFlag(i32),
    /// A flag must not be set.
    MissingFlag(i32),
}

impl Requirement {
    /// Returns the opposite of the requirement.
    pub fn negate(&self) -> Self {
        match *self {
            Self::HaveItem(i) => Self::MissingItem(i),
            Self::MissingItem(i) => Self::HaveItem(i),

            Self::HaveAtc(i) => Self::MissingAtc(i),
            Self::MissingAtc(i) => Self::HaveAtc(i),

            Self::HaveFlag(i) => Self::MissingFlag(i),
            Self::MissingFlag(i) => Self::HaveFlag(i),
        }
    }
}

/// An item slot in the shop.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Slot {
    /// The item in the slot. If `None`, the slot is unused.
    pub item: Option<ItemId>,
    /// The maximum amount of the item that the player can have.
    pub limit: i16,
    /// The requirements for the slot to be visible. If empty, the slot is always visible.
    pub requirements: HashSet<Requirement>,
}

impl Slot {
    /// Constructs an empty `Slot`.
    pub fn new() -> Self {
        Self::default()
    }
}

/// An in-game shop configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Shop {
    pub slots: Box<[Slot]>,
}

impl Shop {
    /// Constructs an empty `Shop`.
    pub fn new() -> Self {
        Self { slots: vec![Slot::default(); NUM_SLOTS].into_boxed_slice() }
    }

    /// Parses a `Shop` from a script with a shop setup subroutine. Usually this should be the
    /// script for stage05 (Chibi-House).
    pub fn parse(script: &Script) -> Result<Self> {
        debug!("Searching for shop setup routine");
        let shop_block = find_shop_setup(script)?;
        debug!("Found shop setup routine starting at block {:?}", shop_block);

        let slots = ShopParser::new(script).parse(shop_block);
        Ok(Self { slots: slots.into_boxed_slice() })
    }
}

impl Default for Shop {
    fn default() -> Self {
        Self::new()
    }
}

pub fn shop_test(opt: ShopTestOpt) -> Result<()> {
    let mut iso = open_iso_optional(opt.container.iso.as_ref())?;
    let mut qp = open_qp_required(iso.as_mut(), &opt.container)?;

    info!("Reading script globals");
    let libs = {
        let mut globals = read_globals_qp(&mut qp)?;
        globals.read_libs()?
    };

    info!("Reading stage file");
    let stage = read_stage_qp(&mut qp, CHIBI_HOUSE.name, &libs)?;

    info!("Analyzing shop data");
    let shop = Shop::parse(&stage.script)?;
    for (i, slot) in shop.slots.iter().enumerate() {
        debug!("{}: {:?}", i, slot);
    }
    Ok(())
}
