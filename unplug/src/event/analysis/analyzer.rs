use super::block::{BlockInfo, BlockInfoMap};
use super::state::LiveState;
use super::subroutine::{
    SubroutineEffects, SubroutineEffectsMap, SubroutineInfo, SubroutineInfoMap,
};
use super::value::{DefId, Definition, DefinitionMap, Label, Value, ValueKind};
use crate::event::{Block, BlockId, Ip};
use arrayvec::ArrayVec;
use log::debug;
use std::collections::{hash_map, HashSet, VecDeque};

/// Helper function to enqueue a block onto a workqueue only if it is not present.
fn enqueue_block(queue: &mut VecDeque<BlockId>, id: BlockId) {
    if !queue.contains(&id) {
        queue.push_back(id);
    }
}

/// Analyzes the data flow in a script.
#[derive(Clone)]
pub struct ScriptAnalyzer {
    /// The definition allocator.
    defs: DefinitionMap,
    /// Information about each analyzed block.
    blocks: BlockInfoMap,
    /// Information about each analyzed subroutine.
    subs: SubroutineInfoMap,
    /// Information about each available library subroutine.
    libs: Vec<SubroutineEffects>,
}

impl ScriptAnalyzer {
    /// Constructs a new `ScriptAnalyzer`.
    pub fn new() -> Self {
        Self {
            defs: DefinitionMap::with_key(),
            blocks: BlockInfoMap::new(),
            subs: SubroutineInfoMap::new(),
            libs: vec![],
        }
    }

    /// Constructs a new `ScriptAnalyzer` with library subroutine info.
    pub fn with_libs(lib_effects: &SubroutineEffectsMap, libs: &[BlockId]) -> Self {
        let mut result = Self::new();
        for &lib in libs {
            result.libs.push(match lib_effects.get(&lib) {
                Some(effects) => effects.clone(),
                None => panic!("Missing subroutine info for library subroutine at {:?}", lib),
            });
        }
        result
    }

    /// Consumes this `ScriptAnalyzer`, returning only the side effects for each subroutine.
    pub fn into_subroutine_effects(self) -> SubroutineEffectsMap {
        self.subs.into_iter().map(|(id, sub)| (id, sub.effects)).collect()
    }

    /// Gets the `SubroutineInfo` corresponding to an entry point, if any.
    pub fn subroutine(&self, entry_point: BlockId) -> Option<&SubroutineInfo> {
        self.subs.get(&entry_point)
    }

    /// Returns an iterator over all subroutines.
    pub fn subroutines(&self) -> impl Iterator<Item = &SubroutineInfo> {
        self.subs.values()
    }

    /// Gets the `BlockInfo` corresponding to a block, if any.
    pub fn block(&self, id: BlockId) -> Option<&BlockInfo> {
        self.blocks.get(id)
    }

    /// Gets a definition by its ID.
    pub fn def(&self, id: DefId) -> &Definition {
        &self.defs[id]
    }

    /// Returns an iterator over all definitions.
    pub fn defs(&self) -> impl Iterator<Item = (DefId, &Definition)> {
        self.defs.iter()
    }

    pub(crate) fn log_stats(&self) {
        debug!(
            "Script analysis found {} subroutines and {} definitions",
            self.subs.len(),
            self.defs.len()
        );
    }

    /// Analyzes the subroutine starting at `entry_point`.
    pub fn analyze_subroutine(&mut self, blocks: &[Block], entry_point: BlockId) {
        match self.subs.entry(entry_point) {
            hash_map::Entry::Occupied(_) => return,
            hash_map::Entry::Vacant(vacant) => {
                vacant.insert(SubroutineInfo::new(entry_point));
            }
        }
        let mut sub_info = SubroutineInfo::from_blocks(blocks, entry_point);
        self.analyze_dependencies(blocks, &sub_info);
        self.analyze_blocks(blocks, &sub_info);
        self.calc_edges(blocks, &sub_info);
        self.bubble_undefined(&mut sub_info);
        self.propagate_definitions(&sub_info);
        self.analyze_references(&mut sub_info);
        self.collect_outputs(&mut sub_info);
        self.subs.insert(entry_point, sub_info);
    }

    /// Recursively finds all offsets referenced by an entry point.
    pub fn find_references(&self, entry_point: BlockId) -> Vec<(ValueKind, Ip)> {
        let mut references = vec![];
        let mut visited = HashSet::<BlockId>::new();
        self.do_find_references(&mut references, &mut visited, entry_point);
        references
    }

    fn do_find_references(
        &self,
        references: &mut Vec<(ValueKind, Ip)>,
        visited: &mut HashSet<BlockId>,
        entry_point: BlockId,
    ) {
        if !visited.insert(entry_point) {
            return;
        }
        let sub_info = match self.subs.get(&entry_point) {
            Some(i) => i,
            None => panic!("Subroutine {:?} is not analyzed", entry_point),
        };
        references.extend(sub_info.references.iter().cloned());
        for &call in &sub_info.calls {
            self.do_find_references(references, visited, call);
        }
    }

    /// Analyzes all of the subroutines called by `sub`.
    fn analyze_dependencies(&mut self, blocks: &[Block], sub: &SubroutineInfo) {
        for &call in &sub.calls {
            self.analyze_subroutine(blocks, call);
        }
    }

    /// Populates the initial `BlockInfo` for each block in `sub`.
    fn analyze_blocks(&mut self, blocks: &[Block], sub: &SubroutineInfo) {
        self.blocks.expand(blocks.len());
        for &id in &sub.postorder {
            if self.blocks.get(id).is_some() {
                continue;
            }
            let code = id.get(blocks).code().unwrap();
            let mut state = LiveState::new();
            for cmd in &code.commands {
                state.analyze_command(cmd, &self.subs, &self.libs);
            }
            self.blocks.insert(state.into_block(id, &mut self.defs));
        }
    }

    /// Scans `sub`'s block hierarchy and fills in each block's `successors` and `predecessors`.
    fn calc_edges(&mut self, blocks: &[Block], sub: &SubroutineInfo) {
        for &id in &sub.postorder {
            let code = id.get(blocks).code().unwrap();
            let mut successors: ArrayVec<[_; 2]> = ArrayVec::new();
            if let Some(Ip::Block(next_id)) = code.next_block {
                self.blocks[next_id].predecessors.push(id);
                successors.push(next_id);
                if let Some(Ip::Block(else_id)) = code.else_block {
                    self.blocks[else_id].predecessors.push(id);
                    successors.push(else_id);
                }
            }
            self.blocks[id].successors = successors;
        }
    }

    /// Bubbles each block's undefined labels up to the entry point of `sub`.
    // :petbub:
    fn bubble_undefined(&mut self, sub: &mut SubroutineInfo) {
        // This mechanism provides a cheap way to account for the fact that almost all state is
        // global and that we don't have any readily-available information on a subroutine's inputs.
        // Technically every global variable should be considered live at the start of a function,
        // however, there are 2048 global variables and so this would be impractical. Instead, we
        // only account for the global variables that a subroutine actually uses by looking for
        // undefined labels in each block and then bubbling the labels up to the top. The algorithm
        // here is essentially the reverse of the output propagation algorithm: each block's
        // undefined label set is recomputed and then its predecessors are enqueued if it changed.
        let mut queue: VecDeque<_> = sub.postorder.iter().copied().collect();
        while let Some(id) = queue.pop_front() {
            let info = &self.blocks[id];
            let undefined = self.recalc_undefined(info);
            if undefined != info.undefined {
                for &pred in &info.predecessors {
                    enqueue_block(&mut queue, pred);
                }
                self.blocks[id].undefined = undefined;
            }
        }

        // Undefined values that bubbled up to the entry point are function inputs
        for &label in self.blocks[sub.entry_point].undefined.iter() {
            let def = self.defs.insert(Definition {
                label,
                origin: None,
                value: Value::Undefined(label),
            });
            sub.inputs.insert(def);
        }
    }

    /// Recomputes a block's undefined label set.
    fn recalc_undefined(&self, info: &BlockInfo) -> HashSet<Label> {
        let mut undefined = HashSet::new();

        // Add each undefined label referenced by the block's commands
        for &(_, value) in &info.references {
            if let Value::Undefined(label) = value {
                undefined.insert(label);
            }
        }

        // Add each undefined label referenced by the block's outputs
        for &output in info.generated.iter() {
            if let Value::Undefined(label) = self.defs[output].value {
                undefined.insert(label);
            }
        }

        // Add each undefined label from the block's successors, minus the labels that are killed by
        // this block
        for &id in &info.successors {
            let successor = &self.blocks[id];
            for label in successor.undefined.iter() {
                if !info.killed.contains(label) {
                    undefined.insert(*label);
                }
            }
        }
        undefined
    }

    /// Propagates all of the definitions in `sub` down to the bottom such that every block has a
    /// complete set of inputs and outputs.
    fn propagate_definitions(&mut self, sub: &SubroutineInfo) {
        // We want to iterate in reverse postorder so that we start at the top and work our way down
        // to the bottom
        let mut queue: VecDeque<_> = sub.postorder.iter().rev().copied().collect();
        while let Some(id) = queue.pop_front() {
            // Recompute inputs from the outputs of the predecessors
            let info = &self.blocks[id];
            let inputs = self.recalc_inputs(sub, info);

            // Propagate the inputs through the block to get the outputs
            let outputs = self.recalc_outputs(info, &inputs);
            let info = &mut self.blocks[id];
            info.inputs = inputs;

            // If the outputs changed, then each of the successors' inputs changed. Enqueue them.
            if outputs != info.outputs {
                info.outputs = outputs;
                for &successor in &info.successors {
                    enqueue_block(&mut queue, successor);
                }
            }
        }
    }

    /// Recomputes a block's inputs by taking the union of its predecessors' outputs.
    fn recalc_inputs(&self, sub: &SubroutineInfo, info: &BlockInfo) -> HashSet<DefId> {
        // If the block is the entry point, just take the subroutine's inputs
        if info.id == sub.entry_point {
            return sub.inputs.clone();
        }
        let mut inputs = HashSet::new();
        for &pred in info.predecessors.iter() {
            inputs.extend(self.blocks[pred].outputs.iter());
        }
        inputs
    }

    /// Recomputes a block's outputs from its inputs.
    fn recalc_outputs(&self, info: &BlockInfo, inputs: &HashSet<DefId>) -> HashSet<DefId> {
        // OUT = IN - KILL + GEN
        let mut outputs = HashSet::new();
        for &input in inputs.iter() {
            let def = &self.defs[input];
            if !info.killed.contains(&def.label) {
                outputs.insert(input);
            }
        }
        outputs.extend(info.generated.iter());
        outputs
    }

    /// Resolves the references for each block in `sub` after inputs and outputs have been fully
    /// computed. References to offsets are added to `sub`'s references, and references which are
    /// still undefined are added to its `input_kinds`.
    fn analyze_references(&self, sub: &mut SubroutineInfo) {
        for &block_id in &sub.postorder {
            for (kind, value) in &self.blocks[block_id].references {
                let references = &mut sub.references;
                let input_kinds = &mut sub.effects.input_kinds;
                self.visit_value(block_id, *value, |v| match v {
                    Value::Offset(offset) => {
                        references.insert((kind.clone(), Ip::Offset(offset)));
                    }
                    Value::Undefined(label) => {
                        input_kinds.insert(label, kind.clone());
                    }
                });
            }
        }
    }

    /// Expands a `value` referenced by a block and invokes `visitor` for each of the values it
    /// expands to.
    fn visit_value<F>(&self, block_id: BlockId, value: Value, mut visitor: F)
    where
        F: FnMut(Value),
    {
        let mut visited = HashSet::<DefId>::new();
        self.do_visit_value(&mut visited, block_id, value, &mut visitor);
    }

    fn do_visit_value<F>(
        &self,
        visited: &mut HashSet<DefId>,
        block_id: BlockId,
        value: Value,
        visitor: &mut F,
    ) where
        F: FnMut(Value),
    {
        match value {
            Value::Offset(_) => {
                visitor(value);
            }
            Value::Undefined(label) => {
                for &id in self.blocks[block_id].inputs.iter() {
                    let def = &self.defs[id];
                    if def.label == label && visited.insert(id) {
                        if let Some(origin) = def.origin {
                            self.do_visit_value(visited, origin, def.value, visitor);
                        } else {
                            visitor(def.value);
                        }
                    }
                }
            }
        }
    }

    /// Scans through the blocks in `sub` and collects the final sets of killed labels and output
    /// definitions for the subroutine.
    fn collect_outputs(&self, sub: &mut SubroutineInfo) {
        let mut killed = HashSet::new();
        for &id in &sub.postorder {
            killed.extend(self.blocks[id].killed.iter());
        }
        sub.effects.killed = killed;

        let mut output_defs = HashSet::new();
        for &id in &sub.exit_points {
            output_defs.extend(self.blocks[id].outputs.iter());
        }

        let mut sub_outputs = HashSet::new();
        for output in output_defs {
            let def = &self.defs[output];
            if let Some(origin) = def.origin {
                self.visit_value(origin, def.value, |v| {
                    sub_outputs.insert((def.label, v));
                });
            }
        }
        sub.effects.outputs = sub_outputs;
    }
}

impl Default for ScriptAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::super::value::ArrayKind;
    use super::*;
    use crate::event::command::{AttachArgs, Command, IfArgs, SetArgs};
    use crate::event::expr::{ArrayElementExpr, BinaryOp, Expr, SetExpr};
    use crate::event::CodeBlock;

    #[test]
    fn test_input_kinds() {
        let offset1 = Ip::Offset(1234);
        let offset2 = Ip::Offset(2345);
        let blocks: &[Block] = &[
            /* 0 */
            Block::Code(CodeBlock {
                commands: vec![
                    Command::PushBp,
                    Command::SetSp(Expr::AddressOf(offset1).into()),
                    Command::SetSp(Expr::Random(Expr::Imm32(1).into()).into()),
                    Command::Run(BlockId::new(1).into()),
                    Command::PopBp,
                    Command::Return,
                ],
                next_block: None,
                else_block: None,
            }),
            /* 1 */
            Block::Code(CodeBlock {
                commands: vec![
                    SetArgs::new(SetExpr::from_var(0), Expr::Stack(0)).into(),
                    SetArgs::new(SetExpr::from_var(1), Expr::from_var(0)).into(),
                    Command::If(Box::new(IfArgs {
                        condition: Expr::Stack(1),
                        else_target: BlockId::new(3).into(),
                    })),
                ],
                next_block: Some(BlockId::new(2).into()),
                else_block: Some(BlockId::new(3).into()),
            }),
            /* 2 */
            Block::Code(CodeBlock {
                commands: vec![SetArgs::new(SetExpr::from_var(1), Expr::AddressOf(offset2)).into()],
                next_block: Some(BlockId::new(3).into()),
                else_block: None,
            }),
            /* 3 */
            Block::Code(CodeBlock {
                commands: vec![
                    Command::Attach(Box::new(AttachArgs {
                        obj: Expr::Imm32(20000),
                        event: Expr::from_var(1),
                    })),
                    Command::Return,
                ],
                next_block: None,
                else_block: None,
            }),
        ];

        let mut analyzer = ScriptAnalyzer::new();
        analyzer.analyze_subroutine(blocks, BlockId::new(0));
        let sub1 = analyzer.subroutine(BlockId::new(0)).unwrap();
        let sub2 = analyzer.subroutine(BlockId::new(1)).unwrap();

        assert!(sub1.references.contains(&(ValueKind::Event, offset1)));
        assert!(sub2.references.contains(&(ValueKind::Event, offset2)));
    }

    #[test]
    fn test_resolve_output() {
        let offset1 = Ip::Offset(1234);
        let offset2 = Ip::Offset(2345);
        let offset3 = Ip::Offset(3456);
        let blocks: &[Block] = &[
            /* 0 */
            Block::Code(CodeBlock {
                commands: vec![
                    Command::PushBp,
                    Command::SetSp(Expr::AddressOf(offset1).into()),
                    Command::SetSp(Expr::Random(Expr::Imm32(1).into()).into()),
                    SetArgs::new(SetExpr::Result1, Expr::AddressOf(offset3)).into(),
                    Command::Run(BlockId::new(1).into()),
                    Command::PopBp,
                    Command::Attach(Box::new(AttachArgs {
                        obj: Expr::Imm32(20000),
                        event: Expr::Result1,
                    })),
                    Command::Return,
                ],
                next_block: None,
                else_block: None,
            }),
            /* 1 */
            Block::Code(CodeBlock {
                commands: vec![
                    SetArgs::new(SetExpr::from_var(0), Expr::Stack(0)).into(),
                    SetArgs::new(SetExpr::from_var(1), Expr::from_var(0)).into(),
                    Command::If(Box::new(IfArgs {
                        condition: Expr::Stack(1),
                        else_target: BlockId::new(3).into(),
                    })),
                ],
                next_block: Some(BlockId::new(2).into()),
                else_block: Some(BlockId::new(3).into()),
            }),
            /* 2 */
            Block::Code(CodeBlock {
                commands: vec![SetArgs::new(SetExpr::from_var(1), Expr::AddressOf(offset2)).into()],
                next_block: Some(BlockId::new(3).into()),
                else_block: None,
            }),
            /* 3 */
            Block::Code(CodeBlock {
                commands: vec![
                    SetArgs::new(SetExpr::Result1, Expr::from_var(1)).into(),
                    Command::Return,
                ],
                next_block: None,
                else_block: None,
            }),
        ];

        let mut analyzer = ScriptAnalyzer::new();
        analyzer.analyze_subroutine(blocks, BlockId::new(0));
        let sub = analyzer.subroutine(BlockId::new(0)).unwrap();

        assert!(sub.references.contains(&(ValueKind::Event, offset1)));
        assert!(sub.references.contains(&(ValueKind::Event, offset2)));
        assert!(!sub.references.contains(&(ValueKind::Event, offset3)));
    }

    #[test]
    fn test_analyze_lib() {
        let offset1 = Ip::Offset(1234);
        let offset2 = Ip::Offset(2345);
        let offset3 = Ip::Offset(3456);

        let lib_blocks: &[Block] = &[
            /* 0 */
            Block::Code(CodeBlock {
                commands: vec![
                    Command::Attach(Box::new(AttachArgs {
                        obj: Expr::Imm32(0),
                        event: Expr::Stack(0),
                    })),
                    Command::Return,
                ],
                next_block: None,
                else_block: None,
            }),
            /* 1 */
            Block::Code(CodeBlock {
                commands: vec![
                    SetArgs::new(SetExpr::from_var(0), Expr::Stack(0)).into(),
                    SetArgs::new(SetExpr::from_var(1), Expr::from_var(0)).into(),
                    Command::If(Box::new(IfArgs {
                        condition: Expr::Stack(1),
                        else_target: BlockId::new(3).into(),
                    })),
                ],
                next_block: Some(BlockId::new(2).into()),
                else_block: Some(BlockId::new(3).into()),
            }),
            /* 2 */
            Block::Code(CodeBlock {
                commands: vec![SetArgs::new(SetExpr::from_var(1), Expr::AddressOf(offset3)).into()],
                next_block: Some(BlockId::new(3).into()),
                else_block: None,
            }),
            /* 3 */
            Block::Code(CodeBlock {
                commands: vec![
                    SetArgs::new(SetExpr::Result1, Expr::from_var(1)).into(),
                    Command::Return,
                ],
                next_block: None,
                else_block: None,
            }),
        ];

        let script_blocks: &[Block] = &[Block::Code(CodeBlock {
            commands: vec![
                Command::PushBp,
                Command::SetSp(Expr::AddressOf(offset1).into()),
                Command::Lib(0),
                Command::PopBp,
                Command::SetSp(Expr::AddressOf(offset2).into()),
                Command::SetSp(Expr::Random(Expr::Imm32(1).into()).into()),
                Command::Lib(1),
                Command::PopBp,
                Command::Attach(Box::new(AttachArgs {
                    obj: Expr::Imm32(20000),
                    event: Expr::Result1,
                })),
                Command::Return,
            ],
            next_block: None,
            else_block: None,
        })];

        let mut lib_analyzer = ScriptAnalyzer::new();
        lib_analyzer.analyze_subroutine(lib_blocks, BlockId::new(0));
        lib_analyzer.analyze_subroutine(lib_blocks, BlockId::new(1));
        let lib_effects = lib_analyzer.into_subroutine_effects();

        let mut analyzer =
            ScriptAnalyzer::with_libs(&lib_effects, &[BlockId::new(0), BlockId::new(1)]);
        analyzer.analyze_subroutine(script_blocks, BlockId::new(0));
        let sub = analyzer.subroutine(BlockId::new(0)).unwrap();

        assert!(sub.references.contains(&(ValueKind::Event, offset1)));
        assert!(sub.references.contains(&(ValueKind::Event, offset2)));
        assert!(sub.references.contains(&(ValueKind::Event, offset3)));
    }

    #[test]
    fn test_ip_array() {
        let offset = Ip::Offset(1234);

        let blocks: &[Block] = &[Block::Code(CodeBlock {
            commands: vec![
                SetArgs::new(
                    SetExpr::from_var(0),
                    Expr::ArrayElement(Box::new(ArrayElementExpr {
                        element_type: Expr::Imm32(-4),
                        index: Expr::Stack(0),
                        address: Expr::AddressOf(offset),
                    })),
                )
                .into(),
                SetArgs::new(
                    SetExpr::from_var(0),
                    Expr::Add(Box::new(BinaryOp {
                        lhs: Expr::from_var(0),
                        rhs: Expr::AddressOf(Ip::Offset(0)),
                    })),
                )
                .into(),
                SetArgs::new(
                    SetExpr::from_var(0),
                    Expr::ArrayElement(Box::new(ArrayElementExpr {
                        element_type: Expr::Imm32(-4),
                        index: Expr::Stack(1),
                        address: Expr::from_var(0),
                    })),
                )
                .into(),
            ],
            next_block: None,
            else_block: None,
        })];

        let mut analyzer = ScriptAnalyzer::new();
        analyzer.analyze_subroutine(blocks, BlockId::new(0));
        let sub = analyzer.subroutine(BlockId::new(0)).unwrap();

        assert!(sub.references.contains(&(ValueKind::Array(ArrayKind::I32), offset)));
        assert!(sub.references.contains(&(
            ValueKind::Array(ArrayKind::Ip(ValueKind::Array(ArrayKind::I32).into())),
            offset
        )));
    }

    #[test]
    fn test_set_pad7() {
        let offset = Ip::Offset(1234);

        let blocks: &[Block] = &[Block::Code(CodeBlock {
            commands: vec![
                SetArgs::new(SetExpr::Pad(Expr::Imm32(7)), Expr::AddressOf(offset)).into()
            ],
            next_block: None,
            else_block: None,
        })];

        let mut analyzer = ScriptAnalyzer::new();
        analyzer.analyze_subroutine(blocks, BlockId::new(0));
        let sub = analyzer.subroutine(BlockId::new(0)).unwrap();

        assert!(sub.references.contains(&(ValueKind::Array(ArrayKind::I16), offset)));
    }
}
