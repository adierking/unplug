use super::value::{DefId, Label, Value, ValueKind};
use crate::event::{Block, BlockId, Command, Ip};
use std::collections::{HashMap, HashSet};

pub type SubroutineEffectsMap = HashMap<BlockId, SubroutineEffects>;
pub(super) type SubroutineInfoMap = HashMap<BlockId, SubroutineInfo>;

/// Information about a subroutine's side effects which is needed to analyze calls to it.
#[derive(Debug, Clone, Default)]
pub struct SubroutineEffects {
    /// The kinds of each of the subroutine's inputs.
    pub input_kinds: HashMap<Label, ValueKind>,
    /// The values of each of the subroutine's outputs.
    pub outputs: HashSet<(Label, Value)>,
    /// The labels which are killed by this subroutine - i.e. assigned to another value.
    pub killed: HashSet<Label>,
}

impl SubroutineEffects {
    /// Constructs an empty `SubroutineEffects`.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Information about a subroutine in a script.
#[derive(Debug, Clone)]
pub struct SubroutineInfo {
    /// The ID of the block where the subroutine starts.
    pub entry_point: BlockId,
    /// The IDs of the leaf blocks where the subroutine returns.
    pub exit_points: Vec<BlockId>,
    /// A postorder traversal of the blocks in the subroutine.
    pub postorder: Vec<BlockId>,
    /// The definitions for each of the subroutine's inputs.
    pub inputs: HashSet<DefId>,
    /// The offsets which are referenced by this subroutine.
    pub references: HashSet<(ValueKind, Ip)>,
    /// The entry points of other subroutines called by this subroutine.
    pub calls: Vec<BlockId>,
    /// The subroutine's side effects.
    pub effects: SubroutineEffects,
}

impl SubroutineInfo {
    /// Constructs an empty `SubroutineInfo` for an entry point.
    pub fn new(entry_point: BlockId) -> Self {
        Self {
            entry_point,
            exit_points: vec![],
            postorder: vec![],
            inputs: HashSet::new(),
            references: HashSet::new(),
            calls: vec![],
            effects: SubroutineEffects::new(),
        }
    }

    /// Constructs a new `SubroutineInfo` by traversing the blocks reachable from an entry point.
    pub fn from_blocks(blocks: &[Block], entry_point: BlockId) -> Self {
        let mut result = Self::new(entry_point);
        let mut visited = HashSet::new();
        result.find_blocks(blocks, &mut visited, entry_point);
        result.find_calls(blocks);
        result
    }

    /// Performs a postorder traversal from the entry point and populates the `postorder` and
    /// `exit_points` lists with the results.
    fn find_blocks(&mut self, blocks: &[Block], visited: &mut HashSet<BlockId>, id: BlockId) {
        let block = id.get(blocks);
        let code = block.code().expect("Expected a code block");
        if !visited.insert(id) {
            return;
        }
        if let Some(next_ip) = code.next_block {
            let next_id = next_ip.block().expect("next_block edge is not resolved");
            self.find_blocks(blocks, visited, next_id);
            if let Some(else_ip) = code.else_block {
                let else_id = else_ip.block().expect("else_block edge is not resolved");
                self.find_blocks(blocks, visited, else_id);
            }
        } else {
            // A block without a "next" edge must exit the subroutine
            self.exit_points.push(id);
        }
        self.postorder.push(id);
    }

    /// Scans the subroutine's blocks for calls to other subroutines and populates the `calls` list
    /// with the results.
    fn find_calls(&mut self, blocks: &[Block]) {
        for &id in &self.postorder {
            let code = id.get(blocks).code().unwrap();
            for cmd in &code.commands {
                if let Command::Run(ip) = cmd {
                    if let Ip::Block(entry_point) = *ip {
                        if !self.calls.contains(&entry_point) {
                            self.calls.push(entry_point);
                        }
                    } else {
                        panic!("Unresolved subroutine IP: {:?}", ip);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::CodeBlock;

    fn empty_block(next_index: Option<usize>, else_index: Option<usize>) -> Block {
        Block::Code(CodeBlock {
            commands: vec![],
            next_block: next_index.map(|i| BlockId::new(i as u32).into()),
            else_block: else_index.map(|i| BlockId::new(i as u32).into()),
        })
    }

    fn block_ids(indexes: &[usize]) -> Vec<BlockId> {
        indexes.iter().map(|i| BlockId::new(*i as u32)).collect()
    }

    #[test]
    fn test_sub_from_blocks() {
        let blocks: &[Block] = &[
            /* 0 */ empty_block(Some(1), None),
            /* 1 */ empty_block(Some(2), Some(3)),
            /* 2 */ empty_block(Some(4), Some(5)),
            /* 3 */ empty_block(Some(6), Some(7)),
            /* 4 */ empty_block(Some(8), None),
            /* 5 */ empty_block(None, None),
            /* 6 */ empty_block(Some(8), None),
            /* 7 */ empty_block(None, None),
            /* 8 */ empty_block(Some(0), None),
            /* 9 (unreachable) */ empty_block(None, None),
        ];
        let sub = SubroutineInfo::from_blocks(blocks, BlockId::new(0));
        assert_eq!(sub.entry_point, BlockId::new(0));
        assert_eq!(sub.exit_points, block_ids(&[5, 7]));
        assert_eq!(sub.postorder, block_ids(&[8, 4, 5, 2, 6, 7, 3, 1, 0]));
    }
}
