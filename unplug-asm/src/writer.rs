use crate::label::{LabelId, LabelMap};
use crate::opcodes::{AsmMsgOp, DirOp, NamedOpcode};
use crate::program::{
    Block, BlockContent, BlockFlags, CodeOperation, EntryPoint, Located, Operand, Operation,
    Program, Target,
};
use crate::span::Span;
use crate::Result;
use smallvec::SmallVec;
use std::collections::HashMap;
use std::io::{self, Write};
use unplug::common::Text;
use unplug::data::Resource;
use unplug::event::opcodes::{CmdOp, ExprOp, MsgOp, TypeOp};
use unplug::event::script::{BlockOffsetMap, ScriptLayout};
use unplug::event::serialize::{EventSerializer, Result as SerResult, SerializeEvent};
use unplug::event::{self, BlockId, DataBlock, Pointer, Script};
use unplug::globals::Libs;
use unplug::stage::{Event, Stage};

/// The optimal tab size for viewing a script, used to determine if a name is too long to fit in
/// the opcode column. The VSCode extension sets the tab size to 8.
const TAB_SIZE: usize = 8;

/// Vertical tab character (`\v`).
const VT: char = '\x0b';

/// Holds assembly code produced by `AsmSerializer`.
#[derive(Debug, Default, Clone)]
struct SerializedAsm {
    /// The content of the serialized data.
    content: Option<BlockContent>,
    /// Block IDs that the commands reference. This may contain duplicates.
    refs: Vec<BlockId>,
}

impl SerializedAsm {
    fn new() -> Self {
        Self::default()
    }

    /// Returns the inner code. ***Panics*** if this is not a code block.
    fn into_code(self) -> Vec<Operation<CmdOp>> {
        match self.content {
            None => vec![],
            Some(BlockContent::Code(code)) => code,
            Some(BlockContent::Data(_)) => panic!("Unexpected data"),
        }
    }

    /// Returns the inner data. ***Panics*** if this is not a data block.
    fn into_data(self) -> Vec<Located<Operand>> {
        match self.content {
            None => vec![],
            Some(BlockContent::Code(_)) => panic!("Unexpected code"),
            Some(BlockContent::Data(data)) => data,
        }
    }
}

/// `EventSerializer` implementation which transforms event data into assembly instructions.
struct AsmSerializer<'a> {
    /// The script which commands are part of.
    script: &'a Script,
    /// The global label map.
    labels: &'a mut LabelMap,
    /// The resulting assembly code.
    asm: SerializedAsm,
    /// The operation to add new operands to.
    operation: Option<CodeOperation>,
    /// The stack to save the current operation to when beginning a sub-operation.
    stack: Vec<CodeOperation>,
}

impl<'a> AsmSerializer<'a> {
    fn new(script: &'a Script, labels: &'a mut LabelMap) -> Self {
        Self { script, labels, asm: SerializedAsm::new(), operation: None, stack: vec![] }
    }

    /// Consumes this serializer, returning the built assembly code.
    fn finish(self) -> SerializedAsm {
        match self.operation {
            Some(CodeOperation::Command(_)) => panic!("unterminated command"),
            Some(CodeOperation::Expr(_)) => panic!("unterminated expression"),
            Some(CodeOperation::MsgCommand(_)) => panic!("unterminated message command"),
            None => (),
        }
        assert!(self.stack.is_empty());
        self.asm
    }

    /// Pushes a new operand onto the current operation.
    fn push_operand(&mut self, operand: Operand) {
        if let Some(op) = &mut self.operation {
            op.push_operand(operand.into());
        } else {
            match self.asm.content.get_or_insert(BlockContent::Data(vec![])) {
                BlockContent::Data(data) => data.push(operand.into()),
                _ => panic!("Unexpected operand: {:?}", operand),
            }
        }
    }

    /// Converts `ptr` into an operand which references it and declares a new label if necessary.
    fn make_reference(&mut self, ptr: Pointer) -> Operand {
        if ptr.is_in_header() {
            Operand::Offset(ptr.offset().unwrap())
        } else {
            let block = self.script.resolve_pointer(ptr).unwrap();
            self.asm.refs.push(block);

            let label = match self.labels.find_block(block).first() {
                Some(&label) => label,
                None => self
                    .labels
                    .insert_new(format!("loc_{}", block.index()), block, Span::EMPTY)
                    .unwrap(),
            };

            // If this command is if-like, use an ElseLabel so that "else" is displayed before it.
            // This should improve readability because it clarifies what the reference is for.
            match &self.operation {
                Some(CodeOperation::Command(c)) if c.opcode.is_if() => Operand::ElseLabel(label),
                _ => Operand::Label(label),
            }
        }
    }

    /// Makes `op` the current operation, saving the old operation onto the stack.
    fn begin_operation(&mut self, op: impl Into<CodeOperation>) {
        if let Some(prev) = self.operation.take() {
            self.stack.push(prev);
        }
        self.operation = Some(op.into());
    }

    /// Returns the current operation and pops the parent operation.
    fn end_operation(&mut self) -> CodeOperation {
        let result = self.operation.take().unwrap();
        if let Some(prev) = self.stack.pop() {
            self.operation = Some(prev);
        }
        result
    }

    /// Serializes an array of data values.
    fn serialize_array<T: Copy + Into<Operand>>(&mut self, arr: &[T]) {
        arr.iter().for_each(|&x| self.push_operand(x.into()));
    }

    /// Postprocess a command.
    fn postprocess_command(cmd: &mut Operation<CmdOp>) {
        if *cmd.opcode == CmdOp::Set {
            // set() puts the destination second; reverse the operands
            cmd.operands.reverse();
        }
    }

    /// Postprocess an expression, turning it into an operand.
    fn postprocess_expr(mut expr: Operation<ExprOp>) -> Operand {
        match *expr.opcode {
            // We have enough information to determine these from the operand
            ExprOp::Imm16 | ExprOp::Imm32 | ExprOp::AddressOf => {
                Located::into_inner(expr.operands.remove(0))
            }
            // Reverse binary operation order
            ExprOp::Equal
            | ExprOp::NotEqual
            | ExprOp::Less
            | ExprOp::LessEqual
            | ExprOp::Greater
            | ExprOp::GreaterEqual
            | ExprOp::Add
            | ExprOp::Subtract
            | ExprOp::Multiply
            | ExprOp::Divide
            | ExprOp::Modulo
            | ExprOp::BitAnd
            | ExprOp::BitOr
            | ExprOp::BitXor
            | ExprOp::AddAssign
            | ExprOp::SubtractAssign
            | ExprOp::MultiplyAssign
            | ExprOp::DivideAssign
            | ExprOp::ModuloAssign
            | ExprOp::BitAndAssign
            | ExprOp::BitOrAssign
            | ExprOp::BitXorAssign => {
                expr.operands.reverse();
                Operand::Expr(expr.into())
            }
            _ => Operand::Expr(expr.into()),
        }
    }
}

impl EventSerializer for AsmSerializer<'_> {
    fn serialize_i8(&mut self, val: i8) -> SerResult<()> {
        self.push_operand(Operand::I8(val));
        Ok(())
    }

    fn serialize_u8(&mut self, val: u8) -> SerResult<()> {
        self.push_operand(Operand::U8(val));
        Ok(())
    }

    fn serialize_i16(&mut self, val: i16) -> SerResult<()> {
        self.push_operand(Operand::I16(val));
        Ok(())
    }

    fn serialize_u16(&mut self, val: u16) -> SerResult<()> {
        self.push_operand(Operand::U16(val));
        Ok(())
    }

    fn serialize_i32(&mut self, val: i32) -> SerResult<()> {
        self.push_operand(Operand::I32(val));
        Ok(())
    }

    fn serialize_u32(&mut self, val: u32) -> SerResult<()> {
        self.push_operand(Operand::U32(val));
        Ok(())
    }

    fn serialize_pointer(&mut self, ptr: Pointer) -> SerResult<()> {
        let reference = self.make_reference(ptr);
        self.push_operand(reference);
        Ok(())
    }

    fn serialize_i8_array(&mut self, arr: &[i8]) -> SerResult<()> {
        self.serialize_array(arr);
        Ok(())
    }

    fn serialize_u8_array(&mut self, arr: &[u8]) -> SerResult<()> {
        self.serialize_array(arr);
        Ok(())
    }

    fn serialize_i16_array(&mut self, arr: &[i16]) -> SerResult<()> {
        self.serialize_array(arr);
        Ok(())
    }

    fn serialize_u16_array(&mut self, arr: &[u16]) -> SerResult<()> {
        self.serialize_array(arr);
        Ok(())
    }

    fn serialize_i32_array(&mut self, arr: &[i32]) -> SerResult<()> {
        self.serialize_array(arr);
        Ok(())
    }

    fn serialize_u32_array(&mut self, arr: &[u32]) -> SerResult<()> {
        self.serialize_array(arr);
        Ok(())
    }

    fn serialize_pointer_array(&mut self, arr: &[Pointer]) -> SerResult<()> {
        for &ptr in arr {
            let reference = self.make_reference(ptr);
            self.push_operand(reference);
        }
        Ok(())
    }

    fn serialize_type(&mut self, ty: TypeOp) -> SerResult<()> {
        self.push_operand(Operand::Type(ty));
        Ok(())
    }

    fn serialize_text(&mut self, text: &Text) -> SerResult<()> {
        self.push_operand(Operand::Text(text.clone()));
        Ok(())
    }

    fn serialize_rgba(&mut self, rgba: u32) -> SerResult<()> {
        self.serialize_u32(rgba)
    }

    fn begin_expr(&mut self, expr: ExprOp) -> SerResult<()> {
        self.begin_operation(Operation::new(expr.into()));
        Ok(())
    }

    fn end_expr(&mut self) -> SerResult<()> {
        let expr = self.end_operation().into_expr();
        self.push_operand(Self::postprocess_expr(expr));
        Ok(())
    }

    fn begin_command(&mut self, command: CmdOp) -> SerResult<()> {
        self.begin_operation(Operation::new(command.into()));
        Ok(())
    }

    fn end_command(&mut self) -> SerResult<()> {
        let mut command = self.end_operation().into_command();
        Self::postprocess_command(&mut command);
        match self.asm.content.get_or_insert(BlockContent::Code(vec![])) {
            BlockContent::Code(code) => code.push(command),
            _ => panic!("Unexpected command"),
        }
        Ok(())
    }

    fn begin_variadic_args(&mut self, _count: usize) -> SerResult<()> {
        Ok(())
    }

    fn end_variadic_args(&mut self) -> SerResult<()> {
        if let Some(CodeOperation::MsgCommand(_)) = &self.operation {
            let op = self.end_operation().into_msg_command();
            self.push_operand(Operand::MsgCommand(op.into()));
        }
        Ok(())
    }

    fn serialize_msg_char(&mut self, mut ch: MsgOp) -> SerResult<()> {
        // For readability purposes, consider newlines to be part of text operands rather than being
        // separate commands.
        ch = match ch {
            MsgOp::Newline => MsgOp::Char(b'\n'),
            MsgOp::NewlineVt => MsgOp::Char(11),
            _ => ch,
        };

        if let Some(CodeOperation::MsgCommand(cmd)) = &mut self.operation {
            let op = *cmd.opcode;
            if op == AsmMsgOp::Text || op == AsmMsgOp::Format {
                // Coalesce characters into newline-terminated text strings.
                if let MsgOp::Char(b) = ch {
                    if let Operand::Text(text) = &mut *cmd.operands[0] {
                        let last = text.as_bytes().last().copied().unwrap_or(0);
                        if last != b'\n' && last != VT as u8 {
                            text.push(b);
                            return Ok(());
                        }
                    } else {
                        panic!("text command does not have a text operand");
                    }
                }
            }
            let finished = self.end_operation().into_msg_command();
            self.push_operand(Operand::MsgCommand(finished.into()));

            // Format text is surrounded by format opcodes. Hide the second one because we push the
            // text onto the first one as an operand.
            if op == AsmMsgOp::Format && ch == MsgOp::Format {
                return Ok(());
            }
        }

        let cmd = match ch {
            MsgOp::End => return Ok(()),
            MsgOp::Speed => AsmMsgOp::Speed,
            MsgOp::Wait => AsmMsgOp::Wait,
            MsgOp::Anim => AsmMsgOp::Anim,
            MsgOp::Sfx => AsmMsgOp::Sfx,
            MsgOp::Voice => AsmMsgOp::Voice,
            MsgOp::Default => AsmMsgOp::Default,
            MsgOp::Format => AsmMsgOp::Format,
            MsgOp::Size => AsmMsgOp::Size,
            MsgOp::Color => AsmMsgOp::Color,
            MsgOp::Rgba => AsmMsgOp::Rgba,
            MsgOp::Proportional => AsmMsgOp::Proportional,
            MsgOp::Icon => AsmMsgOp::Icon,
            MsgOp::Shake => AsmMsgOp::Shake,
            MsgOp::Center => AsmMsgOp::Center,
            MsgOp::Rotate => AsmMsgOp::Rotate,
            MsgOp::Scale => AsmMsgOp::Scale,
            MsgOp::NumInput => AsmMsgOp::NumInput,
            MsgOp::Question => AsmMsgOp::Question,
            MsgOp::Stay => AsmMsgOp::Stay,
            MsgOp::Char(_) | MsgOp::Newline | MsgOp::NewlineVt => AsmMsgOp::Text,
            MsgOp::Invalid => AsmMsgOp::Invalid,
        };
        self.begin_operation(Operation::new(cmd.into()));
        match ch {
            MsgOp::Char(b) => self.push_operand(Operand::Text(Text::with_bytes(vec![b]))),
            MsgOp::Format => self.push_operand(Operand::Text(Text::new())),
            _ => (),
        }
        Ok(())
    }
}

/// Builds up a program from script data.
pub struct ProgramBuilder<'a> {
    script: &'a Script,
    stage: Option<&'a Stage>,
    program: Program,
    queue: Vec<BlockId>,
}

impl<'a> ProgramBuilder<'a> {
    pub fn new(target: Option<Target>, script: &'a Script) -> Self {
        Self::new_impl(target, script, None)
    }

    pub fn with_stage(name: impl Into<String>, stage: &'a Stage) -> Self {
        Self::new_impl(Some(Target::Stage(name.into())), &stage.script, Some(stage))
    }

    fn new_impl(target: Option<Target>, script: &'a Script, stage: Option<&'a Stage>) -> Self {
        let blocks = (0..script.len()).map(|_| Block::new()).collect::<Vec<_>>();
        let mut program = Program::with_blocks(blocks, None);
        program.target = target.map(Located::new);
        Self { script, stage, program, queue: vec![] }
    }

    /// Adds a script entry point of type `kind` beginning at `block_id` to the program.
    pub fn add_entry_point(&mut self, kind: EntryPoint, block_id: BlockId) -> Result<()> {
        self.add_subroutine(block_id)?;
        self.program.entry_points.insert(kind, Located::new(block_id));
        let block = block_id.get_mut(&mut self.program.blocks);
        if !block.flags.contains(BlockFlags::ENTRY_POINT) {
            block.flags.insert(BlockFlags::ENTRY_POINT);
            let name = match kind {
                EntryPoint::Lib(index) => format!("lib_{}", index),
                EntryPoint::Event(Event::Prologue) => "evt_prologue".to_owned(),
                EntryPoint::Event(Event::Startup) => "evt_startup".to_owned(),
                EntryPoint::Event(Event::Dead) => "evt_dead".to_owned(),
                EntryPoint::Event(Event::Pose) => "evt_pose".to_owned(),
                EntryPoint::Event(Event::TimeCycle) => "evt_time_cycle".to_owned(),
                EntryPoint::Event(Event::TimeUp) => "evt_time_up".to_owned(),
                EntryPoint::Event(Event::Interact(id)) => {
                    if let Some(stage) = self.stage {
                        let object = stage.object(id).unwrap().id.name();
                        format!("evt_{}_{}", object, id)
                    } else {
                        format!("evt_interact_{}", id)
                    }
                }
            };
            match self.program.labels.find_block(block_id).first() {
                Some(&label) => self.program.labels.rename(label, name)?,
                None => self.program.labels.insert_new(name, block_id, Span::EMPTY)?,
            };
        }
        Ok(())
    }

    /// Finishes building the program, consumes this builder, and returns the built program.
    pub fn finish(mut self) -> Program {
        if let Some(layout) = self.script.layout() {
            self.sort_blocks(layout);
        } else if !self.program.blocks.is_empty() {
            // Blocks are already in program order
            self.program.first_block = Some(BlockId::new(0));
            for (i, block) in self.program.blocks.iter_mut().enumerate().rev().skip(1) {
                block.next = Some(BlockId::new(i as u32 + 1));
            }
        }
        self.program.mark_subroutines();
        self.program
    }

    /// Adds the subroutine at `entry_point` to the program.
    fn add_subroutine(&mut self, entry_point: BlockId) -> Result<()> {
        if !entry_point.get(&self.program.blocks).is_empty() {
            return Ok(());
        }

        let order = self.script.reverse_postorder(entry_point);
        for &block in &order {
            self.add_code(block)?;
        }

        let name = format!("sub_{}", entry_point.index());
        match self.program.labels.find_block(entry_point).first() {
            Some(&label) => self.program.labels.rename(label, name)?,
            None => self.program.labels.insert_new(name, entry_point, Span::EMPTY)?,
        };

        while let Some(block) = self.queue.pop() {
            match self.script.block(block) {
                event::Block::Placeholder => panic!("Block {:?} is a placeholder", block),
                event::Block::Code(_) => self.add_subroutine(block)?,
                event::Block::Data(data) => self.add_data(block, data)?,
            };
        }
        Ok(())
    }

    /// Adds the data block at `block_id` to the program.
    fn add_data(&mut self, block_id: BlockId, data: &DataBlock) -> Result<()> {
        let block = block_id.get_mut(&mut self.program.blocks);
        if !block.is_empty() {
            return Ok(());
        }

        let mut ser = AsmSerializer::new(self.script, &mut self.program.labels);
        data.serialize(&mut ser)?;
        let serialized = ser.finish();

        self.queue.extend_from_slice(&serialized.refs);
        block.content = Some(BlockContent::Data(serialized.into_data()));
        Ok(())
    }

    /// Adds the code block at `block_id` to the program.
    fn add_code(&mut self, block_id: BlockId) -> Result<()> {
        let block = block_id.get_mut(&mut self.program.blocks);
        if !block.is_empty() {
            return Ok(());
        }

        // This is modeled after ScriptWriter
        let mut ser = AsmSerializer::new(self.script, &mut self.program.labels);
        let code = self.script.block(block_id).code().expect("Expected a code block");
        for command in &code.commands {
            command.serialize(&mut ser)?;
        }
        let serialized = ser.finish();

        self.queue.extend_from_slice(&serialized.refs);
        block.content = Some(BlockContent::Code(serialized.into_code()));

        // If execution can flow directly out of this block into another one, it MUST be written next
        if code.commands.is_empty() || !code.commands.last().unwrap().is_goto() {
            if let Some(Pointer::Block(next)) = code.next_block {
                assert!(next.get(&self.program.blocks).is_empty());
                self.add_code(next)?;
            }
        }
        Ok(())
    }

    /// Sorts blocks by offset according to `layout`.
    fn sort_blocks(&mut self, layout: &ScriptLayout) {
        if self.program.blocks.is_empty() {
            return;
        }

        let mut src_offsets = BlockOffsetMap::new(self.script.len());
        for &loc in layout.block_offsets() {
            src_offsets.insert(loc.id, loc.offset);
        }

        for (i, block) in self.program.blocks.iter_mut().enumerate() {
            if let Some(offset) = src_offsets.try_get(BlockId::new(i as u32)) {
                block.offset = offset;
            }
        }

        let mut order =
            (0..(self.program.blocks.len() as u32)).map(BlockId::new).collect::<Vec<_>>();
        order.sort_by_key(|&id| id.get(&self.program.blocks).offset);
        self.program.first_block = Some(order[0]);
        for pair in order.windows(2) {
            pair[0].get_mut(&mut self.program.blocks).next = Some(pair[1]);
        }
    }
}

/// Gets the directive corresponding to a data operand.
fn operand_type(op: &Operand) -> DirOp {
    match op {
        Operand::I8(_) | Operand::U8(_) | Operand::Text(_) => DirOp::Byte,
        Operand::I16(_) | Operand::U16(_) => DirOp::Word,
        Operand::I32(_) | Operand::U32(_) | Operand::Type(_) => DirOp::Dword,
        Operand::Label(_) | Operand::Offset(_) => DirOp::Dword,
        Operand::ElseLabel(_) | Operand::Expr(_) | Operand::MsgCommand(_) | Operand::Error => {
            panic!("Invalid data operand: {:?}", op);
        }
    }
}

type EntryPointVec = SmallVec<[EntryPoint; 1]>;

/// Writes out a program as human-readable assembly code.
struct ProgramWriter<'a, W: Write> {
    writer: W,
    program: &'a Program,
    block_entries: HashMap<BlockId, EntryPointVec>,
}

impl<'a, W: Write> ProgramWriter<'a, W> {
    fn new(writer: W, program: &'a Program) -> Self {
        // Reverse the program's event map
        let mut block_entries = HashMap::<BlockId, EntryPointVec>::new();
        for (&kind, &block) in &program.entry_points {
            block_entries.entry(*block).or_default().push(kind);
        }
        Self { writer, program, block_entries }
    }

    fn write(mut self) -> io::Result<()> {
        self.write_target()?;
        let mut current = self.program.first_block;
        let mut first = true;
        while let Some(id) = current {
            let block = id.get(&self.program.blocks);
            if !first && (block.flags.contains(BlockFlags::SUBROUTINE) || block.is_data()) {
                write!(self.writer, "\n\n")?;
            }
            self.write_block(id, block)?;
            current = block.next;
            first = false;
        }
        let _ = self.writer.flush();
        Ok(())
    }

    fn write_target(&mut self) -> io::Result<()> {
        let op = match self.program.target.as_deref() {
            Some(Target::Globals) => Operation::new(DirOp::Globals.into()),
            Some(Target::Stage(path)) => Operation::with_operands(
                DirOp::Stage.into(),
                [Located::new(Text::encode(path).unwrap().into())],
            ),
            None => return Ok(()),
        };
        self.write_directive(&op)?;
        write!(self.writer, "\n\n")?;
        Ok(())
    }

    fn write_block(&mut self, id: BlockId, block: &Block) -> io::Result<()> {
        let labels = self.program.labels.find_block(id);
        if !labels.is_empty() {
            if block.flags.contains(BlockFlags::ENTRY_POINT) {
                if let Some(mut entry_points) = self.block_entries.remove(&id) {
                    entry_points.sort_unstable();
                    for &kind in &entry_points {
                        self.write_entry_directive(kind, labels[0])?;
                    }
                }
            }
            for &label in labels {
                writeln!(self.writer, "{}:", self.program.labels.get(label).name)?;
            }
        }
        match &block.content {
            None => Ok(()),
            Some(BlockContent::Code(code)) => self.write_code(code),
            Some(BlockContent::Data(data)) => self.write_data(data),
        }
    }

    fn write_code(&mut self, code: &[Operation<CmdOp>]) -> io::Result<()> {
        code.iter().try_for_each(|c| self.write_command(c))
    }

    fn write_data(&mut self, data: &[Located<Operand>]) -> io::Result<()> {
        let mut current_op: Option<Operation<DirOp>> = None;
        for operand in data {
            let ty = operand_type(operand);
            let op = current_op.get_or_insert_with(|| Operation::new(ty.into()));
            if *op.opcode != ty {
                self.write_directive(op)?;
                op.opcode = ty.into();
                op.operands.clear();
            }
            op.operands.push(operand.clone());
        }
        if let Some(op) = current_op.take() {
            self.write_directive(&op)?;
        }
        Ok(())
    }

    fn write_command(&mut self, command: &Operation<CmdOp>) -> io::Result<()> {
        write!(self.writer, "\t{}", command.opcode.name())?;
        if !command.operands.is_empty() {
            write!(self.writer, "\t")?;
            if matches!(*command.opcode, CmdOp::Msg | CmdOp::Select) {
                self.write_msg_operands(&command.operands)?;
            } else {
                self.write_operands(&command.operands)?;
            }
        }
        writeln!(self.writer)?;
        Ok(())
    }

    fn write_entry_directive(&mut self, kind: EntryPoint, label: LabelId) -> io::Result<()> {
        let mut dir = Operation::new(kind.directive().into());
        match kind {
            EntryPoint::Lib(lib) => dir.operands.push(Operand::I16(lib).into()),
            EntryPoint::Event(Event::Interact(obj)) => dir.operands.push(Operand::I32(obj).into()),
            _ => (),
        }
        dir.operands.push(Operand::Label(label).into());
        self.write_directive(&dir)
    }

    fn write_directive(&mut self, dir: &Operation<DirOp>) -> io::Result<()> {
        let name = dir.opcode.name();
        write!(self.writer, "\t{}", name)?;
        if !dir.operands.is_empty() {
            if name.len() + 1 < TAB_SIZE {
                write!(self.writer, "\t")?;
            } else {
                write!(self.writer, "  ")?;
            }
            self.write_operands(&dir.operands)?;
        }
        writeln!(self.writer)?;
        Ok(())
    }

    fn write_operands(&mut self, operands: &[Located<Operand>]) -> io::Result<()> {
        self.write_operands_impl(operands, ", ")
    }

    fn write_msg_operands(&mut self, operands: &[Located<Operand>]) -> io::Result<()> {
        self.write_operands_impl(operands, ",\n\t\t")
    }

    fn write_operands_impl(
        &mut self,
        operands: &[Located<Operand>],
        separator: &str,
    ) -> io::Result<()> {
        for (i, operand) in operands.iter().enumerate() {
            if i > 0 {
                write!(self.writer, "{}", separator)?;
            }
            self.write_operand(operand)?;
        }
        Ok(())
    }

    fn write_operand(&mut self, operand: &Operand) -> io::Result<()> {
        match operand {
            Operand::I8(i) => write!(self.writer, "{}.b", i),
            Operand::U8(i) => write!(self.writer, "{}.b", i),
            Operand::I16(i) => write!(self.writer, "{}.w", i),
            Operand::U16(i) => write!(self.writer, "{}.w", i),
            Operand::I32(i) => write!(self.writer, "{}.d", i),
            Operand::U32(i) => write!(self.writer, "{}.d", i),
            Operand::Text(text) => {
                let decoded = text.decode().unwrap();
                let escaped = decoded.replace('\n', "\\n").replace(VT, "\\v");
                write!(self.writer, "\"{}\"", escaped)
            }
            Operand::Label(label) => {
                write!(self.writer, "*{}", self.program.labels.get(*label).name)
            }
            Operand::ElseLabel(label) => {
                write!(self.writer, "else *{}", self.program.labels.get(*label).name)
            }
            Operand::Offset(off) => write!(self.writer, "*0x{:x}", off),
            Operand::Type(ty) => write!(self.writer, "{}", ty.name()),
            Operand::Expr(expr) => self.write_expr(expr),
            Operand::MsgCommand(cmd) => self.write_msg_command(cmd),
            Operand::Error => write!(self.writer, "!"),
        }
    }

    fn write_expr(&mut self, expr: &Operation<ExprOp>) -> io::Result<()> {
        write!(self.writer, "{}", expr.opcode.name())?;
        if !expr.operands.is_empty() {
            write!(self.writer, "(")?;
            self.write_operands(&expr.operands)?;
            write!(self.writer, ")")?;
        }
        Ok(())
    }

    fn write_msg_command(&mut self, cmd: &Operation<AsmMsgOp>) -> io::Result<()> {
        match *cmd.opcode {
            AsmMsgOp::Text => self.write_operand(&cmd.operands[0])?,
            _ => {
                write!(self.writer, "{}", cmd.opcode.name())?;
                if !cmd.operands.is_empty() {
                    write!(self.writer, "(")?;
                    self.write_operands(&cmd.operands)?;
                    write!(self.writer, ")")?;
                }
            }
        }
        Ok(())
    }
}

/// Disassembles the script for `globals` into a `Program`.
pub fn disassemble_globals(globals: &Libs) -> Result<Program> {
    let mut builder = ProgramBuilder::new(Some(Target::Globals), &globals.script);
    for (i, &block) in globals.entry_points.iter().enumerate() {
        builder.add_entry_point(EntryPoint::Lib(i as i16), block)?;
    }
    Ok(builder.finish())
}

/// Disassembles the script for a `stage` named `name` into a `Program`.
pub fn disassemble_stage(stage: &Stage, name: &str) -> Result<Program> {
    let mut builder = ProgramBuilder::with_stage(name, stage);
    for (event, block) in stage.events() {
        builder.add_entry_point(EntryPoint::Event(event), block)?;
    }
    Ok(builder.finish())
}

/// Writes `program` as assembly program text to `writer`.
pub fn write_program(program: &Program, mut writer: impl Write) -> io::Result<()> {
    write_program_impl(program, &mut writer)
}

fn write_program_impl(program: &Program, writer: &mut dyn Write) -> io::Result<()> {
    ProgramWriter::new(writer, program).write()
}
