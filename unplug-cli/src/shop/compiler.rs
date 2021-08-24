use super::{Requirement, Slot, SHOP_COUNT_FIRST, SHOP_ITEM_FIRST};

use log::Level::Trace;
use log::{log_enabled, trace, warn};
use std::convert::TryFrom;
use std::iter;
use unplug::data::atc::AtcId;
use unplug::data::item::ItemId;
use unplug::event::command::{IfArgs, SetArgs};
use unplug::event::expr::BinaryOp;
use unplug::event::{Block, BlockId, CodeBlock, Command, Expr, Ip, Script, SetExpr};

const TIMER_5_RATE: i16 = 200;
const TIMER_10_RATE: i16 = 100;
const TIMER_15_RATE: i16 = 67;

/// Compiles a collection of requirements into an expression.
fn compile_requirements(requirements: impl IntoIterator<Item = Requirement>) -> Option<Expr> {
    let mut result = None;
    for requirement in requirements {
        let condition = compile_requirement(&requirement);
        result = Some(match result {
            Some(other) => Expr::BitAnd(BinaryOp::new(other, condition).into()),
            None => condition,
        });
    }
    result
}

/// Compiles a single requirement into an expression.
fn compile_requirement(requirement: &Requirement) -> Expr {
    match *requirement {
        Requirement::HaveItem(item) => compile_item_requirement(item),
        Requirement::MissingItem(item) => compile_item_requirement(item).negate(),
        Requirement::HaveAtc(atc) => compile_atc_requirement(atc),
        Requirement::MissingAtc(atc) => compile_atc_requirement(atc).negate(),
        Requirement::HaveFlag(index) => compile_flag_requirement(index),
        Requirement::MissingFlag(index) => compile_flag_requirement(index).negate(),
    }
}

/// Compiles an expression for checking whether the player has `item`.
fn compile_item_requirement(item: ItemId) -> Expr {
    let lhs = Expr::Item(Expr::Imm16(item.into()).into());
    let rhs = Expr::Imm16(0);
    Expr::NotEqual(BinaryOp::new(lhs, rhs).into())
}

/// Compiles an expression for checking whether the player has `atc`.
fn compile_atc_requirement(atc: AtcId) -> Expr {
    let lhs = Expr::Atc(Expr::Imm16(atc.into()).into());
    let rhs = Expr::Imm16(0);
    Expr::NotEqual(BinaryOp::new(lhs, rhs).into())
}

/// Compiles an expression for checking whether `flag` is set.
fn compile_flag_requirement(flag: i32) -> Expr {
    Expr::Flag(Expr::Imm16(flag as i16).into())
}

/// Returns whether `item` is a timer.
fn is_timer(item: Option<ItemId>) -> bool {
    matches!(item, Some(ItemId::Timer5 | ItemId::Timer10 | ItemId::Timer15))
}

/// Compilation context for a single slot.
struct ItemContext {
    /// The index of the slot to emit code for.
    index: usize,
    /// The item in the slot, if any.
    item: Option<ItemId>,
    /// The slot's corresponding ATC, if any.
    atc: Option<AtcId>,
    /// The effective limit for the slot.
    limit: i16,
    /// The block to jump to after the slot has been initialized.
    end_block: BlockId,
    /// The block to jump to if the slot needs to be hidden, if any.
    hide_block: Option<BlockId>,
    /// The slot's requirements stored in a deterministic order.
    requirements: Vec<Requirement>,
}

impl ItemContext {
    fn new(slot: &Slot, index: usize, end_block: BlockId) -> Self {
        let item = slot.item;
        let atc = item.and_then(|i| AtcId::try_from(i).ok());

        // Coerce the limit to comply with the game restrictions if necessary
        let limit = if item.is_none() {
            0
        } else if atc.is_some() {
            if slot.limit != 1 {
                warn!("Slot {}: Attachment items can only have a limit of 1", index);
            }
            1
        } else if is_timer(item) {
            if slot.limit != 1 {
                warn!("Slot {}: Timers can only have a limit of 1", index);
            }
            1
        } else if slot.limit > 10 {
            warn!("Slot {}: Item limit cannot exceed 10", index);
            10
        } else if slot.limit < 1 {
            warn!("Slot {}: Item limit must be at least 1", index);
            1
        } else {
            slot.limit
        };

        // Sort the requirements into a deterministic order so we always generate the same code
        let mut requirements: Vec<_> = slot.requirements.iter().copied().collect();
        requirements.sort_unstable();

        Self { index, item, atc, limit, end_block, hide_block: None, requirements }
    }
}

/// Builds up a code block.
#[derive(Default)]
struct BlockBuilder {
    code: CodeBlock,
}

impl BlockBuilder {
    /// Creates a new `BlockBuilder`.
    fn new() -> Self {
        Self::default()
    }

    /// Adds `command` to the end of the block.
    fn emit(&mut self, command: Command) {
        self.code.commands.push(command)
    }

    /// Emits a `Set()` command which sets `var` to `value`.
    fn emit_set(&mut self, var: usize, value: Expr) {
        self.emit(Command::from(SetArgs::new(SetExpr::Variable(Expr::Imm16(var as i16)), value)));
    }

    /// Emits `Set()` commands to fill the shop slot at `index` with `item` and `count`.
    fn emit_set_slot(&mut self, index: usize, item: ItemId, count: Expr) {
        self.emit_set(SHOP_ITEM_FIRST + index, Expr::Imm16(item.into()));
        self.emit_set(SHOP_COUNT_FIRST + index, count);
    }

    /// Emits `Set()` commands to hide the shop slot at `index`.
    fn emit_set_slot_empty(&mut self, index: usize) {
        self.emit_set(SHOP_ITEM_FIRST + index, Expr::Imm16(-1));
        self.emit_set(SHOP_COUNT_FIRST + index, Expr::Imm16(0));
    }

    /// Emits an `If()` command which checks `condition` and jumps to `else_block` if it's false.
    fn emit_if_else(&mut self, condition: Expr, else_block: BlockId) {
        self.emit(Command::If(IfArgs { condition, else_target: Ip::Block(else_block) }.into()));
    }

    /// Emits an `EndIf()` command which jumps to `target`.
    fn emit_endif(&mut self, target: BlockId) {
        self.emit(Command::EndIf(target.into()))
    }

    /// Finishes building the block and returns the inner `CodeBlock`.
    fn finish(self) -> CodeBlock {
        self.code
    }
}

/// A compiled shop configuration which can be inserted back into the original script.
pub(super) struct CompiledShop<'s> {
    script: &'s mut Script,
    root: BlockId,
}

#[allow(dead_code)]
impl CompiledShop<'_> {
    /// Appends the compiled code to the script and returns the root block ID.
    pub(super) fn append(self) -> BlockId {
        self.root
    }

    /// Replaces `old_root` in the script with the compiled code.
    pub(super) fn replace(self, old_root: BlockId) {
        self.script.redirect_block(old_root, self.root);
    }

    /// Debug logging for the compiled code
    fn log(&self) {
        trace!("Compiled shop code:");
        let order = self.script.reverse_postorder(self.root);
        for id in order {
            let block = self.script.block(id);
            for command in block.commands().unwrap() {
                trace!("{:<4} {:?}", id.index(), command);
            }
        }
    }
}

/// Compiles a shop configuration into script code.
pub(super) struct ShopCompiler<'s> {
    /// The script to operate on.
    script: &'s mut Script,
    /// The last full block that was emitted, if any.
    last_block: Option<BlockId>,
}

impl<'s> ShopCompiler<'s> {
    /// Creates a new `ShopCompiler` that operates on `script`.
    pub(super) fn new(script: &'s mut Script) -> Self {
        Self { script, last_block: None }
    }

    /// Compiles `slots` into shop code and returns a `CompiledShop` which can be inserted back into
    /// the script.
    #[must_use]
    pub(super) fn compile(mut self, slots: &[Slot]) -> CompiledShop<'s> {
        // We compile things in backwards order so that we can resolve all of the block edges in one
        // pass. Start with the return statement and then compile all the slots in reverse.
        let _return_block = self.compile_block(|b| {
            b.emit(Command::Return);
        });

        for (index, slot) in slots.iter().enumerate().rev() {
            self.compile_slot(slot, index);
        }

        let root = self.last_block.unwrap();
        let compiled = CompiledShop { script: self.script, root };
        if log_enabled!(Trace) {
            compiled.log();
        }
        compiled
    }

    /// Compiles code for filling in `slot` at `index`.
    fn compile_slot(&mut self, slot: &Slot, index: usize) {
        // As in compile(), we build up the blocks backwards to make the edges easy to connect.

        let end_block = self.last_block.expect("no end block");
        let mut ctx = ItemContext::new(slot, index, end_block);

        // If the slot has no item or it has requirements, there needs to be a block to hide it
        if ctx.item.is_none() || !ctx.requirements.is_empty() {
            self.compile_slot_hidden(&mut ctx);
        }

        // If the slot is empty, we're already done
        let item = match ctx.item {
            Some(item) => item,
            None => return,
        };

        // There are two ways to handle limits. For items with a limit of 1, we just need to set the
        // item count to 1 or 0 depending on whether the enable condition passes. For items with
        // more than one slot, we have to calculate how many more items the player can fit.
        //
        // TODO: Timer conditions are currently broken, this needs to be addressed
        if ctx.limit == 1 {
            self.compile_slot_unique(&ctx, item);
        } else {
            self.compile_slot_limit(&ctx, item);
        }
    }

    /// Compiles a slot which is hidden.
    fn compile_slot_hidden(&mut self, ctx: &mut ItemContext) {
        ctx.hide_block = Some(self.compile_block(|b| {
            b.emit_set_slot_empty(ctx.index);
        }));
    }

    /// Compiles a slot which holds a "unique" item that can only be in the inventory once.
    fn compile_slot_unique(&mut self, ctx: &ItemContext, item: ItemId) {
        assert_eq!(ctx.limit, 1);

        // Psuedocode:
        //   if (!<acquired> && <requirements>) {
        //     vars[SLOT_ITEM] = <item>
        //     vars[SLOT_COUNT] = 1
        //   } else if (<acquired>) {
        //     vars[SLOT_ITEM] = <item>
        //     vars[SLOT_COUNT] = 0
        //   } else {
        //     vars[SLOT_ITEM] = -1
        //     vars[SLOT_COUNT] = 0
        //   }
        //
        // As in compile(), we build up the blocks backwards to make the edges easy to connect.

        // If the item has a corresponding ATC, we need to check that as the source of truth
        let acquired_rec = ctx.atc.map(Requirement::HaveAtc).unwrap_or(Requirement::HaveItem(item));
        let mut acquired = compile_requirements(iter::once(acquired_rec)).unwrap();

        // Timers can also be considered acquired if the current time rate (`time[2]`) matches the
        // timer's rate.
        if is_timer(ctx.item) {
            let rate = match ctx.item.unwrap() {
                ItemId::Timer5 => TIMER_5_RATE,
                ItemId::Timer10 => TIMER_10_RATE,
                ItemId::Timer15 => TIMER_15_RATE,
                other => panic!("missing rate for {:?}", other),
            };
            let rate_expr = Expr::Time(Expr::Imm16(2).into());
            let rate_compare = Expr::Equal(BinaryOp::new(rate_expr, Expr::Imm16(rate)).into());
            acquired = Expr::BitOr(BinaryOp::new(acquired, rate_compare).into())
        }

        // vars[SLOT_ITEM] = ITEM
        // vars[SLOT_COUNT] = 0
        let disable_block = self.compile_block(|b| {
            b.emit_set_slot(ctx.index, item, Expr::Imm16(0));
            if ctx.hide_block.is_some() {
                // Jump over the hide block
                b.emit_endif(ctx.end_block);
            }
        });

        let else_block = if let Some(hide_block) = ctx.hide_block {
            // else if (<acquired>)
            self.compile_block(|b| {
                b.emit_if_else(acquired.clone(), hide_block);
            })
        } else {
            // else
            disable_block
        };

        // vars[SLOT_ITEM] = <item>
        // vars[SLOT_COUNT] = 1
        let enable_block = self.compile_block(|b| {
            b.emit_set_slot(ctx.index, item, Expr::Imm16(1));
            b.emit_endif(ctx.end_block);
        });

        // Timer5 is a special case because 5-minute days are the default but the item will not be
        // in the player's inventory. So even if we don't have the item, we have to disable it if
        // the other two timers are also not acquired.
        //
        // Psuedocode:
        //   if (item[TIMER_10] == 0 && item[TIMER_15] == 0) {
        //     vars[SLOT_ITEM] = <item>
        //     vars[SLOT_COUNT] = 0
        //   } else {
        //     <enable>
        //   }
        if ctx.item == Some(ItemId::Timer5) {
            let _disable_block_2 = self.compile_block(|b| {
                b.emit_set_slot(ctx.index, item, Expr::Imm16(0));
                b.emit_endif(ctx.end_block);
            });
            let _if_no_timer_block = self.compile_block(|b| {
                let condition = compile_requirements([
                    Requirement::MissingItem(ItemId::Timer10),
                    Requirement::MissingItem(ItemId::Timer15),
                ]);
                b.emit_if_else(condition.unwrap(), enable_block);
            });
        }

        // if (!<acquired> && <requirements>)
        let _if_missing_and_visible_block = self.compile_block(|b| {
            let missing = acquired.negate();
            let requirements = compile_requirements(ctx.requirements.iter().copied());
            let condition = match requirements {
                Some(r) => Expr::BitAnd(BinaryOp::new(missing, r).into()),
                None => missing,
            };
            b.emit_if_else(condition, else_block);
        });
    }

    /// Compiles a slot which may hold more than one item.
    fn compile_slot_limit(&mut self, ctx: &ItemContext, item: ItemId) {
        // Psuedocode:
        //   if (<requirements>) {
        //     vars[0] = <limit> - item[<item>]
        //     if (vars[0] < 0) vars[0] = 0
        //     vars[SLOT_ITEM] = <item>
        //     vars[SLOT_COUNT] = vars[0]
        //   } else {
        //     vars[SLOT_ITEM] = -1
        //     vars[SLOT_COUNT] = 0
        //   }
        //
        // As in compile(), we build up the blocks backwards to make the edges easy to connect.

        // vars[SLOT_ITEM] = <item>
        // vars[SLOT_COUNT] = vars[0]
        let temp_var = Expr::Variable(Expr::Imm16(0).into());
        let set_block = self.compile_block(|b| {
            b.emit_set_slot(ctx.index, item, temp_var.clone());
            if ctx.hide_block.is_some() {
                // Jump over the hide block
                b.emit_endif(ctx.end_block);
            }
        });

        // vars[0] = 0
        let _reset_block = self.compile_block(|b| {
            b.emit_set(0, Expr::Imm16(0));
        });

        // vars[0] = <limit> - item[<item>]
        let _remaining_block = self.compile_block(|b| {
            let limit_expr = Expr::Imm16(ctx.limit);
            let item_expr = Expr::Item(Expr::Imm16(item.into()).into());
            b.emit_set(0, Expr::Subtract(BinaryOp::new(limit_expr, item_expr).into()));
            // if (vars[0] < 0)
            b.emit_if_else(Expr::Less(BinaryOp::new(temp_var, Expr::Imm16(0)).into()), set_block);
        });

        // If() statement for the visibility conditions
        if let Some(condition) = compile_requirements(ctx.requirements.iter().copied()) {
            let _if_visible_block = self.compile_block(|b| {
                b.emit_if_else(condition, ctx.hide_block.unwrap());
            });
        }
    }

    /// Calls `build` with a `BlockBuilder` and appends the resulting block to the script. The block
    /// will be chained on top of the previously-emitted block as necessary.
    fn compile_block<F>(&mut self, build: F) -> BlockId
    where
        F: FnOnce(&mut BlockBuilder),
    {
        let mut builder = BlockBuilder::new();
        build(&mut builder);
        self.emit_block(builder.finish())
    }

    fn emit_block(&mut self, mut block: CodeBlock) -> BlockId {
        let last = match block.commands.last() {
            Some(last) => last,
            None => panic!("cannot emit an empty block"),
        };

        if let Some(target) = last.goto_target() {
            // Last command is a goto command - use its target
            block.next_block = Some(*target);
        } else {
            // Next block is the last one emitted
            block.next_block = self.last_block.map(Ip::from);
            // Else block comes from an ending if() statement
            if let Some(args) = last.if_args() {
                block.else_block = Some(args.else_target);
            }
        }

        let id = self.script.push(Block::Code(block));
        self.last_block = Some(id);
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shop::parser::ShopParser;
    use crate::shop::Shop;
    use anyhow::Result;

    /// Convenience macro for initializing HashSets
    macro_rules! set {
        [$($value:expr),* $(,)*] => {
            vec![$($value),*].into_iter().collect::<::std::collections::HashSet<_>>()
        };
    }

    fn compile(slots: &[Slot]) -> Vec<Command> {
        let mut script = Script::new();
        let root = ShopCompiler::new(&mut script).compile(slots).append();
        script
            .reverse_postorder(root)
            .into_iter()
            .map(|id| script.block(id).commands().unwrap())
            .flatten()
            .cloned()
            .collect()
    }

    fn set(var: usize, value: i16) -> Command {
        SetArgs::new(SetExpr::Variable(Expr::Imm16(var as i16)), Expr::Imm16(value)).into()
    }

    fn set_expr(var: usize, value: Expr) -> Command {
        SetArgs::new(SetExpr::Variable(Expr::Imm16(var as i16)), value).into()
    }

    fn if_else(condition: Expr, else_block: BlockId) -> Command {
        Command::If(IfArgs { condition, else_target: Ip::Block(else_block) }.into())
    }

    fn endif(block: BlockId) -> Command {
        Command::EndIf(block.into())
    }

    fn eq(lhs: Expr, rhs: Expr) -> Expr {
        Expr::Equal(BinaryOp::new(lhs, rhs).into())
    }

    fn ne(lhs: Expr, rhs: Expr) -> Expr {
        Expr::NotEqual(BinaryOp::new(lhs, rhs).into())
    }

    fn eq_0(expr: Expr) -> Expr {
        eq(expr, Expr::Imm16(0))
    }

    fn ne_0(expr: Expr) -> Expr {
        ne(expr, Expr::Imm16(0))
    }

    fn lt_0(expr: Expr) -> Expr {
        Expr::Less(BinaryOp::new(expr, Expr::Imm16(0)).into())
    }

    fn sub(lhs: Expr, rhs: Expr) -> Expr {
        Expr::Subtract(BinaryOp::new(lhs, rhs).into())
    }

    fn and(lhs: Expr, rhs: Expr) -> Expr {
        Expr::BitAnd(BinaryOp::new(lhs, rhs).into())
    }

    fn or(lhs: Expr, rhs: Expr) -> Expr {
        Expr::BitOr(BinaryOp::new(lhs, rhs).into())
    }

    fn item(id: ItemId) -> Expr {
        Expr::Item(Expr::Imm16(id.into()).into())
    }

    fn atc(id: AtcId) -> Expr {
        Expr::Atc(Expr::Imm16(id.into()).into())
    }

    fn flag(index: i16) -> Expr {
        Expr::Flag(Expr::Imm16(index).into())
    }

    fn var(index: i16) -> Expr {
        Expr::Variable(Expr::Imm16(index).into())
    }

    fn compare_shops(a: &Shop, b: &Shop) {
        for (i, (slot_a, slot_b)) in a.slots().iter().zip(b.slots()).enumerate() {
            assert_eq!(slot_a, slot_b, "slot {}", i);
        }
    }

    fn compile_and_parse(shop: &Shop) -> Shop {
        let mut script = Script::new();
        let root = ShopCompiler::new(&mut script).compile(shop.slots()).append();
        Shop::with_slots(ShopParser::new(&script).parse(root))
    }

    #[test]
    fn test_compile_empty_slot() {
        let slots = vec![Slot::default()];
        let expected = vec![set(SHOP_ITEM_FIRST, -1), set(SHOP_COUNT_FIRST, 0), Command::Return];
        let actual = compile(&slots);
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn test_compile_unique_item_without_requirements() {
        let slots = vec![Slot { item: Some(ItemId::HotRod), limit: 1, requirements: set![] }];
        let expected = vec![
            if_else(eq_0(item(ItemId::HotRod)), BlockId::new(1)),
            set(SHOP_ITEM_FIRST, ItemId::HotRod.into()),
            set(SHOP_COUNT_FIRST, 1),
            endif(BlockId::new(0)),
            set(SHOP_ITEM_FIRST, ItemId::HotRod.into()),
            set(SHOP_COUNT_FIRST, 0),
            Command::Return,
        ];
        let actual = compile(&slots);
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn test_compile_atc_without_requirements() {
        let slots = vec![Slot { item: Some(ItemId::Toothbrush), limit: 1, requirements: set![] }];
        let expected = vec![
            if_else(eq_0(atc(AtcId::Toothbrush)), BlockId::new(1)),
            set(SHOP_ITEM_FIRST, ItemId::Toothbrush.into()),
            set(SHOP_COUNT_FIRST, 1),
            endif(BlockId::new(0)),
            set(SHOP_ITEM_FIRST, ItemId::Toothbrush.into()),
            set(SHOP_COUNT_FIRST, 0),
            Command::Return,
        ];
        let actual = compile(&slots);
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn test_compile_unique_item_with_requirements() {
        let slots = vec![Slot {
            item: Some(ItemId::HotRod),
            limit: 1,
            requirements: set![
                Requirement::HaveItem(ItemId::Spoon),
                Requirement::HaveAtc(AtcId::Toothbrush),
                Requirement::HaveFlag(123),
            ],
        }];
        let expected = vec![
            if_else(
                and(
                    eq_0(item(ItemId::HotRod)),
                    and(and(ne_0(item(ItemId::Spoon)), ne_0(atc(AtcId::Toothbrush))), flag(123)),
                ),
                BlockId::new(3),
            ),
            set(SHOP_ITEM_FIRST, ItemId::HotRod.into()),
            set(SHOP_COUNT_FIRST, 1),
            endif(BlockId::new(0)),
            if_else(ne_0(item(ItemId::HotRod)), BlockId::new(1)),
            set(SHOP_ITEM_FIRST, ItemId::HotRod.into()),
            set(SHOP_COUNT_FIRST, 0),
            endif(BlockId::new(0)),
            set(SHOP_ITEM_FIRST, -1),
            set(SHOP_COUNT_FIRST, 0),
            Command::Return,
        ];
        let actual = compile(&slots);
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn test_compile_timer_without_requirements() {
        let slots = vec![Slot { item: Some(ItemId::Timer15), limit: 1, requirements: set![] }];
        let expected = vec![
            if_else(
                and(
                    eq_0(item(ItemId::Timer15)),
                    ne(Expr::Time(Expr::Imm16(2).into()), Expr::Imm16(TIMER_15_RATE)),
                ),
                BlockId::new(1),
            ),
            set(SHOP_ITEM_FIRST, ItemId::Timer15.into()),
            set(SHOP_COUNT_FIRST, 1),
            endif(BlockId::new(0)),
            set(SHOP_ITEM_FIRST, ItemId::Timer15.into()),
            set(SHOP_COUNT_FIRST, 0),
            Command::Return,
        ];
        let actual = compile(&slots);
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn test_compile_timer5() {
        let slots = vec![Slot { item: Some(ItemId::Timer5), limit: 1, requirements: set![] }];
        let expected = vec![
            if_else(
                and(
                    eq_0(item(ItemId::Timer5)),
                    ne(Expr::Time(Expr::Imm16(2).into()), Expr::Imm16(TIMER_5_RATE)),
                ),
                BlockId::new(1),
            ),
            if_else(and(eq_0(item(ItemId::Timer10)), eq_0(item(ItemId::Timer15))), BlockId::new(2)),
            set(SHOP_ITEM_FIRST, ItemId::Timer5.into()),
            set(SHOP_COUNT_FIRST, 0),
            endif(BlockId::new(0)),
            set(SHOP_ITEM_FIRST, ItemId::Timer5.into()),
            set(SHOP_COUNT_FIRST, 1),
            endif(BlockId::new(0)),
            set(SHOP_ITEM_FIRST, ItemId::Timer5.into()),
            set(SHOP_COUNT_FIRST, 0),
            Command::Return,
        ];
        let actual = compile(&slots);
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn test_compile_timer_with_requirements() {
        let slots = vec![Slot {
            item: Some(ItemId::Timer15),
            limit: 1,
            requirements: set![Requirement::HaveFlag(123)],
        }];
        let expected = vec![
            if_else(
                and(
                    and(
                        eq_0(item(ItemId::Timer15)),
                        ne(Expr::Time(Expr::Imm16(2).into()), Expr::Imm16(TIMER_15_RATE)),
                    ),
                    flag(123),
                ),
                BlockId::new(3),
            ),
            set(SHOP_ITEM_FIRST, ItemId::Timer15.into()),
            set(SHOP_COUNT_FIRST, 1),
            endif(BlockId::new(0)),
            if_else(
                or(
                    ne_0(item(ItemId::Timer15)),
                    eq(Expr::Time(Expr::Imm16(2).into()), Expr::Imm16(TIMER_15_RATE)),
                ),
                BlockId::new(1),
            ),
            set(SHOP_ITEM_FIRST, ItemId::Timer15.into()),
            set(SHOP_COUNT_FIRST, 0),
            endif(BlockId::new(0)),
            set(SHOP_ITEM_FIRST, -1),
            set(SHOP_COUNT_FIRST, 0),
            Command::Return,
        ];
        let actual = compile(&slots);
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn test_compile_limit_item_without_requirements() {
        let slots = vec![Slot { item: Some(ItemId::HotRod), limit: 5, requirements: set![] }];
        let expected = vec![
            set_expr(0, sub(Expr::Imm16(5), item(ItemId::HotRod))),
            if_else(lt_0(var(0)), BlockId::new(1)),
            set(0, 0),
            set(SHOP_ITEM_FIRST, ItemId::HotRod.into()),
            set_expr(SHOP_COUNT_FIRST, var(0)),
            Command::Return,
        ];
        let actual = compile(&slots);
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn test_compile_limit_item_with_requirements() {
        let slots = vec![Slot {
            item: Some(ItemId::HotRod),
            limit: 5,
            requirements: set![
                Requirement::HaveItem(ItemId::Spoon),
                Requirement::HaveAtc(AtcId::Toothbrush),
                Requirement::HaveFlag(123),
            ],
        }];
        let expected = vec![
            if_else(
                and(and(ne_0(item(ItemId::Spoon)), ne_0(atc(AtcId::Toothbrush))), flag(123)),
                BlockId::new(1),
            ),
            set_expr(0, sub(Expr::Imm16(5), item(ItemId::HotRod))),
            if_else(lt_0(var(0)), BlockId::new(2)),
            set(0, 0),
            set(SHOP_ITEM_FIRST, ItemId::HotRod.into()),
            set_expr(SHOP_COUNT_FIRST, var(0)),
            endif(BlockId::new(0)),
            set(SHOP_ITEM_FIRST, -1),
            set(SHOP_COUNT_FIRST, 0),
            Command::Return,
        ];
        let actual = compile(&slots);
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn test_compile_and_parse_shop() -> Result<()> {
        let original = Shop::with_slots(vec![
            Slot { item: Some(ItemId::HotRod), limit: 1, requirements: set![] },
            Slot { item: Some(ItemId::HotRod), limit: 5, requirements: set![] },
            Slot::default(),
            Slot {
                item: Some(ItemId::HotRod),
                limit: 1,
                requirements: set![
                    Requirement::HaveItem(ItemId::Spoon),
                    Requirement::HaveAtc(AtcId::Toothbrush),
                    Requirement::HaveFlag(123),
                ],
            },
            Slot {
                item: Some(ItemId::HotRod),
                limit: 5,
                requirements: set![
                    Requirement::HaveItem(ItemId::Spoon),
                    Requirement::HaveAtc(AtcId::Toothbrush),
                    Requirement::HaveFlag(123),
                ],
            },
        ]);
        let parsed = compile_and_parse(&original);
        compare_shops(&parsed, &original);
        Ok(())
    }

    #[test]
    fn test_invalid_limits() -> Result<()> {
        let original = Shop::with_slots(vec![
            // Limits cannot be below 1
            Slot { item: Some(ItemId::HotRod), limit: 0, requirements: set![] },
            // Limits cannot be above 10
            Slot { item: Some(ItemId::HotRod), limit: 11, requirements: set![] },
            // ATCs must have a limit of 1
            Slot { item: Some(ItemId::Toothbrush), limit: 2, requirements: set![] },
            // Timers must have a limit of 1
            Slot { item: Some(ItemId::Timer15), limit: 2, requirements: set![] },
            // Empty slots must have a limit of 0
            Slot { item: None, limit: 1, requirements: set![] },
        ]);
        let expected = Shop::with_slots(vec![
            Slot { item: Some(ItemId::HotRod), limit: 1, requirements: set![] },
            Slot { item: Some(ItemId::HotRod), limit: 10, requirements: set![] },
            Slot { item: Some(ItemId::Toothbrush), limit: 1, requirements: set![] },
            Slot { item: Some(ItemId::Timer15), limit: 1, requirements: set![] },
            Slot { item: None, limit: 0, requirements: set![] },
        ]);
        let parsed = compile_and_parse(&original);
        compare_shops(&parsed, &expected);
        Ok(())
    }
}
