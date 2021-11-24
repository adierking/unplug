use super::{Error, Result, Script, ScriptLayout};
use crate::common::{ReadFrom, ReadSeek};
use crate::event::analysis::{ArrayKind, ScriptAnalyzer, ValueKind};
use crate::event::block::{Block, BlockId, CodeBlock, DataBlock, Ip};
use crate::event::command::{self, Command};
use crate::event::expr::{self, ObjBone, ObjPair};
use byteorder::{ReadBytesExt, LE};
use std::collections::BTreeMap;
use std::convert::TryInto;
use std::ffi::CString;
use std::io::SeekFrom;
use tracing::{debug, error, trace};

/// The kind of data in a data block.
#[derive(Debug, Clone, PartialEq, Eq)]
enum DataType {
    Array(ArrayKind),
    ObjBone,
    ObjPair,
    String,
}

/// Describes the layout of a block in the bytecode file.
enum BlockLayout {
    Code(CodeLayout),
    Data(DataLayout),
}

impl BlockLayout {
    /// Gets the block's offset.
    fn offset(&self) -> u32 {
        match self {
            Self::Code(code) => code.start_offset,
            Self::Data(data) => data.start_offset,
        }
    }

    /// Returns whether this is a code block.
    fn is_code(&self) -> bool {
        matches!(self, Self::Code(_))
    }

    /// Returns whether this is a data block.
    fn is_data(&self) -> bool {
        matches!(self, Self::Data(_))
    }

    /// If this layout is for a code block, returns a mutable reference to the `CodeLayout`.
    fn code_mut(&mut self) -> Option<&mut CodeLayout> {
        match self {
            Self::Code(c) => Some(c),
            _ => None,
        }
    }
}

/// Describes the layout of a code block in the bytecode file.
struct CodeLayout {
    /// The offset of the first command in the block.
    start_offset: u32,
    /// The offset past the last command in the block.
    end_offset: u32,
    /// The offset of each command in the block.
    command_offsets: Vec<u32>,
}

/// Describes the layout of a data block in the bytecode file.
struct DataLayout {
    /// The offset of the data.
    start_offset: u32,
    /// The data's type.
    ty: DataType,
}

impl DataLayout {
    /// Updates the type of the layout based on a new reference to the data.
    /// Returns `false` if the hint is not compatible with the current type.
    fn add_type_hint(&mut self, hint: &DataType) -> bool {
        if self.ty == *hint {
            true
        } else if let (DataType::Array(current), DataType::Array(new)) = (&self.ty, &hint) {
            if current.element_size() == 4 && new.is_ip() {
                // Any 4-byte array type can be upgraded to an IP array
                self.ty = hint.clone();
                true
            } else {
                // Don't downgrade from IP to another type
                current.is_ip() && new.element_size() == 4
            }
        } else {
            false
        }
    }
}

/// Reads event commands and builds a `Script` from them.
pub struct ScriptReader<'r> {
    // This is intentionally a trait object to only instantiate each of the various `ReadFrom<R>`
    // implementations once.
    reader: &'r mut dyn ReadSeek,
    /// The blocks in the tree. IDs refer to this list.
    blocks: Vec<Block>,
    /// The layout of each block. Runs parallel to `blocks`, so IDs can refer to this list too.
    layouts: Vec<BlockLayout>,
    /// The IDs of blocks in the tree ordered by offset.
    offset_map: BTreeMap<u32, BlockId>,
    /// The script analyzer.
    analyzer: ScriptAnalyzer,
}

impl<'r> ScriptReader<'r> {
    /// Constructs a new `ScriptReader` which wraps a reader.
    pub fn new(reader: &'r mut dyn ReadSeek) -> Self {
        Self::with_analyzer(reader, ScriptAnalyzer::new())
    }

    /// Constructs a new `ScriptReader` which is aware of library function side-effects.
    /// This is required in order to support scripts which use the lib() command.
    pub fn with_libs(
        reader: &'r mut dyn ReadSeek,
        lib_script: &Script,
        lib_blocks: &[BlockId],
    ) -> Self {
        let layout = lib_script.layout().expect("Missing script layout information");
        let effects = layout.subroutine_effects();
        Self::with_analyzer(reader, ScriptAnalyzer::with_libs(effects, lib_blocks))
    }

    fn with_analyzer(reader: &'r mut dyn ReadSeek, analyzer: ScriptAnalyzer) -> Self {
        Self { reader, blocks: vec![], layouts: vec![], offset_map: BTreeMap::new(), analyzer }
    }

    /// Reads commands from an event starting at `start_offset` and returns the `BlockId` for the
    /// entry point.
    pub fn read_event(&mut self, start_offset: u32) -> Result<BlockId> {
        if let Some(&block) = self.offset_map.get(&start_offset) {
            return Ok(block);
        }

        // The game doesn't store any information about how large an event is, so we have to read it
        // in blocks and analyze the control flow. Start by reading the first block, which we know
        // for sure is a code block, and then keep analyzing and reading new blocks we discover.
        trace!("Reading event at {:#x}", start_offset);
        let entry_block = self.read_all_code(start_offset)?;
        self.resolve_edges(entry_block);

        // Use the analyzer to find all of the data that the code references.
        self.analyzer.analyze_subroutine(&self.blocks, entry_block);
        let mut references = self.analyzer.find_references(entry_block);
        // References must be sorted to make the block order deterministic
        references.sort_unstable_by_key(|(_, ip)| *ip);
        for (kind, ip) in references {
            self.process_reference(kind, ip)?;
        }
        Ok(entry_block)
    }

    /// Finishes reading events and constructs the final script data.
    pub fn finish(mut self) -> Result<Script> {
        let file_size = self.reader.seek(SeekFrom::End(0))? as u32;
        self.read_ip_arrays(file_size)?;
        self.read_data(file_size)?;
        self.analyzer.log_stats();
        debug!("Read {} script blocks", self.blocks.len());
        let block_offsets: Vec<_> = self.layouts.into_iter().map(|l| l.offset()).collect();
        let subroutines = self.analyzer.into_subroutine_effects();
        let layout = ScriptLayout::new(block_offsets, subroutines);
        Ok(Script::with_blocks_and_layout(self.blocks, layout))
    }

    /// Insert a block into the script and return its ID.
    fn insert(&mut self, block: Block, layout: BlockLayout) -> BlockId {
        let id = self.next_id();
        self.offset_map.insert(layout.offset(), id);
        self.blocks.push(block);
        self.layouts.push(layout);
        id
    }

    /// Gets the ID of the next block that will be created.
    fn next_id(&self) -> BlockId {
        assert!(self.blocks.len() == self.layouts.len());
        self.blocks.len().try_into().unwrap()
    }

    /// Returns a `(before, middle, after)` tuple of the blocks before, at, and after `offset`.
    fn surrounding(&self, offset: u32) -> (Option<BlockId>, Option<BlockId>, Option<BlockId>) {
        let mut before_range = self.offset_map.range(..offset);
        let mut after_range = self.offset_map.range(offset..);
        let before = before_range.next_back().map(|(_, &v)| v);
        let after1 = after_range.next().map(|(_, &v)| v);
        let after2 = after_range.next().map(|(_, &v)| v);
        if after1.is_some() && after1.unwrap().get(&self.layouts).offset() == offset {
            (before, after1, after2)
        } else {
            (before, None, after1)
        }
    }

    /// Splits a code block into a new block starting at `offset`.
    fn split(&mut self, id: BlockId, offset: u32) -> BlockId {
        let new_id = self.next_id();
        let block =
            id.get_mut(&mut self.blocks).code_mut().expect("Attempted to split a non-code block");
        let layout =
            id.get_mut(&mut self.layouts).code_mut().expect("Attempted to split a non-code layout");

        assert!(offset > layout.start_offset);
        assert!(offset < layout.end_offset);
        assert!(layout.command_offsets.len() == block.commands.len());

        let end_offset = layout.end_offset;
        layout.end_offset = offset;

        let split_index = layout.command_offsets.binary_search(&offset).unwrap();
        let commands = block.commands.split_off(split_index);
        let command_offsets = layout.command_offsets.split_off(split_index);

        let next_block = block.next_block.replace(Ip::Block(new_id));
        let else_block = block.else_block.take();

        let new_block = Block::Code(CodeBlock { commands, next_block, else_block });
        let new_layout =
            BlockLayout::Code(CodeLayout { start_offset: offset, end_offset, command_offsets });
        let actual_new_id = self.insert(new_block, new_layout);
        assert!(actual_new_id == new_id);
        actual_new_id
    }

    /// Processes a block referenced by another block.
    fn process_reference(&mut self, kind: ValueKind, ip: Ip) -> Result<()> {
        // Some scripts read values directly out of the stage header and object table. We don't want
        // to create blocks for these, so just ignore anything up to and including the offset of the
        // object table. This isn't technically correct for the globals script, but that will start
        // with the library table anyway.
        if ip.is_in_header() {
            return Ok(());
        }
        let offset = ip.offset().unwrap();
        match kind {
            ValueKind::Event => {
                self.read_event(offset)?;
            }
            ValueKind::Array(kind) => self.add_data(offset, DataType::Array(kind))?,
            ValueKind::String => self.add_data(offset, DataType::String)?,
            ValueKind::ObjBone => self.add_data(offset, DataType::ObjBone)?,
            ValueKind::ObjPair => self.add_data(offset, DataType::ObjPair)?,
        }
        Ok(())
    }

    /// Reads all code blocks immediately reachable from `offset` and returns the ID of the head.
    fn read_all_code(&mut self, offset: u32) -> Result<BlockId> {
        let (before, middle, after) = self.surrounding(offset);
        if let Some(middle_id) = middle {
            // There is already a block at this offset. We can skip, but make sure this is actually
            // a code block.
            let middle_layout = middle_id.get(&self.layouts);
            if !middle_layout.is_code() {
                error!("Block at {:#x} is not a code block", offset);
                return Err(Error::InconsistentType(offset));
            }
            return Ok(middle_id);
        }

        // If there is a block before this offset, it may already have the commands we want to put
        // in this block. Check if the current offset is inside the block, and if so, split it into
        // two separate blocks.
        if let Some(before_id) = before {
            let before_layout = before_id.get(&self.layouts);
            if let BlockLayout::Code(code) = before_layout {
                if offset < code.end_offset {
                    return Ok(self.split(before_id, offset));
                }
            }
        }

        // Read a new block up until the start of the next block (if any)
        let end_offset = after.map_or(std::u32::MAX, |i| i.get(&self.layouts).offset());
        self.reader.seek(SeekFrom::Start(offset as u64))?;
        let new_id = self.read_code(end_offset)?;
        let new_code = new_id.get(&self.blocks).code().unwrap();

        // Read the blocks immediately following this one
        let mut next_offsets = vec![];
        if let Some(Ip::Offset(offset)) = new_code.next_block {
            next_offsets.push(offset);
        }
        if let Some(Ip::Offset(offset)) = new_code.else_block {
            next_offsets.push(offset);
        }
        // If the code calls into any subroutines, read them as well
        for command in &new_code.commands {
            if let Command::Run(Ip::Offset(offset)) = *command {
                next_offsets.push(offset);
            }
        }
        for offset in next_offsets {
            self.read_all_code(offset)?;
        }
        Ok(new_id)
    }

    /// Reads commands into a new code block up until `end_offset` or a branch.
    fn read_code(&mut self, end_offset: u32) -> Result<BlockId> {
        let mut code = CodeBlock::new();
        let mut command_offsets: Vec<u32> = vec![];

        let start_offset: u32 =
            self.reader.seek(SeekFrom::Current(0))?.try_into().expect("File offset overflow");
        let mut offset = start_offset;
        loop {
            if offset >= end_offset {
                code.next_block = Some(end_offset.into());
                break;
            }

            let command = match Command::read_from(&mut self.reader) {
                Ok(c) => c,
                Err(source) if Self::is_known_error(&source, offset) => {
                    debug!("Ignoring block with known invalid code at {:#x}", offset);
                    Command::Abort
                }
                Err(source) => return Err(Error::ReadCommand { source: source.into(), offset }),
            };
            code.commands.push(command);
            command_offsets.push(offset);

            offset =
                self.reader.seek(SeekFrom::Current(0))?.try_into().expect("File offset overflow");

            let command = code.commands.last().unwrap();
            if let Some(goto_target) = command.goto_target() {
                // A goto command ends the block and jumps to the offset given in the command
                code.next_block = Some(*goto_target);
                break;
            } else if let Some(args) = command.if_args() {
                // A conditional ends the block and either continues execution or branches
                code.next_block = Some(offset.into());
                code.else_block = Some(args.else_target);
                break;
            } else if command.is_control_flow() {
                // Other control commands (break, return) end the block and do not branch
                break;
            }
        }

        let layout =
            BlockLayout::Code(CodeLayout { start_offset, end_offset: offset, command_offsets });
        Ok(self.insert(Block::Code(code), layout))
    }

    fn add_data(&mut self, offset: u32, ty: DataType) -> Result<()> {
        if let Some(&block_id) = self.offset_map.get(&offset) {
            let layout = block_id.get_mut(&mut self.layouts);
            match layout {
                BlockLayout::Code(_) => {
                    error!(
                        "Block type mismatch at {:#x}: already read as code but requested as {:?}",
                        offset, ty
                    );
                    return Err(Error::InconsistentType(offset));
                }
                BlockLayout::Data(data) => {
                    if !data.add_type_hint(&ty) {
                        error!(
                            "Block type mismatch at {:#x}: already read as {:?} but requested as {:?}",
                            offset, data.ty, ty
                        );
                        return Err(Error::InconsistentType(offset));
                    }
                }
            }
        } else {
            let layout = BlockLayout::Data(DataLayout { start_offset: offset, ty });
            self.insert(Block::Placeholder, layout);
        }
        Ok(())
    }

    /// Resolves block edges from file offsets to block IDs.
    fn resolve_edges(&mut self, start: BlockId) {
        for block in &mut self.blocks[start.index()..] {
            if let Block::Code(code) = block {
                if let Some(Ip::Offset(offset)) = code.next_block {
                    code.next_block = Some(self.offset_map[&offset].into());
                }
                if let Some(Ip::Offset(offset)) = code.else_block {
                    code.else_block = Some(self.offset_map[&offset].into());
                }

                for command in &mut code.commands {
                    if let Command::Run(target) = command {
                        if let Ip::Offset(offset) = target {
                            *target = self.offset_map[offset].into();
                        }
                    }
                }

                let last = code.commands.last_mut().unwrap();
                if let Some(target) = last.goto_target_mut() {
                    *target = code.next_block.expect("next_block is not resolved");
                } else if let Some(args) = last.if_args_mut() {
                    args.else_target = code.else_block.expect("else_block is not resolved");
                }
            }
        }
    }

    /// Reads the data block pointers out of each IP array.
    fn read_ip_arrays(&mut self, file_size: u32) -> Result<()> {
        struct PendingArray {
            block_id: BlockId,
            offset: u32,
            element_kind: ValueKind,
        }

        // The game generally only uses arrays of arrays, but for correctness purposes we support
        // arrays being able to point to anything, including other IP arrays. Keep looping until
        // there are no more new data blocks to process.
        let mut start = 0;
        let mut queue = vec![];
        while start < self.layouts.len() {
            queue.clear();
            trace!("Searching for IP arrays starting at {}", start);
            for (index, layout) in self.layouts[start..].iter().enumerate() {
                if let BlockLayout::Data(data) = layout {
                    if let DataType::Array(ArrayKind::Ip(kind)) = &data.ty {
                        queue.push(PendingArray {
                            block_id: index.try_into().unwrap(),
                            offset: data.start_offset,
                            element_kind: (**kind).clone(),
                        });
                    }
                }
            }
            start = self.layouts.len();

            for pending in queue.drain(..) {
                if !pending.block_id.get(&self.blocks).is_placeholder() {
                    continue;
                }

                // Put an upper bound on the array size by looking at the offset of the next block
                let (_, _, next) = self.surrounding(pending.offset);
                let next_offset =
                    next.map(|id| id.get(&self.layouts).offset()).unwrap_or(file_size);
                let max_len = ((next_offset - pending.offset) / 4) as usize;
                trace!("Reading IP array at {:#x} with max length {}", pending.offset, max_len);

                let mut offsets = Vec::with_capacity(max_len);
                self.reader.seek(SeekFrom::Start(pending.offset as u64))?;
                while offsets.len() < max_len {
                    // We don't have any context on how the array is used, so assume that it
                    // contains nothing but offsets and that it may be terminated by a zero or
                    // negative value.
                    let offset = self.reader.read_i32::<LE>()?;
                    if offset <= 0 {
                        break;
                    }
                    offsets.push(Ip::Offset(offset as u32));
                }

                // Just process each offset like it's a reference. This gives us free support for
                // all data types.
                for &offset in &offsets {
                    trace!(
                        "Processing IP array reference: {:?} at {:?}",
                        pending.element_kind,
                        offset
                    );
                    self.process_reference(pending.element_kind.clone(), offset)?;
                }

                self.blocks[pending.block_id.index()] = DataBlock::ArrayIp(offsets).into();
            }
        }

        Ok(())
    }

    /// Reads the remaining data blocks.
    fn read_data(&mut self, file_size: u32) -> Result<()> {
        let mut iter = self.offset_map.values().peekable();
        while let Some(id) = iter.next() {
            if !id.get(&self.layouts).is_data() {
                continue;
            }
            if !id.get(&self.blocks).is_placeholder() {
                continue;
            }
            let next_offset =
                iter.peek().map(|id| id.get(&self.layouts).offset()).unwrap_or(file_size);
            if let BlockLayout::Data(layout) = id.get_mut(&mut self.layouts) {
                let max_size = (next_offset - layout.start_offset) as usize;
                trace!(
                    "Reading {:?} at {:#x} with estimated size {:#x}",
                    layout.ty,
                    layout.start_offset,
                    max_size
                );
                self.reader.seek(SeekFrom::Start(layout.start_offset as u64))?;
                let data = match &layout.ty {
                    DataType::Array(kind) => Self::read_array(self.reader, kind, max_size)?,
                    DataType::ObjBone => ObjBone::read_from(&mut self.reader)?.into(),
                    DataType::ObjPair => ObjPair::read_from(&mut self.reader)?.into(),
                    DataType::String => CString::read_from(&mut self.reader)?.into(),
                };
                self.blocks[id.index()] = Block::Data(data);
            }
        }
        Ok(())
    }

    /// Reads an array `DataBlock` from a stream.
    fn read_array(
        reader: &mut dyn ReadSeek,
        kind: &ArrayKind,
        max_size: usize,
    ) -> Result<DataBlock> {
        let max_len = max_size / kind.element_size();
        Ok(match kind {
            ArrayKind::I8 => {
                let mut arr = vec![0i8; max_len];
                reader.read_i8_into(&mut arr)?;
                arr.into()
            }
            ArrayKind::U8 => {
                let mut arr = vec![0u8; max_len];
                reader.read_exact(&mut arr)?;
                arr.into()
            }
            ArrayKind::I16 => {
                let mut arr = vec![0i16; max_len];
                reader.read_i16_into::<LE>(&mut arr)?;
                arr.into()
            }
            ArrayKind::U16 => {
                let mut arr = vec![0u16; max_len];
                reader.read_u16_into::<LE>(&mut arr)?;
                arr.into()
            }
            ArrayKind::I32 => {
                let mut arr = vec![0i32; max_len];
                reader.read_i32_into::<LE>(&mut arr)?;
                arr.into()
            }
            ArrayKind::U32 => {
                let mut arr = vec![0u32; max_len];
                reader.read_u32_into::<LE>(&mut arr)?;
                arr.into()
            }
            ArrayKind::Ip(_) => {
                panic!("IP arrays must be read before other data");
            }
        })
    }

    /// Returns whether a command error is "known" and should be ignored.
    fn is_known_error(err: &command::Error, offset: u32) -> bool {
        // This is a workaround for stage06 (the bedroom) actually having invalid code in it. Around
        // this offset, we see these byte sequences:
        //
        //   00013cea 1b 18 46 01 00 00 17 00 00
        //   00013cf3 24 18 5e 01 00 00 17 00 00
        //   00013cfc 1b 18 2d 4e 00 00 17 00 00
        //
        // The bytes at 0x13cea read as Dir(Imm32(326), Imm16(0)). We know this is correct because
        // this command appears again in a nearby block. Therefore a command must start after it at
        // 0x13cf3. Going value-by-value:
        //
        //   00013cf3 Opcode: 24 -> CMD_POS
        //   00013cf4 PosArgs.obj: 18 5e 01 00 00 -> Imm32(350)
        //   00013cf9 PosArgs.x: 17 00 00 -> Imm16(0)
        //   00013cfc PosArgs.y: 1b 18 -> ParentStack(24)
        //   00013cfe PosArgs.z: 2d -> Invalid expression opcode (45)
        //
        // Things start to look obviously wrong at 0x13cfc because ParentStack() doesn't make any
        // sense here. Judging by context, it seems like the opcode at 0x13cf3 should have been
        // CMD_DIR (0x1b), because then the code reads correctly and also makes sense. Perhaps the
        // developers made a copy-and-paste error which their compiler didn't detect, and this block
        // just never happens to run? Experimentally, forcing the game to execute this block results
        // in a crash.
        //
        // Regardless, it isn't our job to fix the game, but it would also be silly to reject the
        // entire file for this. Returning true here produces an Abort command so we can keep
        // reading the other blocks in the file.
        if offset == 0x13cf3 {
            if let command::Error::Expr(err) = err {
                return matches!(**err, expr::Error::UnrecognizedOp(45));
            }
        }
        false
    }
}
