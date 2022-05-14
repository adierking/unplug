mod reader;
mod writer;

pub use reader::ScriptReader;
pub use writer::{BlockOffsetMap, ScriptWriter};

use super::analysis::SubroutineEffectsMap;
use super::block::{Block, BlockId, CodeBlock, Ip};
use super::command::{self, Command};
use std::collections::HashSet;
use std::convert::TryInto;
use std::io;
use std::iter::{DoubleEndedIterator, Enumerate, ExactSizeIterator, Extend, FusedIterator};
use std::slice;
use thiserror::Error;

/// The result type for script operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for script operations.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("script does not have layout information")]
    MissingLayout,

    #[error("offset {0:#x} is not mapped to a block")]
    InvalidOffset(u32),

    #[error("ID {0:?} is not mapped to a block")]
    InvalidId(BlockId),

    #[error("block at {0:#x} has an inconsistent type")]
    InconsistentType(u32),

    #[error("failed to read command at {offset:#x}")]
    ReadCommand { source: Box<command::Error>, offset: u32 },

    #[error("failed to write command")]
    WriteCommand(#[source] Box<command::Error>),

    #[error(transparent)]
    Io(Box<io::Error>),
}

from_error_boxed!(Error::Io, io::Error);

/// Describes the offset and ID of a block.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct BlockLocation {
    pub id: BlockId,
    pub offset: u32,
}

impl BlockLocation {
    const fn new(id: BlockId, offset: u32) -> Self {
        Self { id, offset }
    }
}

/// Describes the block and index of a command.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct CommandLocation {
    pub block: BlockLocation,
    pub index: usize,
}

impl CommandLocation {
    const fn new(block: BlockLocation, index: usize) -> Self {
        Self { block, index }
    }
}

#[derive(Clone)]
pub struct ScriptLayout {
    /// A list of each block's location in the source file. The list is sorted by offset and each
    /// block appears exactly once. This is useful for resolving `AddressOf` expressions.
    block_offsets: Vec<BlockLocation>,

    /// The side-effects of each subroutine as determined by a `ScriptAnalyzer`.
    subroutines: SubroutineEffectsMap,
}

impl ScriptLayout {
    /// Constructs a new `ScriptLayout`. `block_offsets` is a list of each block's offset in order by
    /// block ID.
    pub fn new(block_offsets: Vec<u32>, subroutines: SubroutineEffectsMap) -> Self {
        // Representing block_offsets in this fashion lets us guarantee that each block will appear
        // in the list exactly once. This is an important constraint for safe mutable iteration.
        let mut block_offsets: Vec<_> = block_offsets
            .into_iter()
            .enumerate()
            .map(|(i, o)| BlockLocation::new(i.try_into().unwrap(), o))
            .collect();
        block_offsets.sort_unstable_by_key(|loc| loc.offset);
        Self { block_offsets, subroutines }
    }

    /// Returns a reference to the script's block offset list, sorted by offset.
    pub fn block_offsets(&self) -> &[BlockLocation] {
        &self.block_offsets
    }

    /// Returns a reference to the script's subroutine side-effects map.
    pub fn subroutines(&self) -> &SubroutineEffectsMap {
        &self.subroutines
    }

    /// Looks up the ID of the block at `offset`.
    pub fn resolve_offset(&self, offset: u32) -> Result<BlockId> {
        match self.block_offsets.binary_search_by_key(&offset, |loc| loc.offset) {
            Ok(i) => Ok(self.block_offsets[i].id),
            Err(_) => Err(Error::InvalidOffset(offset)),
        }
    }
}

/// An event script.
#[derive(Clone, Default)]
pub struct Script {
    /// The blocks in the script.
    blocks: Vec<Block>,
    /// If the script was read from a file, holds information about the layout of the script.
    layout: Option<ScriptLayout>,
}

impl Script {
    /// Constructs an empty script.
    pub fn new() -> Self {
        Self::default()
    }

    /// Constructs a script from a list of blocks.
    pub fn with_blocks(blocks: impl Into<Vec<Block>>) -> Self {
        Self { blocks: blocks.into(), layout: None }
    }

    /// Constructs a script from a list of blocks and layout information.
    pub fn with_blocks_and_layout(blocks: impl Into<Vec<Block>>, layout: ScriptLayout) -> Self {
        Self { blocks: blocks.into(), layout: Some(layout) }
    }

    /// Returns a reference to a block by ID.
    pub fn block(&self, id: BlockId) -> &Block {
        id.get(&self.blocks)
    }

    /// Returns a mutable reference to a block by ID.
    pub fn block_mut(&mut self, id: BlockId) -> &mut Block {
        id.get_mut(&mut self.blocks)
    }

    /// Returns a slice containing the blocks in the script ordered by ID.
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// Returns a mutable slice containing the blocks in the script ordered by ID.
    pub fn blocks_mut(&mut self) -> &mut [Block] {
        &mut self.blocks
    }

    /// Returns `true` if the script does not contain any blocks.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Returns the number of blocks in the script.
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Returns the script's layout information if it was read from a file.
    pub fn layout(&self) -> Option<&ScriptLayout> {
        self.layout.as_ref()
    }

    /// Pushes `block` onto the end of the script and returns its ID.
    pub fn push(&mut self, block: Block) -> BlockId {
        let id = self.blocks.len().try_into().unwrap();
        self.blocks.push(block);
        id
    }

    /// Returns an iterator over the blocks in the script ordered by offset.
    /// ***Panics*** if the script does not have up-to-date layout information.
    pub fn blocks_ordered(&self) -> BlocksOrdered<'_> {
        let layout = match self.layout.as_ref() {
            Some(layout) => layout,
            None => panic!("Script does not have layout information"),
        };
        if layout.block_offsets.len() != self.blocks.len() {
            panic!("Script layout does not match the current block list");
        }
        BlocksOrdered { blocks: &self.blocks, locations: layout.block_offsets.iter() }
    }

    /// Returns a mutable iterator over the blocks in the script ordered by offset.
    /// ***Panics*** if the script does not have up-to-date layout information.
    pub fn blocks_ordered_mut(&mut self) -> BlocksOrderedMut<'_> {
        let layout = match self.layout.as_ref() {
            Some(layout) => layout,
            None => panic!("Script does not have layout information"),
        };
        if layout.block_offsets.len() != self.len() {
            panic!("Script layout does not match the current block list");
        }
        BlocksOrderedMut {
            blocks: self.blocks.as_mut_ptr(),
            len: self.len(),
            locations: layout.block_offsets.iter(),
        }
    }

    /// Returns an iterator over the commands in the script ordered by offset.
    /// ***Panics*** if the script does not have up-to-date layout information.
    pub fn commands_ordered(&self) -> CommandsOrdered<'_> {
        CommandsOrdered { block_iter: self.blocks_ordered(), command_iter: None }
    }

    /// Returns a mutable iterator over the commands in the script ordered by offset.
    /// ***Panics*** if the script does not have up-to-date layout information.
    pub fn commands_ordered_mut(&mut self) -> CommandsOrderedMut<'_> {
        CommandsOrderedMut { block_iter: self.blocks_ordered_mut(), command_iter: None }
    }

    /// Returns a postorder iterator over a tree of blocks starting from `root`.
    pub fn postorder(&self, root: BlockId) -> Postorder<'_> {
        Postorder::new(&self.blocks, root)
    }

    /// Returns a reverse postorder ordering of a tree of blocks starting from `root`.
    pub fn reverse_postorder(&self, root: BlockId) -> Vec<BlockId> {
        let mut postorder: Vec<_> = Postorder::new(&self.blocks, root).collect();
        postorder.reverse();
        postorder
    }

    /// Looks up the ID of the block corresponding to an `Ip`.
    pub fn resolve_ip(&self, ip: Ip) -> Result<BlockId> {
        match ip {
            Ip::Block(id) if id.index() < self.len() => Ok(id),
            Ip::Block(id) => Err(Error::InvalidId(id)),
            Ip::Offset(offset) => self.layout().ok_or(Error::MissingLayout)?.resolve_offset(offset),
        }
    }

    /// Empties the `from` block and chains it with `to`, effectively redirecting anything that
    /// references it. ***Panics*** if either block is not a code block.
    pub fn redirect_block(&mut self, from: BlockId, to: BlockId) {
        assert!(self.block(to).is_code(), "expected a code block");
        let from_block = self.block_mut(from);
        assert!(from_block.is_code(), "expected a code block");
        *from_block = Block::Code(CodeBlock {
            commands: vec![],
            next_block: Some(to.into()),
            else_block: None,
        });
    }
}

impl Extend<Block> for Script {
    fn extend<T: IntoIterator<Item = Block>>(&mut self, iter: T) {
        self.blocks.extend(iter);
    }
}

/// An iterator over the blocks in a script ordered by offset.
#[derive(Clone)]
pub struct BlocksOrdered<'a> {
    blocks: &'a [Block],
    locations: slice::Iter<'a, BlockLocation>,
}

impl<'a> Iterator for BlocksOrdered<'a> {
    type Item = (BlockLocation, &'a Block);

    fn next(&mut self) -> Option<Self::Item> {
        match self.locations.next() {
            Some(&loc) => Some((loc, loc.id.get(self.blocks))),
            None => None,
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.locations.size_hint()
    }
}

impl DoubleEndedIterator for BlocksOrdered<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        match self.locations.next_back() {
            Some(&loc) => Some((loc, loc.id.get(self.blocks))),
            None => None,
        }
    }
}

impl ExactSizeIterator for BlocksOrdered<'_> {}

impl FusedIterator for BlocksOrdered<'_> {}

/// A mutable iterator over the blocks in a script ordered by offset.
pub struct BlocksOrderedMut<'a> {
    blocks: *mut Block,
    len: usize,
    locations: slice::Iter<'a, BlockLocation>,
}

impl<'a> BlocksOrderedMut<'a> {
    fn get(&self, id: BlockId) -> &'a mut Block {
        let index = id.index();
        if index >= self.len {
            panic!("Invalid block index: {}", index);
        }
        // Safety:
        //
        // We validated above that the index is within the bounds of the block list.
        //
        // ScriptLayout::new() guarantees that each block ID appears in the location list exactly
        // once. It is not possible to obtain simultaneous mutable references to the same block.
        unsafe { &mut *self.blocks.add(index) }
    }
}

impl<'a> Iterator for BlocksOrderedMut<'a> {
    type Item = (BlockLocation, &'a mut Block);

    fn next(&mut self) -> Option<Self::Item> {
        self.locations.next().map(|&loc| (loc, self.get(loc.id)))
    }
}

impl DoubleEndedIterator for BlocksOrderedMut<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.locations.next_back().map(|&loc| (loc, self.get(loc.id)))
    }
}

impl ExactSizeIterator for BlocksOrderedMut<'_> {}

impl FusedIterator for BlocksOrderedMut<'_> {}

type CommandIter<'a> = Enumerate<slice::Iter<'a, Command>>;

/// An iterator over the commands in a script ordered by offset.
pub struct CommandsOrdered<'a> {
    block_iter: BlocksOrdered<'a>,
    command_iter: Option<(BlockLocation, CommandIter<'a>)>,
}

impl<'a> CommandsOrdered<'a> {
    fn next_impl(
        &mut self,
        next_block: impl Fn(&mut BlocksOrdered<'a>) -> Option<(BlockLocation, &'a Block)>,
        next_command: impl Fn(&mut CommandIter<'a>) -> Option<(usize, &'a Command)>,
    ) -> Option<(CommandLocation, &'a Command)> {
        loop {
            if let Some((loc, iter)) = &mut self.command_iter {
                match next_command(iter) {
                    Some((i, next)) => return Some((CommandLocation::new(*loc, i), next)),
                    None => self.command_iter = None,
                }
            }
            match next_block(&mut self.block_iter) {
                Some((loc, Block::Code(code))) => {
                    self.command_iter = Some((loc, code.commands.iter().enumerate()));
                }
                Some(_) => (),
                None => return None,
            }
        }
    }
}

impl<'a> Iterator for CommandsOrdered<'a> {
    type Item = (CommandLocation, &'a Command);

    fn next(&mut self) -> Option<Self::Item> {
        self.next_impl(|i| i.next(), |i| i.next())
    }
}

impl DoubleEndedIterator for CommandsOrdered<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.next_impl(|i| i.next_back(), |i| i.next_back())
    }
}

impl FusedIterator for CommandsOrdered<'_> {}

type CommandIterMut<'a> = Enumerate<slice::IterMut<'a, Command>>;

/// A mutable iterator over the commands in a script ordered by offset.
pub struct CommandsOrderedMut<'a> {
    block_iter: BlocksOrderedMut<'a>,
    command_iter: Option<(BlockLocation, CommandIterMut<'a>)>,
}

impl<'a> CommandsOrderedMut<'a> {
    fn next_impl(
        &mut self,
        next_block: impl Fn(&mut BlocksOrderedMut<'a>) -> Option<(BlockLocation, &'a mut Block)>,
        next_command: impl Fn(&mut CommandIterMut<'a>) -> Option<(usize, &'a mut Command)>,
    ) -> Option<(CommandLocation, &'a mut Command)> {
        loop {
            if let Some((loc, iter)) = &mut self.command_iter {
                match next_command(iter) {
                    Some((i, next)) => return Some((CommandLocation::new(*loc, i), next)),
                    None => self.command_iter = None,
                }
            }
            match next_block(&mut self.block_iter) {
                Some((loc, Block::Code(code))) => {
                    self.command_iter = Some((loc, code.commands.iter_mut().enumerate()));
                }
                Some(_) => (),
                None => return None,
            }
        }
    }
}

impl<'a> Iterator for CommandsOrderedMut<'a> {
    type Item = (CommandLocation, &'a mut Command);

    fn next(&mut self) -> Option<Self::Item> {
        self.next_impl(|i| i.next(), |i| i.next())
    }
}

impl DoubleEndedIterator for CommandsOrderedMut<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.next_impl(|i| i.next_back(), |i| i.next_back())
    }
}

impl FusedIterator for CommandsOrderedMut<'_> {}

/// A postorder iterator over a tree of blocks in a script.
pub struct Postorder<'a> {
    blocks: &'a [Block],
    current: Option<BlockId>,
    prev: BlockId,
    stack: Vec<BlockId>,
    visited: HashSet<BlockId>,
}

impl<'a> Postorder<'a> {
    /// Constructs a new postorder iterator over `blocks` starting at `start`.
    pub fn new(blocks: &'a [Block], start: BlockId) -> Self {
        Self { blocks, current: Some(start), prev: start, stack: vec![], visited: HashSet::new() }
    }
}

impl Iterator for Postorder<'_> {
    type Item = BlockId;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(current) = self.current {
                if !self.visited.insert(current) {
                    self.current = None;
                    self.prev = current;
                    continue;
                }

                // Move to else_block first. This ensures that the reverse postorder representation
                // puts the "true" branch first.
                self.stack.push(current);
                let code = current.get(self.blocks).code().expect("expected code block");
                self.current =
                    code.else_block.map(|i| i.block().expect("else_block is not resolved"));
            } else if let Some(&peek) = self.stack.last() {
                // If we didn't just come from next_block, then it hasn't been visited yet.
                let code = peek.get(self.blocks).code().expect("expected code block");
                if let Some(Ip::Block(next_block)) = code.next_block {
                    if self.prev != next_block {
                        self.current = Some(next_block);
                        continue;
                    }
                }

                // Visit this node and go up the stack
                self.prev = peek;
                self.stack.pop();
                return Some(peek);
            } else {
                // No more unvisited blocks in the tree
                return None;
            }
        }
    }
}

impl FusedIterator for Postorder<'_> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::CodeBlock;

    use lazy_static::lazy_static;

    fn test_block(i: i16) -> Block {
        Block::Code(CodeBlock {
            commands: vec![Command::Lib(i)],
            next_block: None,
            else_block: None,
        })
    }

    fn tree_block(next_block: Option<u32>, else_block: Option<u32>) -> Block {
        Block::Code(CodeBlock {
            commands: vec![],
            next_block: next_block.map(|i| Ip::Block(BlockId::new(i))),
            else_block: else_block.map(|i| Ip::Block(BlockId::new(i))),
        })
    }

    fn is_id_error<T>(result: Result<T>) -> bool {
        matches!(result, Err(Error::InvalidId(_)))
    }

    fn is_offset_error<T>(result: Result<T>) -> bool {
        matches!(result, Err(Error::InvalidOffset(_)))
    }

    lazy_static! {
        static ref TEST_SCRIPT: Script = {
            let blocks = vec![test_block(1), test_block(0), test_block(2)];
            let effects = SubroutineEffectsMap::new();
            let block_offsets = vec![0x456, 0x123, 0x789];
            let layout = ScriptLayout::new(block_offsets, effects);
            Script::with_blocks_and_layout(blocks, layout)
        };
        static ref LOCATION_0: BlockLocation = BlockLocation::new(BlockId::new(0), 0x456);
        static ref LOCATION_1: BlockLocation = BlockLocation::new(BlockId::new(1), 0x123);
        static ref LOCATION_2: BlockLocation = BlockLocation::new(BlockId::new(2), 0x789);
    }

    #[test]
    fn test_blocks_ordered() {
        let blocks: Vec<_> = TEST_SCRIPT.blocks_ordered().collect();
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].0, *LOCATION_1);
        assert_eq!(blocks[0].1.commands().unwrap(), &[Command::Lib(0)]);
        assert_eq!(blocks[1].0, *LOCATION_0);
        assert_eq!(blocks[1].1.commands().unwrap(), &[Command::Lib(1)]);
        assert_eq!(blocks[2].0, *LOCATION_2);
        assert_eq!(blocks[2].1.commands().unwrap(), &[Command::Lib(2)]);
    }

    #[test]
    fn test_blocks_ordered_rev() {
        let blocks: Vec<_> = TEST_SCRIPT.blocks_ordered().rev().collect();
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].0, *LOCATION_2);
        assert_eq!(blocks[0].1.commands().unwrap(), &[Command::Lib(2)]);
        assert_eq!(blocks[1].0, *LOCATION_0);
        assert_eq!(blocks[1].1.commands().unwrap(), &[Command::Lib(1)]);
        assert_eq!(blocks[2].0, *LOCATION_1);
        assert_eq!(blocks[2].1.commands().unwrap(), &[Command::Lib(0)]);
    }

    #[test]
    fn test_blocks_ordered_mut() {
        let mut script = TEST_SCRIPT.clone();
        let mut blocks: Vec<_> = script.blocks_ordered_mut().collect();
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].0, *LOCATION_1);
        assert_eq!(blocks[0].1.commands().unwrap(), &[Command::Lib(0)]);
        assert_eq!(blocks[1].0, *LOCATION_0);
        assert_eq!(blocks[1].1.commands().unwrap(), &[Command::Lib(1)]);
        assert_eq!(blocks[2].0, *LOCATION_2);
        assert_eq!(blocks[2].1.commands().unwrap(), &[Command::Lib(2)]);

        *blocks[0].1 = test_block(100);
        *blocks[1].1 = test_block(200);
        *blocks[2].1 = test_block(300);

        assert_eq!(script.block(BlockId::new(0)).commands().unwrap(), &[Command::Lib(200)]);
        assert_eq!(script.block(BlockId::new(1)).commands().unwrap(), &[Command::Lib(100)]);
        assert_eq!(script.block(BlockId::new(2)).commands().unwrap(), &[Command::Lib(300)]);
    }

    #[test]
    fn test_blocks_ordered_mut_rev() {
        let mut script = TEST_SCRIPT.clone();
        let mut blocks: Vec<_> = script.blocks_ordered_mut().rev().collect();
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].0, *LOCATION_2);
        assert_eq!(blocks[0].1.commands().unwrap(), &[Command::Lib(2)]);
        assert_eq!(blocks[1].0, *LOCATION_0);
        assert_eq!(blocks[1].1.commands().unwrap(), &[Command::Lib(1)]);
        assert_eq!(blocks[2].0, *LOCATION_1);
        assert_eq!(blocks[2].1.commands().unwrap(), &[Command::Lib(0)]);

        *blocks[0].1 = test_block(100);
        *blocks[1].1 = test_block(200);
        *blocks[2].1 = test_block(300);

        assert_eq!(script.block(BlockId::new(0)).commands().unwrap(), &[Command::Lib(200)]);
        assert_eq!(script.block(BlockId::new(1)).commands().unwrap(), &[Command::Lib(300)]);
        assert_eq!(script.block(BlockId::new(2)).commands().unwrap(), &[Command::Lib(100)]);
    }

    #[test]
    fn test_commands_ordered() {
        let commands: Vec<_> = TEST_SCRIPT.commands_ordered().collect();
        assert_eq!(commands.len(), 3);
        assert_eq!(commands[0].0, CommandLocation::new(*LOCATION_1, 0));
        assert_eq!(*commands[0].1, Command::Lib(0));
        assert_eq!(commands[1].0, CommandLocation::new(*LOCATION_0, 0));
        assert_eq!(*commands[1].1, Command::Lib(1));
        assert_eq!(commands[2].0, CommandLocation::new(*LOCATION_2, 0));
        assert_eq!(*commands[2].1, Command::Lib(2));
    }

    #[test]
    fn test_commands_ordered_rev() {
        let commands: Vec<_> = TEST_SCRIPT.commands_ordered().rev().collect();
        assert_eq!(commands.len(), 3);
        assert_eq!(commands[0].0, CommandLocation::new(*LOCATION_2, 0));
        assert_eq!(*commands[0].1, Command::Lib(2));
        assert_eq!(commands[1].0, CommandLocation::new(*LOCATION_0, 0));
        assert_eq!(*commands[1].1, Command::Lib(1));
        assert_eq!(commands[2].0, CommandLocation::new(*LOCATION_1, 0));
        assert_eq!(*commands[2].1, Command::Lib(0));
    }

    #[test]
    fn test_commands_ordered_mut() {
        let mut script = TEST_SCRIPT.clone();
        let mut commands: Vec<_> = script.commands_ordered_mut().collect();
        assert_eq!(commands.len(), 3);
        assert_eq!(commands[0].0, CommandLocation::new(*LOCATION_1, 0));
        assert_eq!(*commands[0].1, Command::Lib(0));
        assert_eq!(commands[1].0, CommandLocation::new(*LOCATION_0, 0));
        assert_eq!(*commands[1].1, Command::Lib(1));
        assert_eq!(commands[2].0, CommandLocation::new(*LOCATION_2, 0));
        assert_eq!(*commands[2].1, Command::Lib(2));

        *commands[0].1 = Command::Lib(100);
        *commands[1].1 = Command::Lib(200);
        *commands[2].1 = Command::Lib(300);

        assert_eq!(script.block(BlockId::new(0)).commands().unwrap(), &[Command::Lib(200)]);
        assert_eq!(script.block(BlockId::new(1)).commands().unwrap(), &[Command::Lib(100)]);
        assert_eq!(script.block(BlockId::new(2)).commands().unwrap(), &[Command::Lib(300)]);
    }

    #[test]
    fn test_commands_ordered_mut_rev() {
        let mut script = TEST_SCRIPT.clone();
        let mut commands: Vec<_> = script.commands_ordered_mut().rev().collect();
        assert_eq!(commands.len(), 3);
        assert_eq!(commands[0].0, CommandLocation::new(*LOCATION_2, 0));
        assert_eq!(*commands[0].1, Command::Lib(2));
        assert_eq!(commands[1].0, CommandLocation::new(*LOCATION_0, 0));
        assert_eq!(*commands[1].1, Command::Lib(1));
        assert_eq!(commands[2].0, CommandLocation::new(*LOCATION_1, 0));
        assert_eq!(*commands[2].1, Command::Lib(0));

        *commands[0].1 = Command::Lib(100);
        *commands[1].1 = Command::Lib(200);
        *commands[2].1 = Command::Lib(300);

        assert_eq!(script.block(BlockId::new(0)).commands().unwrap(), &[Command::Lib(200)]);
        assert_eq!(script.block(BlockId::new(1)).commands().unwrap(), &[Command::Lib(300)]);
        assert_eq!(script.block(BlockId::new(2)).commands().unwrap(), &[Command::Lib(100)]);
    }

    #[test]
    fn test_push() {
        let mut script = Script::new();
        assert_eq!(script.len(), 0);

        let b0 = script.push(test_block(0));
        assert_eq!(script.len(), 1);
        assert_eq!(b0.index(), 0);

        let b1 = script.push(test_block(1));
        assert_eq!(script.len(), 2);
        assert_eq!(b1.index(), 1);

        let b2 = script.push(test_block(2));
        assert_eq!(script.len(), 3);
        assert_eq!(b2.index(), 2);

        assert_eq!(script.blocks(), &[test_block(0), test_block(1), test_block(2)]);
    }

    #[test]
    fn test_extend() {
        let mut script = Script::new();
        assert_eq!(script.len(), 0);
        script.extend(vec![test_block(0), test_block(1)]);
        script.extend(vec![test_block(2), test_block(3)]);
        assert_eq!(script.blocks(), &[test_block(0), test_block(1), test_block(2), test_block(3)]);
    }

    #[test]
    fn test_resolve_ip() -> Result<()> {
        assert_eq!(TEST_SCRIPT.resolve_ip(BlockId::new(0).into())?, BlockId::new(0));
        assert_eq!(TEST_SCRIPT.resolve_ip(BlockId::new(1).into())?, BlockId::new(1));
        assert_eq!(TEST_SCRIPT.resolve_ip(BlockId::new(2).into())?, BlockId::new(2));
        assert!(is_id_error(TEST_SCRIPT.resolve_ip(BlockId::new(3).into())));

        assert_eq!(TEST_SCRIPT.resolve_ip(Ip::Offset(0x123))?, BlockId::new(1));
        assert_eq!(TEST_SCRIPT.resolve_ip(Ip::Offset(0x456))?, BlockId::new(0));
        assert_eq!(TEST_SCRIPT.resolve_ip(Ip::Offset(0x789))?, BlockId::new(2));
        assert!(is_offset_error(TEST_SCRIPT.resolve_ip(Ip::Offset(0x654))));
        assert!(is_offset_error(TEST_SCRIPT.resolve_ip(Ip::Offset(0x122))));
        assert!(is_offset_error(TEST_SCRIPT.resolve_ip(Ip::Offset(0x78a))));
        Ok(())
    }

    lazy_static! {
        static ref TREE_SCRIPT: Script = {
            let blocks = vec![
                /* 0 */ tree_block(Some(1), Some(4)),
                /* 1 */ tree_block(Some(2), Some(4)),
                /* 2 */ tree_block(Some(3), None),
                /* 3 */ tree_block(None, None),
                /* 4 */ tree_block(Some(0), Some(5)),
                /* 5 */ tree_block(None, None),
            ];
            Script::with_blocks(blocks)
        };
    }

    #[test]
    fn test_postorder() -> Result<()> {
        let order: Vec<_> = TREE_SCRIPT.postorder(BlockId::new(0)).map(|b| b.index()).collect();
        assert_eq!(order, [5, 4, 3, 2, 1, 0]);
        Ok(())
    }

    #[test]
    fn test_reverse_postorder() -> Result<()> {
        let order: Vec<_> =
            TREE_SCRIPT.reverse_postorder(BlockId::new(0)).into_iter().map(|b| b.index()).collect();
        assert_eq!(order, [0, 1, 2, 3, 4, 5]);
        Ok(())
    }

    #[test]
    fn test_redirect_block() -> Result<()> {
        let mut script = TREE_SCRIPT.clone();
        script.redirect_block(BlockId::new(1), BlockId::new(4));

        let old_block = script.block(BlockId::new(1)).code().unwrap();
        assert!(old_block.commands.is_empty());
        assert_eq!(old_block.next_block, Some(BlockId::new(4).into()));
        assert_eq!(old_block.else_block, None);

        let order: Vec<_> =
            script.reverse_postorder(BlockId::new(0)).into_iter().map(|b| b.index()).collect();
        assert_eq!(order, [0, 1, 4, 5]);
        Ok(())
    }
}
