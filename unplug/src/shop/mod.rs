mod compiler;
mod parser;

use compiler::ShopCompiler;
use parser::ShopParser;

use crate::data::{Atc, Item};
use crate::event::analysis::Label;
use crate::event::{BlockId, Command, Ip, Script, SetExpr};
use std::collections::HashSet;
use thiserror::Error;
use tracing::debug;

/// The result type for shop operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for shop operations.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("could not locate the shop setup code")]
    ShopCodeNotFound,
}

/// The maximum number of slots that can fit in a shop.
pub const NUM_SLOTS: usize = 20;

/// The first global variable for shop items.
const SHOP_ITEM_FIRST: usize = 600;
/// The last global variable for shop items.
const SHOP_ITEM_LAST: usize = SHOP_ITEM_FIRST + NUM_SLOTS - 1;
/// The first global variable for shop item limits.
const SHOP_COUNT_FIRST: usize = SHOP_ITEM_LAST + 1;
/// The last global variable for shop item limits.
const SHOP_COUNT_LAST: usize = SHOP_COUNT_FIRST + NUM_SLOTS - 1;

fn find_shop_setup(script: &Script) -> Result<BlockId> {
    // There should only be one subroutine in the script which builds the shop, and our goal is to
    // find it. We can use the analyzer output to greatly limit our search space - the routine will
    // be marked as killing the shop vars. Every routine which calls the shop routine will also
    // technically kill the shop vars, so if we sort by the number of killed labels then the first
    // one is likely to be our goal.
    debug!("Searching for shop setup routine");
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
            debug!("Found shop setup routine starting at block {:?}", block);
            return Ok(block);
        }
    }
    Err(Error::ShopCodeNotFound)
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
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum Requirement {
    /// The player must have an item.
    HaveItem(Item),
    /// The player must not have an item.
    MissingItem(Item),
    /// The player must have an ATC.
    HaveAtc(Atc),
    /// The player must not have an ATC.
    MissingAtc(Atc),
    /// A flag must be set.
    HaveFlag(i32),
    /// A flag must not be set.
    MissingFlag(i32),
}

impl Requirement {
    /// Returns the opposite of the requirement.
    #[must_use]
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
    pub item: Option<Item>,
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
pub struct Shop {
    slots: Vec<Slot>,
}

impl Shop {
    /// Creates an empty `Shop`.
    pub fn new() -> Self {
        Self { slots: vec![Slot::default(); NUM_SLOTS] }
    }

    /// Creates a `Shop` initialized with `slots`. Only up to `NUM_SLOTS` slots are used.
    pub fn with_slots(slots: impl IntoIterator<Item = Slot>) -> Self {
        let mut slots: Vec<Slot> = slots.into_iter().take(NUM_SLOTS).collect();
        slots.resize(NUM_SLOTS, Slot::default());
        slots.shrink_to_fit();
        Self { slots }
    }

    /// Parses a `Shop` from a script with a shop setup subroutine. Usually this should be the
    /// script for stage05 (Chibi-House).
    pub fn parse(script: &Script) -> Result<Self> {
        let shop_block = find_shop_setup(script)?;
        let mut slots = ShopParser::new(script).parse(shop_block);
        slots.shrink_to_fit();
        Ok(Self { slots })
    }

    /// Recompiles the shop and replaces the shop setup subroutine in `script`. Usually this should
    /// be the script for stage05 (Chibi-House).
    pub fn compile(&self, script: &mut Script) -> Result<()> {
        let shop_block = find_shop_setup(script)?;
        ShopCompiler::new(script).compile(&self.slots).replace(shop_block);
        Ok(())
    }

    /// Retrieves a slice over the slots in the shop.
    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }

    /// Retrieves a mutable slice over the slots in the shop.
    pub fn slots_mut(&mut self) -> &mut [Slot] {
        &mut self.slots
    }
}

impl Default for Shop {
    fn default() -> Self {
        Self::new()
    }
}
