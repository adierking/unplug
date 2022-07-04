use super::{Error, Result, Script, ScriptLayout};
use crate::common::{WriteSeek, WriteTo};
use crate::event::bin::BinSerializer;
use crate::event::block::{Block, BlockId, DataBlock, Ip, WriteIp};
use crate::event::serialize::SerializeEvent;
use byteorder::{WriteBytesExt, LE};
use std::io::{self, Cursor, Seek, SeekFrom, Write};
use std::num::NonZeroU32;
use tracing::{debug, trace};

/// Maps block IDs to their file offsets
pub struct BlockOffsetMap {
    /// Each offset is stored as +1 so that 0 can be used for None
    offsets: Vec<Option<NonZeroU32>>,
}

impl BlockOffsetMap {
    /// Constructs a new `BlockOffsetMap` which can hold `len` block offsets.
    pub fn new(len: usize) -> Self {
        Self { offsets: vec![None; len] }
    }

    /// Inserts a block and its corresponding offset into the map.
    pub fn insert(&mut self, id: BlockId, offset: u32) {
        self.offsets[id.index()] = Some(NonZeroU32::new(offset + 1).expect("Invalid block offset"));
    }

    /// Gets a block's offset.
    /// ***Panics*** if the block is not in the map.
    pub fn get(&self, id: BlockId) -> u32 {
        match self.try_get(id) {
            Some(offset) => offset,
            None => panic!("block {:?} does not have an offset", id),
        }
    }

    /// Gets a block's offset if it has one.
    pub fn try_get(&self, id: BlockId) -> Option<u32> {
        id.get(&self.offsets).map(|o| o.get() - 1)
    }
}

/// A pending pointer fixup.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Fixup {
    /// An offset to a block should be written.
    BlockOffset(Ip),
    /// A relative offset should become an absolute offset.
    RelOffset(i32),
}

/// Wraps a writer with a WriteIp implementation.
/// When an IP is written, it is saved to a `fixups` list to be filled in later.
struct BlockWriter<W: Write + Seek> {
    writer: W,
    /// A list of (offset, fixup) pairs for offsets which still need to be filled in.
    fixups: Vec<(u32, Fixup)>,
}

impl<W: Write + Seek> BlockWriter<W> {
    fn new(writer: W) -> Self {
        Self { writer, fixups: vec![] }
    }

    /// Uses the provided `resolve_ip` function to fill in unresolved block offsets.
    fn fix_offsets<F>(&mut self, mut resolve_ip: F) -> io::Result<()>
    where
        F: FnMut(Ip) -> u32,
    {
        debug!("Applying {} offset fixups", self.fixups.len());
        for (base_offset, fixup) in self.fixups.drain(..) {
            self.writer.seek(SeekFrom::Start(base_offset.into()))?;
            match fixup {
                Fixup::BlockOffset(ip) => {
                    self.writer.write_u32::<LE>(resolve_ip(ip))?;
                }
                Fixup::RelOffset(offset) => {
                    self.writer.write_u32::<LE>(base_offset.wrapping_add(offset as u32))?;
                }
            }
        }
        Ok(())
    }

    fn write_placeholder(&mut self) -> io::Result<()> {
        // We can easily identify this if it ends up in a file
        self.writer.write_all(&[0xab, 0xab, 0xab, 0xab])
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
            self.writer.write_u32::<LE>(ip.offset().unwrap())
        } else {
            let base_offset = self.writer.seek(SeekFrom::Current(0))?.try_into().unwrap();
            self.fixups.push((base_offset, Fixup::BlockOffset(ip)));
            self.write_placeholder()
        }
    }

    fn write_rel_offset(&mut self, offset: i32) -> io::Result<()> {
        let base_offset = self.writer.seek(SeekFrom::Current(0))?.try_into().unwrap();
        self.fixups.push((base_offset, Fixup::RelOffset(offset)));
        self.write_placeholder()
    }
}

/// A blob of serialized block data to be written to a script.
struct Blob {
    /// The main block represented by the blob.
    block: BlockId,
    /// The offset of the main block within the source file, if known.
    src_offset: u32,
    /// The writer for block data.
    writer: BlockWriter<Cursor<Vec<u8>>>,
    /// Offsets of blocks within the blob.
    block_offsets: Vec<(BlockId, u32)>,
}

impl Blob {
    fn new(block: BlockId, capacity: usize) -> Self {
        Self {
            block,
            src_offset: u32::MAX,
            writer: BlockWriter::new(Cursor::new(vec![])),
            block_offsets: Vec::with_capacity(capacity),
        }
    }

    /// Begins a new block within the blob.
    fn begin(&mut self, block: BlockId) {
        let offset = self.writer.seek(SeekFrom::Current(0)).unwrap().try_into().unwrap();
        self.block_offsets.push((block, offset));
    }
}

/// Wraps a writer and writes script blocks to it.
pub struct ScriptWriter<'a> {
    /// The source script data.
    script: &'a Script,
    /// Pending blobs to be written out.
    blobs: Vec<Blob>,
    /// A bitset of block IDs which have been serialized.
    visited: Box<[u32]>,
}

impl<'a> ScriptWriter<'a> {
    /// Constructs a new `ScriptWriter` which can write blocks from `script` to `writer`.
    pub fn new(script: &'a Script) -> Self {
        Self {
            script,
            blobs: vec![],
            visited: vec![0; (script.len() + 31) / 32].into_boxed_slice(),
        }
    }

    /// Adds a block and all of its related data to the script.
    pub fn add_block(&mut self, block_id: BlockId) -> Result<()> {
        if self.visited(block_id) {
            return Ok(());
        }
        match self.script.block(block_id) {
            Block::Placeholder => panic!("Block {:?} is a placeholder", block_id),
            Block::Code(_) => self.add_subroutine(block_id),
            Block::Data(data) => self.add_data(block_id, data),
        }
    }

    fn add_subroutine(&mut self, entry_point: BlockId) -> Result<()> {
        let order = self.script.reverse_postorder(entry_point);
        let mut blob = Blob::new(entry_point, order.len());
        for block in order {
            self.write_code(&mut blob, block)?;
        }
        for &(_, fixup) in &blob.writer.fixups {
            if let Fixup::BlockOffset(ip) = fixup {
                let block_id = self.script.resolve_ip(ip)?;
                self.add_block(block_id)?;
            }
        }
        self.blobs.push(blob);
        Ok(())
    }

    fn add_data(&mut self, block_id: BlockId, data: &DataBlock) -> Result<()> {
        self.mark_visited(block_id);
        let mut blob = Blob::new(block_id, 1);
        blob.begin(block_id);
        self.write_data(&mut blob, data)?;
        self.blobs.push(blob);
        Ok(())
    }

    /// Writes a code block into a blob.
    fn write_code(&mut self, blob: &mut Blob, block_id: BlockId) -> Result<()> {
        if self.visited(block_id) {
            return Ok(());
        }

        self.mark_visited(block_id);
        blob.begin(block_id);
        let code = self.script.block(block_id).code().expect("Expected a code block");
        let mut ser = BinSerializer::new(&mut blob.writer);
        for command in &code.commands {
            command.serialize(&mut ser).map_err(|err| Error::WriteCommand(err.into()))?;
        }

        // If execution can flow directly out of this block into another one, it MUST be written next
        if code.commands.is_empty() || !code.commands.last().unwrap().is_goto() {
            if let Some(Ip::Block(next)) = code.next_block {
                assert!(!self.visited(next));
                self.write_code(blob, next)?;
            }
        }
        Ok(())
    }

    fn write_data(&mut self, blob: &mut Blob, data: &DataBlock) -> Result<()> {
        let mut ser = BinSerializer::new(&mut blob.writer);
        match data {
            DataBlock::ArrayI8(arr) => {
                for &x in arr {
                    blob.writer.write_i8(x)?;
                }
            }
            DataBlock::ArrayU8(arr) => {
                blob.writer.write_all(arr)?;
            }
            DataBlock::ArrayI16(arr) => {
                for &x in arr {
                    blob.writer.write_i16::<LE>(x)?;
                }
            }
            DataBlock::ArrayU16(arr) => {
                for &x in arr {
                    blob.writer.write_u16::<LE>(x)?;
                }
            }
            DataBlock::ArrayI32(arr) => {
                for &x in arr {
                    blob.writer.write_i32::<LE>(x)?;
                }
            }
            DataBlock::ArrayU32(arr) => {
                for &x in arr {
                    blob.writer.write_u32::<LE>(x)?;
                }
            }
            DataBlock::ArrayIp(arr) => {
                for &ip in arr {
                    ip.write_to(&mut blob.writer)?;
                }
                blob.writer.write_i32::<LE>(0)?;
                for &ip in arr {
                    let child_id = self.script.resolve_ip(ip)?;
                    self.add_block(child_id)?;
                }
            }
            DataBlock::ObjBone(bone) => {
                bone.serialize(&mut ser)?;
            }
            DataBlock::ObjPair(pair) => {
                pair.serialize(&mut ser)?;
            }
            DataBlock::String(string) => {
                string.write_to(&mut blob.writer)?;
            }
        }
        Ok(())
    }

    /// Writes out the final script data and returns a map from block IDs to file offsets.
    pub fn write_to(&mut self, writer: &mut dyn WriteSeek) -> Result<BlockOffsetMap> {
        if let Some(layout) = self.script.layout() {
            self.sort_blobs(layout);
        }

        let fixups = Vec::with_capacity(self.script.blocks().len());
        let mut writer = BlockWriter { writer, fixups };
        let offsets = self.write_blobs(&mut writer)?;

        let end_offset = writer.seek(SeekFrom::Current(0))?;
        writer.fix_offsets(|ip| {
            let block = match self.script.resolve_ip(ip) {
                Ok(id) => id,
                Err(e) => panic!("Failed to resolve IP {:?}: {}", ip, e),
            };
            match offsets.try_get(block) {
                Some(offset) => offset,
                None => panic!("Block {:?} was never written", block),
            }
        })?;

        writer.seek(SeekFrom::Start(end_offset))?;
        Ok(offsets)
    }

    /// Sorts the script blobs by offset according to `layout`.
    fn sort_blobs(&mut self, layout: &ScriptLayout) {
        let mut src_offsets = BlockOffsetMap::new(self.script.len());
        for &loc in layout.block_offsets() {
            src_offsets.insert(loc.id, loc.offset);
        }
        for blob in &mut self.blobs {
            if let Some(offset) = src_offsets.try_get(blob.block) {
                blob.src_offset = offset;
            }
        }
        self.blobs.sort_by_key(|b| b.src_offset);
    }

    /// Writes out the script blobs to `writer`.
    fn write_blobs<W: Write + Seek>(
        &mut self,
        writer: &mut BlockWriter<W>,
    ) -> Result<BlockOffsetMap> {
        let mut offsets = BlockOffsetMap::new(self.script.len());
        let mut base_offset = u32::try_from(writer.seek(SeekFrom::Current(0))?).unwrap();
        for blob in &self.blobs {
            // Merge in block offsets
            for &(block, offset) in &blob.block_offsets {
                offsets.insert(block, base_offset + offset);
            }
            // Merge in fixups
            for &(offset, ip) in &blob.writer.fixups {
                writer.fixups.push((base_offset + offset, ip));
            }
            let data = blob.writer.writer.get_ref();
            trace!("Write blob offset={:#x} len={:#x}", base_offset, data.len());
            writer.write_all(data)?;
            base_offset += u32::try_from(data.len()).unwrap();
        }
        Ok(offsets)
    }

    fn visited(&self, block: BlockId) -> bool {
        let (index, mask) = (block.index() / 32, 1 << (block.index() & 31));
        self.visited[index] & mask != 0
    }

    fn mark_visited(&mut self, block: BlockId) {
        let (index, mask) = (block.index() / 32, 1 << (block.index() & 31));
        self.visited[index] |= mask;
    }
}
