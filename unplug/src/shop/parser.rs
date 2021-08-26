use super::{
    Requirement, Slot, NUM_SLOTS, SHOP_COUNT_FIRST, SHOP_COUNT_LAST, SHOP_ITEM_FIRST,
    SHOP_ITEM_LAST,
};

use crate::data::{Atc, Item};
use crate::event::command::SetArgs;
use crate::event::expr::BinaryOp;
use crate::event::{BlockId, Command, Expr, Script, SetExpr};
use log::warn;
use std::collections::HashSet;
use std::convert::TryFrom;
use std::iter;

/// Parses `condition` into a set of `Requirement`s.
fn parse_requirements(condition: &Expr) -> HashSet<Requirement> {
    let mut requirements = HashSet::new();
    do_parse_requirements(condition, &mut requirements, false);

    // Filter out requirements which contradict each other
    let negated: HashSet<_> = requirements.iter().map(|r| r.negate()).collect();
    requirements.retain(|r| !negated.contains(r));
    requirements
}

fn do_parse_requirements(condition: &Expr, requirements: &mut HashSet<Requirement>, negate: bool) {
    match condition {
        // Join on both AND and OR. This isn't technically correct because we only support AND in our
        // representation but it's good enough for handling the default game code.
        Expr::BitAnd(op) | Expr::BitOr(op) => {
            do_parse_requirements(&op.lhs, requirements, negate);
            do_parse_requirements(&op.rhs, requirements, negate);
        }

        // To handle not(), we just toggle the current negation state
        Expr::Not(e) => do_parse_requirements(e, requirements, !negate),

        _ => {
            if let Some(req) = parse_requirement(condition) {
                requirements.insert(if negate { req.negate() } else { req });
            }
        }
    }
}

fn parse_requirement(condition: &Expr) -> Option<Requirement> {
    match condition {
        // `!= 0` and `> 0` check if a requirement is true
        Expr::NotEqual(op) | Expr::Greater(op) => {
            if let (Some(lhs), Some(0)) = (parse_requirement(&op.lhs), op.rhs.value()) {
                return Some(lhs);
            }
        }

        // `== 0` and `<= 0` check if a requirement is false
        Expr::Equal(op) | Expr::LessEqual(op) => {
            if let (Some(lhs), Some(0)) = (parse_requirement(&op.lhs), op.rhs.value()) {
                return Some(lhs.negate());
            }
        }

        // Assume that item/atc/flag references are checking for the corresponding thing
        Expr::Item(e) => {
            if let Ok(item) = Item::try_from(&**e) {
                return Some(Requirement::HaveItem(item));
            }
        }
        Expr::Atc(e) => {
            if let Ok(atc) = Atc::try_from(&**e) {
                return Some(Requirement::HaveAtc(atc));
            }
        }
        Expr::Flag(e) => {
            if let Some(index) = e.value() {
                return Some(Requirement::HaveFlag(index));
            }
        }

        _ => (),
    }
    None
}

/// Joins `right` to `left` using `Expr::BitAnd` if necessary.
fn join(left: Option<Expr>, right: Expr) -> Expr {
    match left {
        Some(left) => Expr::BitAnd(BinaryOp::new(left, right).into()),
        None => right,
    }
}

/// A compact table which holds the state of the shop variables.
#[derive(Debug, Default, Clone)]
struct VarTable {
    /// If a bit is set, the corresponding item value is present.
    item_mask: u32,
    /// If a bit is set, the corresponding count value is present.
    count_mask: u32,
    /// The item variables.
    items: [i16; NUM_SLOTS],
    /// The count variables.
    counts: [i16; NUM_SLOTS],
}

impl VarTable {
    /// Creates an empty `VarTable`.
    fn new() -> Self {
        Self::default()
    }

    /// Gets the item at `slot` if it is present.
    fn item(&self, slot: usize) -> Option<i16> {
        Self::get(&self.items, self.item_mask, slot)
    }

    /// Puts `item` into item slot `slot`.
    fn put_item(&mut self, slot: usize, item: i16) {
        Self::put(&mut self.items, &mut self.item_mask, slot, item);
    }

    /// Gets the count at `slot` if it is present.
    fn count(&self, slot: usize) -> Option<i16> {
        Self::get(&self.counts, self.count_mask, slot)
    }

    /// Puts `count` into count slot `slot`.
    fn put_count(&mut self, slot: usize, count: i16) {
        Self::put(&mut self.counts, &mut self.count_mask, slot, count);
    }

    /// Returns true if no slot contains any values.
    fn is_empty(&self) -> bool {
        self.item_mask == 0 && self.count_mask == 0
    }

    /// Creates an iterator over the indexes of slots which have values in them.
    fn slots(&self) -> impl Iterator<Item = usize> {
        // This is actually pretty simple, we basically just need to merge the masks and iterate
        // over the indexes of the set bits.
        let mut mask = self.item_mask | self.count_mask;
        let mut slot = mask.trailing_zeros(); // Start on the first present slot
        mask = mask.wrapping_shr(slot); // Don't panic on overflow
        iter::from_fn(move || {
            while mask != 0 {
                let present = mask & 1;
                mask >>= 1;
                slot += 1;
                if present != 0 {
                    return Some(slot as usize - 1);
                }
            }
            None
        })
    }

    fn get(vars: &[i16], mask: u32, slot: usize) -> Option<i16> {
        match mask & (1 << slot) {
            0 => None,
            _ => Some(vars[slot]),
        }
    }

    fn put(vars: &mut [i16], mask: &mut u32, slot: usize, value: i16) {
        vars[slot] = value;
        *mask |= 1 << slot;
    }
}

/// Parses shop data from shop code.
pub(super) struct ShopParser<'s> {
    /// The script to operate on
    script: &'s Script,
    /// Current configuration of each slot
    slots: Vec<Slot>,
    /// Set of visited blocks
    visited: HashSet<BlockId>,
    /// Current item limit
    current_limit: i16,
}

impl<'s> ShopParser<'s> {
    /// Creates a new `ShopParser` over `script`.
    pub(super) fn new(script: &'s Script) -> Self {
        Self {
            script,
            slots: vec![Slot::default(); NUM_SLOTS],
            visited: HashSet::new(),
            current_limit: 0,
        }
    }

    /// Parses script code starting from `block` and returns the estimated slot configuration.
    pub(super) fn parse(mut self, block: BlockId) -> Vec<Slot> {
        self.parse_block(block, None);
        self.slots
    }

    /// Parses `block`, building requirements from `condition`.
    fn parse_block(&mut self, block: BlockId, condition: Option<Expr>) {
        if !self.visited.insert(block) {
            return;
        }

        // The idea here is that `condition` holds an expression that must be true for the code in
        // this block to execute. We run through the block and evaluate the set() commands in it to
        // build up the state of the shop vars under this condition. Then we propagate that state to
        // the slots and add the requirements for this condition to be true.
        let code = self.script.block(block).code().unwrap();
        let next_block = code.next_block.map(|ip| ip.block().unwrap());
        let else_block = code.else_block.map(|ip| ip.block().unwrap());
        let (mut next_condition, mut else_condition) = (None, None);
        let mut vars = VarTable::new();
        for command in &code.commands {
            match command {
                // if() statements give us the conditions for the next and else blocks. If a block
                // does not end in an if(), the current condition is not propagated to the next
                // block. For our purposes, this is a good-enough heuristic to know when a
                // conditional ends.
                Command::If(arg) => {
                    next_condition = Some(join(condition.clone(), arg.condition.clone()));
                    else_condition = Some(join(condition.clone(), arg.condition.clone().negate()));
                }

                // set() commands assign to the shop variables
                Command::Set(arg) => self.evaluate_set(&mut vars, arg),

                _ => (),
            }
        }

        // Propagate the current state to any slots that were updated
        if !vars.is_empty() {
            let requirements = condition.as_ref().map_or(HashSet::new(), parse_requirements);
            self.update_slots(&requirements, &vars);
        }

        if let Some(next_block) = next_block {
            self.parse_block(next_block, next_condition);
            if let Some(else_block) = else_block {
                self.parse_block(else_block, else_condition);
            }
        }
    }

    /// Evaluates a set() command and updates `vars` with any new data.
    fn evaluate_set(&mut self, vars: &mut VarTable, set: &SetArgs) {
        // Only interested in variables
        let index = match set.target {
            SetExpr::Variable(Expr::Imm32(i)) => i as usize,
            SetExpr::Variable(Expr::Imm16(i)) => i as usize,
            _ => {
                warn!("unsupported set() target: {:?}", set.target);
                return;
            }
        };

        // The game handles limits using code like this:
        //
        //   vars[0] = LIMIT - item[ITEM]
        //   if (vars[0] < 0) vars[0] = 0
        //   vars[SLOT_ITEM] = ITEM
        //   vars[SLOT_COUNT] = vars[0]
        //
        // We mainly need to focus on the first subtraction statement to get the limit. Later, if
        // vars[0] is assigned to something, we use the last limit that we found (i.e. we
        // effectively substitute 0 for the current item count).
        if index == 0 {
            if let Expr::Subtract(op) = &set.value {
                if let BinaryOp { rhs: Expr::Item(item), lhs } = &**op {
                    if let (Some(_), Some(limit)) = (item.value(), lhs.value()) {
                        self.current_limit = limit as i16;
                        return;
                    }
                }
            } else if let Some(0) = set.value.value() {
                return; // Ignore the assignment to clamp to 0
            }
            warn!("unsupported temp var assignment: {:?}", set);
            return;
        }

        let value = match &set.value {
            Expr::Imm32(i) => *i as i16,
            Expr::Imm16(i) => *i,
            Expr::Variable(i) if i.value() == Some(0) => self.current_limit, // See above.
            _ => {
                warn!("unsupported set() value: {:?}", set.value);
                return;
            }
        };
        match index {
            0 => (),
            SHOP_ITEM_FIRST..=SHOP_ITEM_LAST => vars.put_item(index - SHOP_ITEM_FIRST, value),
            SHOP_COUNT_FIRST..=SHOP_COUNT_LAST => vars.put_count(index - SHOP_COUNT_FIRST, value),
            _ => warn!("unsupported global var: {}", index),
        }
    }

    /// Propagates `requirements` and `vars` to shop slots that were modified.
    fn update_slots(&mut self, requirements: &HashSet<Requirement>, vars: &VarTable) {
        for index in vars.slots() {
            // We need to have both an item ID and count for the slot
            let id = vars.item(index).unwrap_or(-1);
            let count = match vars.count(index) {
                Some(count) => count,
                None => continue,
            };

            // Negative IDs are hidden slots
            let slot = &mut self.slots[index];
            if id < 0 {
                // Since this is hiding the slot, we have to invert the condition
                slot.requirements.extend(requirements.iter().map(|r| r.negate()));
                continue;
            }

            // Update the slot item
            if let Ok(item) = Item::try_from(id) {
                if let Some(existing) = slot.item {
                    if existing != item {
                        warn!(
                            "Item ID conflict in slot {}: have {:?} but found {:?}",
                            index, existing, item
                        );
                    }
                } else {
                    slot.item = Some(item);
                }
            } else {
                warn!("Unrecognized item ID in slot {}: {}", index, id);
            }
            if count > 0 {
                slot.limit = count;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::command::IfArgs;
    use crate::event::{Block, CodeBlock, Ip};
    use crate::expr;

    /// Convenience macro for initializing HashSets
    macro_rules! set {
        [$($value:expr),* $(,)*] => {
            vec![$($value),*].into_iter().collect::<::std::collections::HashSet<_>>()
        };
    }

    fn set(var: usize, value: i16) -> Command {
        SetArgs::new(SetExpr::Variable(Expr::Imm16(var as i16)), Expr::Imm16(value)).into()
    }

    fn set_expr(var: usize, value: Expr) -> Command {
        SetArgs::new(SetExpr::Variable(Expr::Imm32(var as i32)), value).into()
    }

    fn if_(condition: Expr, else_block: u32) -> Command {
        Command::If(IfArgs { condition, else_target: Ip::Block(BlockId::new(else_block)) }.into())
    }

    #[test]
    fn test_parse_requirements_item() {
        assert_eq!(
            parse_requirements(&expr![item[Item::HotRod] != 0]),
            set![Requirement::HaveItem(Item::HotRod)]
        );
        assert_eq!(
            parse_requirements(&expr![item[Item::HotRod] > 0]),
            set![Requirement::HaveItem(Item::HotRod)]
        );
        assert_eq!(
            parse_requirements(&expr![item[Item::HotRod] == 0]),
            set![Requirement::MissingItem(Item::HotRod)]
        );
        assert_eq!(
            parse_requirements(&expr![item[Item::HotRod] <= 0]),
            set![Requirement::MissingItem(Item::HotRod)]
        );
    }

    #[test]
    fn test_parse_requirements_atc() {
        assert_eq!(
            parse_requirements(&expr![atc[Atc::Toothbrush] != 0]),
            set![Requirement::HaveAtc(Atc::Toothbrush)]
        );
        assert_eq!(
            parse_requirements(&expr![atc[Atc::Toothbrush] > 0]),
            set![Requirement::HaveAtc(Atc::Toothbrush)]
        );
        assert_eq!(
            parse_requirements(&expr![atc[Atc::Toothbrush] == 0]),
            set![Requirement::MissingAtc(Atc::Toothbrush)]
        );
        assert_eq!(
            parse_requirements(&expr![atc[Atc::Toothbrush] <= 0]),
            set![Requirement::MissingAtc(Atc::Toothbrush)]
        );
    }

    #[test]
    fn test_parse_requirements_flag() {
        assert_eq!(parse_requirements(&expr![flag[123]]), set![Requirement::HaveFlag(123)]);
        assert_eq!(parse_requirements(&expr![flag[123] != 0]), set![Requirement::HaveFlag(123)]);
        assert_eq!(parse_requirements(&expr![flag[123] > 0]), set![Requirement::HaveFlag(123)]);
        assert_eq!(parse_requirements(&expr![!(flag[123])]), set![Requirement::MissingFlag(123)]);
        assert_eq!(parse_requirements(&expr![flag[123] == 0]), set![Requirement::MissingFlag(123)]);
        assert_eq!(parse_requirements(&expr![flag[123] <= 0]), set![Requirement::MissingFlag(123)]);
    }

    #[test]
    fn test_parse_requirements_not() {
        assert_eq!(
            parse_requirements(&expr![!(item[Item::HotRod] > 0)]),
            set![Requirement::MissingItem(Item::HotRod)]
        );
        assert_eq!(
            parse_requirements(&expr![!(!(item[Item::HotRod] > 0))]),
            set![Requirement::HaveItem(Item::HotRod)]
        );
        assert_eq!(
            parse_requirements(&expr![!(!(!(item[Item::HotRod] > 0)))]),
            set![Requirement::MissingItem(Item::HotRod)]
        );
    }

    #[test]
    fn test_parse_requirements_multiple() {
        assert_eq!(
            parse_requirements(&expr![
                (item[Item::HotRod] > 0) && (atc[Atc::Toothbrush] > 0) && flag[123]
            ]),
            set![
                Requirement::HaveItem(Item::HotRod),
                Requirement::HaveAtc(Atc::Toothbrush),
                Requirement::HaveFlag(123)
            ]
        );
    }

    #[test]
    fn test_var_table() {
        let mut vars = VarTable::new();
        assert!(vars.is_empty());

        assert!(vars.item(3).is_none());
        vars.put_item(3, 1);
        assert_eq!(vars.item(3), Some(1));
        assert!(!vars.is_empty());

        assert!(vars.count(5).is_none());
        vars.put_count(5, 2);
        assert_eq!(vars.count(5), Some(2));

        assert!(vars.item(7).is_none());
        assert!(vars.count(7).is_none());
        vars.put_item(7, 3);
        vars.put_count(7, 4);
        assert_eq!(vars.item(7), Some(3));
        assert_eq!(vars.count(7), Some(4));

        let slots: Vec<_> = vars.slots().collect();
        assert_eq!(slots, &[3, 5, 7]);
    }

    #[test]
    #[allow(clippy::identity_op)]
    fn test_parse_shop() {
        let blocks = vec![
            // 0
            Block::Code(CodeBlock {
                commands: vec![
                    // item = HotRot
                    set(SHOP_ITEM_FIRST + 0, Item::HotRod.into()),
                    // count = 1
                    set(SHOP_COUNT_FIRST + 0, 1),
                    // item = -1
                    set(SHOP_ITEM_FIRST + 1, -1),
                    // count = 0
                    set(SHOP_COUNT_FIRST + 1, 0),
                ],
                next_block: Some(Ip::Block(BlockId::new(1))),
                else_block: None,
            }),
            // 1
            Block::Code(CodeBlock {
                commands: vec![
                    // var[0] = 10 - item[BlueFlowerSeed]
                    set_expr(0, expr![10 - item[Item::BlueFlowerSeed]]),
                    // if (var[0] < 0)
                    if_(expr![var[0] < 0], 3),
                ],
                next_block: Some(Ip::Block(BlockId::new(2))),
                else_block: Some(Ip::Block(BlockId::new(3))),
            }),
            // 2
            Block::Code(CodeBlock {
                commands: vec![
                    // vars[0] = 0
                    set(0, 0),
                ],
                next_block: Some(Ip::Block(BlockId::new(3))),
                else_block: None,
            }),
            // 3
            Block::Code(CodeBlock {
                commands: vec![
                    // item = BlueFlowerSeed
                    set(SHOP_ITEM_FIRST + 2, Item::BlueFlowerSeed.into()),
                    // count = vars[0]
                    set_expr(SHOP_COUNT_FIRST + 2, expr![var[0]]),
                ],
                next_block: Some(Ip::Block(BlockId::new(4))),
                else_block: None,
            }),
            // 4
            Block::Code(CodeBlock {
                commands: vec![
                    // if atc[Toothbrush] == 0 && flag[123]
                    if_(expr![(atc[Atc::Toothbrush] == 0) && flag[123]], 6),
                ],
                next_block: Some(Ip::Block(BlockId::new(5))),
                else_block: Some(Ip::Block(BlockId::new(6))),
            }),
            // 5
            Block::Code(CodeBlock {
                commands: vec![
                    // item = Toothbrush
                    set(SHOP_ITEM_FIRST + 3, Item::Toothbrush.into()),
                    // count = 1
                    set(SHOP_COUNT_FIRST + 3, 1),
                    Command::EndIf(Ip::Block(BlockId::new(9))),
                ],
                next_block: Some(Ip::Block(BlockId::new(9))),
                else_block: None,
            }),
            // 6
            Block::Code(CodeBlock {
                commands: vec![
                    // else if atc[Toothbrush] > 0
                    if_(expr![atc[Atc::Toothbrush] > 0], 8),
                ],
                next_block: Some(Ip::Block(BlockId::new(7))),
                else_block: Some(Ip::Block(BlockId::new(8))),
            }),
            // 7
            Block::Code(CodeBlock {
                commands: vec![
                    // item = Toothbrush
                    set(SHOP_ITEM_FIRST + 3, Item::Toothbrush.into()),
                    // count = 0
                    set(SHOP_COUNT_FIRST + 3, 0),
                    Command::EndIf(Ip::Block(BlockId::new(9))),
                ],
                next_block: Some(Ip::Block(BlockId::new(9))),
                else_block: None,
            }),
            // 8
            Block::Code(CodeBlock {
                commands: vec![
                    // else
                    // item = -1
                    set(SHOP_ITEM_FIRST + 3, -1),
                    // count = 0
                    set(SHOP_COUNT_FIRST + 3, 0),
                ],
                next_block: Some(Ip::Block(BlockId::new(9))),
                else_block: None,
            }),
            // 9
            Block::Code(CodeBlock {
                commands: vec![Command::Return],
                next_block: None,
                else_block: None,
            }),
        ];

        let script = Script::with_blocks(blocks);
        let slots = ShopParser::new(&script).parse(BlockId::new(0));
        assert_eq!(slots[0], Slot { item: Some(Item::HotRod), limit: 1, requirements: set![] });
        assert_eq!(slots[1], Slot { item: None, limit: 0, requirements: set![] });
        assert_eq!(
            slots[2],
            Slot { item: Some(Item::BlueFlowerSeed), limit: 10, requirements: set![] }
        );
        assert_eq!(
            slots[3],
            Slot {
                item: Some(Item::Toothbrush),
                limit: 1,
                requirements: set![Requirement::HaveFlag(123)],
            }
        );
        for slot in slots.iter().skip(4) {
            assert_eq!(*slot, Slot { item: None, limit: 0, requirements: set![] });
        }
    }
}
