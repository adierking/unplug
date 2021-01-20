use super::{Error, Result, Script};
use crate::common::{WriteSeek, WriteTo};
use crate::event::block::{Block, BlockId, DataBlock, Ip, WriteIp};
use byteorder::{WriteBytesExt, LE};
use log::debug;
use std::collections::HashSet;
use std::convert::TryFrom;
use std::io::{self, Seek, SeekFrom, Write};
use std::num::NonZeroU32;

/// Maps block IDs to their file offsets
struct BlockOffsetMap {
    /// Each offset is stored as +1 so that 0 can be used for None
    offsets: Vec<Option<NonZeroU32>>,
}

impl BlockOffsetMap {
    /// Constructs a new `BlockOffsetMap` which can hold `len` block offsets.
    fn new(len: usize) -> Self {
        Self { offsets: vec![None; len] }
    }

    /// Inserts a block and its corresponding offset into the map.
    fn insert(&mut self, id: BlockId, offset: u32) {
        self.offsets[id.index()] = Some(NonZeroU32::new(offset + 1).expect("Invalid block offset"));
    }

    /// Gets a block's offset if it has one.
    fn get(&self, id: BlockId) -> Option<u32> {
        id.get(&self.offsets).map(|o| o.get() - 1)
    }
}

/// Wraps a writer with a WriteIp implementation.
/// When an IP is written, it is saved to a `fixups` list to be filled in later.
struct BlockWriter<W: Write + Seek> {
    writer: W,
    /// A list of (offset, ip) pairs for block offsets which still need to be filled in.
    fixups: Vec<(u32, Ip)>,
}

impl<W: Write + Seek> BlockWriter<W> {
    /// Uses the provided `resolve_ip` function to fill in unresolved block offsets.
    fn fix_offsets<F>(&mut self, mut resolve_ip: F) -> Result<()>
    where
        F: FnMut(Ip) -> u32,
    {
        debug!("Applying {} offset fixups", self.fixups.len());
        for (offset, ip) in self.fixups.drain(..) {
            self.writer.seek(SeekFrom::Start(offset as u64))?;
            self.writer.write_u32::<LE>(resolve_ip(ip))?;
        }
        Ok(())
    }
}

impl<W: Write + Seek> Write for BlockWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.writer.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

impl<W: Write + Seek> Seek for BlockWriter<W> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.writer.seek(pos)
    }
}

impl<W: Write + Seek> WriteIp for BlockWriter<W> {
    fn write_ip(&mut self, ip: Ip) -> io::Result<()> {
        if ip.is_in_header() {
            // We can just directly write header offsets because they don't have blocks
            Ok(self.writer.write_u32::<LE>(ip.offset().unwrap())?)
        } else {
            // Save this offset for later so we can fix it up at the end
            // TODO: Can we check the BlockOffsetMap here?
            let offset = self.writer.seek(SeekFrom::Current(0))?;
            self.fixups.push((offset as u32, ip));
            // Write a placeholder that we can easily identify if it ends up in a file
            Ok(self.writer.write_all(&[0xab, 0xab, 0xab, 0xab])?)
        }
    }
}

/// Wraps a writer and writes script blocks to it.
pub struct ScriptWriter<'a> {
    script: &'a Script,
    writer: BlockWriter<&'a mut dyn WriteSeek>,
    block_offsets: BlockOffsetMap,
    /// A block must be immediately followed by its next block if it does not always jump. We always
    /// write each block's next block immediately afterwards, but it's possible that we may jump into
    /// a block which is expected to follow another block. This list lets us handle that case - for
    /// each block, we store the ID of the block that must immediately precede it. When we go to
    /// write a block, we keep looking up the block in this list until we find the beginning of the
    /// current "chain" of blocks.
    predecessors: Vec<Option<BlockId>>,
}

impl<'a> ScriptWriter<'a> {
    /// Constructs a new `ScriptWriter` which can write blocks from `script` to `writer`.
    pub fn new(script: &'a Script, writer: &'a mut dyn WriteSeek) -> Self {
        // The number of blocks tends to be a pretty good heuristic for the number of fixups
        let fixups = Vec::with_capacity(script.blocks().len());
        Self {
            script,
            writer: BlockWriter { writer, fixups },
            block_offsets: BlockOffsetMap::new(script.len()),
            predecessors: vec![None; script.len()],
        }
    }

    /// Finishes writing the script by fixing up block offsets. The writer will be left positioned at
    /// the end of the script data.
    pub fn finish(mut self) -> Result<()> {
        let end_offset = self.writer.seek(SeekFrom::Current(0))?;
        let block_offsets = &self.block_offsets;
        let script = &self.script;
        self.writer.fix_offsets(|ip| {
            let block = match script.resolve_ip(ip) {
                Ok(id) => id,
                Err(e) => panic!("Failed to resolve IP {:?}: {}", ip, e),
            };
            match block_offsets.get(block) {
                Some(offset) => offset,
                None => panic!("Block {:?} was never written", block),
            }
        })?;
        self.writer.seek(SeekFrom::Start(end_offset))?;
        Ok(())
    }

    /// Writes the subroutine starting at `entry_point` and returns its offset.
    pub fn write_subroutine(&mut self, entry_point: BlockId) -> Result<u32> {
        if let Some(offset) = self.block_offsets.get(entry_point) {
            return Ok(offset);
        }

        let mut visited = HashSet::new();
        self.find_predecessors(&mut visited, entry_point);

        // Write the code and keep track of which fixups it adds to the list
        let fixups_start = self.writer.fixups.len();
        let offset = self.write_code(entry_point)?;
        let fixups_end = self.writer.fixups.len();

        // Loop over all the fixups it added and write them out as well
        for i in fixups_start..fixups_end {
            let block_id = self.script.resolve_ip(self.writer.fixups[i].1)?;
            self.write_reference(block_id)?;
        }
        Ok(offset)
    }

    /// Finds each block's direct predecessor and populates the `predecessors` list.
    fn find_predecessors(&mut self, visited: &mut HashSet<BlockId>, block_id: BlockId) {
        if !visited.insert(block_id) || self.block_offsets.get(block_id).is_some() {
            return;
        }
        let code = self.script.block(block_id).code().expect("Expected a code block");
        if let Some(next_ip) = code.next_block {
            let next_block = next_ip.block().expect("next_block edge is not resolved");
            if !code.commands.last().unwrap().is_goto() {
                if next_block.get(&self.predecessors).is_some() {
                    panic!("Block {:?} has more than one direct predecessor", next_block);
                } else {
                    self.predecessors[next_block.index()] = Some(block_id);
                }
            }
            self.find_predecessors(visited, next_block);
            if let Some(else_ip) = code.else_block {
                let else_block = else_ip.block().expect("else_block edge is not resolved");
                self.find_predecessors(visited, else_block);
            }
        }
    }

    /// Writes a referenced block and returns its offset.
    fn write_reference(&mut self, block_id: BlockId) -> Result<u32> {
        match self.script.block(block_id) {
            Block::Placeholder => panic!("Block {:?} is a placeholder", block_id),
            Block::Code(_) => self.write_subroutine(block_id),
            Block::Data(data) => self.write_data(block_id, data),
        }
    }

    /// Writes a code block along with its adjacent blocks and returns its offset.
    fn write_code(&mut self, block_id: BlockId) -> Result<u32> {
        if let Some(offset) = self.block_offsets.get(block_id) {
            return Ok(offset);
        }

        // If this block is in the middle of a "chain", find the start
        let mut actual_id = block_id;
        while let Some(predecessor) = actual_id.get(&self.predecessors) {
            if self.block_offsets.get(*predecessor).is_some() {
                break;
            }
            actual_id = *predecessor;
        }

        let offset =
            u32::try_from(self.writer.seek(SeekFrom::Current(0))?).expect("File offset overflow");
        self.block_offsets.insert(actual_id, offset);
        let code = self.script.block(actual_id).code().expect("Expected a code block");
        for command in &code.commands {
            command.write_to(&mut self.writer).map_err(|err| Error::WriteCommand(err.into()))?;
        }

        if let Some(next_ip) = code.next_block {
            let next_block = next_ip.block().expect("next_block edge is not resolved");
            self.write_code(next_block)?;
            if let Some(else_ip) = code.else_block {
                let else_block = else_ip.block().expect("else_block edge is not resolved");
                self.write_code(else_block)?;
            }
        }
        Ok(self.block_offsets.get(actual_id).unwrap())
    }

    /// Writes a data block and returns its offset.
    fn write_data(&mut self, block_id: BlockId, block: &DataBlock) -> Result<u32> {
        if let Some(offset) = self.block_offsets.get(block_id) {
            return Ok(offset);
        }
        let offset =
            u32::try_from(self.writer.seek(SeekFrom::Current(0))?).expect("File offset overflow");
        self.block_offsets.insert(block_id, offset);
        match block {
            DataBlock::ArrayI8(arr) => {
                for &x in arr {
                    self.writer.write_i8(x)?;
                }
            }
            DataBlock::ArrayU8(arr) => {
                self.writer.write_all(arr)?;
            }
            DataBlock::ArrayI16(arr) => {
                for &x in arr {
                    self.writer.write_i16::<LE>(x)?;
                }
            }
            DataBlock::ArrayU16(arr) => {
                for &x in arr {
                    self.writer.write_u16::<LE>(x)?;
                }
            }
            DataBlock::ArrayI32(arr) => {
                for &x in arr {
                    self.writer.write_i32::<LE>(x)?;
                }
            }
            DataBlock::ArrayU32(arr) => {
                for &x in arr {
                    self.writer.write_u32::<LE>(x)?;
                }
            }
            DataBlock::ArrayIp(arr) => {
                for &ip in arr {
                    ip.write_to(&mut self.writer)?;
                }
                self.writer.write_i32::<LE>(0)?;
                for &ip in arr {
                    let child_id = self.script.resolve_ip(ip)?;
                    self.write_reference(child_id)?;
                }
            }
            DataBlock::ObjBone(bone) => {
                bone.write_to(&mut self.writer)?;
            }
            DataBlock::ObjPair(pair) => {
                pair.write_to(&mut self.writer)?;
            }
            DataBlock::String(string) => {
                string.write_to(&mut self.writer)?;
            }
        }
        Ok(offset)
    }
}
