use crate::label::{LabelId, LabelMap};
use crate::opcodes::{AsmMsgOp, DirOp, NamedOpcode};
use crate::program::{AnyOperation, Block, BlockFlags, Instruction, Operand, Operation, Program};
use crate::Result;
use smallvec::SmallVec;
use std::collections::HashMap;
use std::io::Write;
use unplug::common::Text;
use unplug::data::Resource;
use unplug::event::opcodes::{CmdOp, ExprOp, MsgOp, TypeOp};
use unplug::event::script::{BlockOffsetMap, ScriptLayout};
use unplug::event::serialize::{
    Error as SerError, EventSerializer, Result as SerResult, SerializeEvent,
};
use unplug::event::{self, BlockId, DataBlock, Pointer, Script};
use unplug::stage::{Event, Stage};

/// The optimal tab size for viewing a script, used to determine if a name is too long to fit in
/// the opcode column. The VSCode extension sets the tab size to 8.
const TAB_SIZE: usize = 8;

/// Holds assembly code produced by `AsmSerializer`.
#[derive(Debug, Default, Clone)]
struct SerializedAsm {
    /// The assembly commands in order of their appearance in the source file.
    commands: Vec<Operation<CmdOp>>,
    /// Directives in order of their appearance in the source file.
    directives: Vec<Operation<DirOp>>,
    /// Block IDs that the commands reference. This may contain duplicates.
    refs: Vec<BlockId>,
}

impl SerializedAsm {
    fn new() -> Self {
        Self::default()
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
    operation: Option<AnyOperation>,
    /// The stack to save the current operation to when beginning a sub-operation.
    stack: Vec<AnyOperation>,
}

impl<'a> AsmSerializer<'a> {
    fn new(script: &'a Script, labels: &'a mut LabelMap) -> Self {
        Self { script, labels, asm: SerializedAsm::new(), operation: None, stack: vec![] }
    }

    /// Consumes this serializer, returning the built assembly code.
    fn finish(mut self) -> SerializedAsm {
        match self.operation {
            Some(AnyOperation::Directive(dir)) => self.asm.directives.push(dir),
            Some(AnyOperation::Command(_)) => panic!("unterminated command"),
            Some(AnyOperation::Expr(_)) => panic!("unterminated expression"),
            Some(AnyOperation::MsgCommand(_)) => panic!("unterminated message command"),
            None => (),
        }
        assert!(self.stack.is_empty());
        self.asm
    }

    /// Pushes a new operand onto the current operation.
    fn push_operand(&mut self, operand: Operand) {
        self.operation.as_mut().unwrap().push_operand(operand)
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
                None => {
                    self.labels.insert_new(format!("loc_{}", block.index()), Some(block)).unwrap()
                }
            };

            // If this command is if-like, use an ElseLabel so that "else" is displayed before it.
            // This should improve readability because it clarifies what the reference is for.
            match &self.operation {
                Some(AnyOperation::Command(c)) if c.opcode.is_if() => Operand::ElseLabel(label),
                _ => Operand::Label(label),
            }
        }
    }

    /// Makes `op` the current operation, saving the old operation onto the stack.
    fn begin_operation(&mut self, op: impl Into<AnyOperation>) {
        if let Some(prev) = self.operation.take() {
            self.stack.push(prev);
        }
        self.operation = Some(op.into());
    }

    /// Returns the current operation and pops the parent operation.
    fn end_operation(&mut self) -> AnyOperation {
        let result = self.operation.take().unwrap();
        if let Some(prev) = self.stack.pop() {
            self.operation = Some(prev);
        }
        result
    }

    /// If there is no active operation or the current directive does not match, makes `op` the
    /// current directive.
    fn begin_directive(&mut self, op: DirOp) {
        match &self.operation {
            Some(AnyOperation::Directive(dir)) if dir.opcode == op => return,
            Some(AnyOperation::Directive(_)) => self.end_directive(),
            Some(_) => return,
            None => (),
        }
        if self.operation.is_none() {
            self.operation = Some(Operation::new(op).into());
        }
    }

    /// Ends the current directive and appends it to the program.
    fn end_directive(&mut self) {
        let dir = self.end_operation().into_directive();
        self.asm.directives.push(dir);
    }

    /// Serializes an array of data values of type `op`.
    fn serialize_array<T: Copy + Into<Operand>>(&mut self, op: DirOp, arr: &[T]) -> SerResult<()> {
        self.begin_directive(op);
        if let Some(AnyOperation::Directive(dir)) = &mut self.operation {
            arr.iter().for_each(|&x| dir.operands.push(x.into()));
            Ok(())
        } else {
            Err(SerError::custom(format!("unexpected {:?} array", op)))
        }
    }
}

impl EventSerializer for AsmSerializer<'_> {
    fn serialize_i8(&mut self, val: i8) -> SerResult<()> {
        self.begin_directive(DirOp::Byte);
        self.push_operand(Operand::I8(val));
        Ok(())
    }

    fn serialize_u8(&mut self, val: u8) -> SerResult<()> {
        self.begin_directive(DirOp::Byte);
        self.push_operand(Operand::U8(val));
        Ok(())
    }

    fn serialize_i16(&mut self, val: i16) -> SerResult<()> {
        self.begin_directive(DirOp::Word);
        self.push_operand(Operand::I16(val));
        Ok(())
    }

    fn serialize_u16(&mut self, val: u16) -> SerResult<()> {
        self.begin_directive(DirOp::Word);
        self.push_operand(Operand::U16(val));
        Ok(())
    }

    fn serialize_i32(&mut self, val: i32) -> SerResult<()> {
        self.begin_directive(DirOp::Dword);
        self.push_operand(Operand::I32(val));
        Ok(())
    }

    fn serialize_u32(&mut self, val: u32) -> SerResult<()> {
        self.begin_directive(DirOp::Dword);
        self.push_operand(Operand::U32(val));
        Ok(())
    }

    fn serialize_pointer(&mut self, ptr: Pointer) -> SerResult<()> {
        self.begin_directive(DirOp::Dword);
        let reference = self.make_reference(ptr);
        self.push_operand(reference);
        Ok(())
    }

    fn serialize_i8_array(&mut self, arr: &[i8]) -> SerResult<()> {
        self.serialize_array(DirOp::Byte, arr)
    }

    fn serialize_u8_array(&mut self, arr: &[u8]) -> SerResult<()> {
        self.serialize_array(DirOp::Byte, arr)
    }

    fn serialize_i16_array(&mut self, arr: &[i16]) -> SerResult<()> {
        self.serialize_array(DirOp::Word, arr)
    }

    fn serialize_u16_array(&mut self, arr: &[u16]) -> SerResult<()> {
        self.serialize_array(DirOp::Word, arr)
    }

    fn serialize_i32_array(&mut self, arr: &[i32]) -> SerResult<()> {
        self.serialize_array(DirOp::Dword, arr)
    }

    fn serialize_u32_array(&mut self, arr: &[u32]) -> SerResult<()> {
        self.serialize_array(DirOp::Dword, arr)
    }

    fn serialize_pointer_array(&mut self, arr: &[Pointer]) -> SerResult<()> {
        self.begin_directive(DirOp::Dword);
        if let Some(AnyOperation::Directive(mut dir)) = self.operation.take() {
            arr.iter().for_each(|&ptr| dir.operands.push(self.make_reference(ptr)));
            self.operation = Some(AnyOperation::Directive(dir));
            Ok(())
        } else {
            Err(SerError::custom("unexpected pointer array"))
        }
    }

    fn serialize_type(&mut self, ty: TypeOp) -> SerResult<()> {
        self.begin_directive(DirOp::Dword);
        self.push_operand(Operand::Type(ty));
        Ok(())
    }

    fn serialize_text(&mut self, text: &Text) -> SerResult<()> {
        self.begin_directive(DirOp::Byte);
        self.push_operand(Operand::Text(text.clone()));
        Ok(())
    }

    fn serialize_rgba(&mut self, rgba: u32) -> SerResult<()> {
        self.begin_directive(DirOp::Dword);
        self.serialize_u32(rgba)
    }

    fn begin_expr(&mut self, expr: ExprOp) -> SerResult<()> {
        self.begin_operation(Operation::new(expr));
        Ok(())
    }

    fn end_expr(&mut self) -> SerResult<()> {
        let expr = self.end_operation().into_expr();
        self.push_operand(Operand::Expr(expr.into()));
        Ok(())
    }

    fn begin_command(&mut self, command: CmdOp) -> SerResult<()> {
        self.begin_operation(Operation::new(command));
        Ok(())
    }

    fn end_command(&mut self) -> SerResult<()> {
        let command = self.end_operation().into_command();
        self.asm.commands.push(command);
        Ok(())
    }

    fn begin_call(&mut self) -> SerResult<()> {
        Ok(())
    }

    fn end_call(&mut self) -> SerResult<()> {
        Ok(())
    }

    fn begin_msg(&mut self) -> SerResult<()> {
        Ok(())
    }

    fn serialize_msg_char(&mut self, ch: MsgOp) -> SerResult<()> {
        if let Some(AnyOperation::MsgCommand(cmd)) = &mut self.operation {
            if cmd.opcode == AsmMsgOp::Text {
                // Coalesce characters into text strings.
                if let MsgOp::Char(b) = ch {
                    match &mut cmd.operands[0] {
                        Operand::Text(text) => text.push(b),
                        _ => panic!("text command does not have a text operand"),
                    }
                    return Ok(());
                }
            }
            let op = self.end_operation().into_msg_command();
            self.push_operand(Operand::MsgCommand(op.into()));
        }
        let cmd = match ch {
            MsgOp::End => AsmMsgOp::End,
            MsgOp::Speed => AsmMsgOp::Speed,
            MsgOp::Wait => AsmMsgOp::Wait,
            MsgOp::Anim => AsmMsgOp::Anim,
            MsgOp::Sfx => AsmMsgOp::Sfx,
            MsgOp::Voice => AsmMsgOp::Voice,
            MsgOp::Default => AsmMsgOp::Default,
            MsgOp::Newline => AsmMsgOp::Newline,
            MsgOp::NewlineVt => AsmMsgOp::NewlineVt,
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
            MsgOp::Char(_) => AsmMsgOp::Text,
        };
        self.begin_operation(Operation::new(cmd));
        if let MsgOp::Char(b) = ch {
            self.push_operand(Operand::Text(Text::with_bytes(vec![b])));
        }
        Ok(())
    }

    fn end_msg(&mut self) -> SerResult<()> {
        if let Some(AnyOperation::MsgCommand(_)) = &self.operation {
            let op = self.end_operation().into_msg_command();
            self.push_operand(Operand::MsgCommand(op.into()));
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
    pub fn new(script: &'a Script) -> Self {
        Self::new_impl(script, None)
    }

    pub fn with_stage(stage: &'a Stage) -> Self {
        Self::new_impl(&stage.script, Some(stage))
    }

    fn new_impl(script: &'a Script, stage: Option<&'a Stage>) -> Self {
        let blocks: Vec<_> =
            (0..script.len()).map(|id| Block::new(BlockId::new(id as u32))).collect();
        Self { script, stage, program: Program::with_blocks(blocks), queue: vec![] }
    }

    /// Adds a script event of type `event` beginning at `block_id` to the program.
    pub fn add_event(&mut self, event: Event, block_id: BlockId) -> Result<()> {
        self.add_subroutine(block_id)?;
        self.program.events.insert(event, block_id);
        let block = block_id.get_mut(&mut self.program.blocks);
        if !block.flags.contains(BlockFlags::EVENT) {
            block.flags.insert(BlockFlags::EVENT);
            let name = match event {
                Event::Prologue => "evt_prologue".to_owned(),
                Event::Startup => "evt_startup".to_owned(),
                Event::Dead => "evt_dead".to_owned(),
                Event::Pose => "evt_pose".to_owned(),
                Event::TimeCycle => "evt_time_cycle".to_owned(),
                Event::TimeUp => "evt_time_up".to_owned(),
                Event::Interact(id) => {
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
                None => self.program.labels.insert_new(name, Some(block_id))?,
            };
        }
        Ok(())
    }

    /// Finishes building the program, consumes this builder, and returns the built program.
    pub fn finish(mut self) -> Program {
        if let Some(layout) = self.script.layout() {
            self.sort_blocks(layout);
        }
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
        entry_point.get_mut(&mut self.program.blocks).flags.insert(BlockFlags::SUBROUTINE);

        let name = format!("sub_{}", entry_point.index());
        match self.program.labels.find_block(entry_point).first() {
            Some(&label) => self.program.labels.rename(label, name)?,
            None => self.program.labels.insert_new(name, Some(entry_point))?,
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

        assert!(serialized.commands.is_empty());
        block.extend(serialized.directives);
        block.flags.insert(BlockFlags::DATA);
        self.queue.extend(serialized.refs);
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

        assert!(serialized.directives.is_empty());
        block.extend(serialized.commands);
        self.queue.extend(serialized.refs);

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
        let mut src_offsets = BlockOffsetMap::new(self.script.len());
        for &loc in layout.block_offsets() {
            src_offsets.insert(loc.id, loc.offset);
        }
        for block in &mut self.program.blocks {
            if let Some(offset) = src_offsets.try_get(block.id) {
                block.offset = offset;
            }
        }
        self.program.blocks.sort_by_key(|b| b.offset);
    }
}

type EventVec = SmallVec<[Event; 1]>;

/// Writes out a program as human-readable assembly code.
pub struct ProgramWriter<'a, W: Write> {
    writer: W,
    program: &'a Program,
    block_events: HashMap<BlockId, EventVec>,
}

impl<'a, W: Write> ProgramWriter<'a, W> {
    pub fn new(writer: W, program: &'a Program) -> Self {
        // Reverse the program's event map
        let mut block_events: HashMap<BlockId, EventVec> = HashMap::new();
        for (&event, &block) in &program.events {
            block_events.entry(block).or_default().push(event);
        }
        Self { writer, program, block_events }
    }

    pub fn write(mut self) -> Result<()> {
        for (i, block) in self.program.blocks.iter().enumerate() {
            if i > 0 && block.flags.intersects(BlockFlags::SUBROUTINE | BlockFlags::DATA) {
                write!(self.writer, "\n\n")?;
            }
            self.write_block(block)?;
        }
        let _ = self.writer.flush();
        Ok(())
    }

    fn write_block(&mut self, block: &Block) -> Result<()> {
        let labels = self.program.labels.find_block(block.id);
        if !labels.is_empty() {
            if block.flags.contains(BlockFlags::EVENT) {
                if let Some(mut events) = self.block_events.remove(&block.id) {
                    events.sort_unstable();
                    for &event in &events {
                        self.write_event_directive(event, labels[0])?;
                    }
                }
            }
            for &label in labels {
                writeln!(self.writer, "{}:", self.program.labels.get(label).name)?;
            }
        }
        for instruction in &block.instructions {
            match instruction {
                Instruction::Command(cmd) => self.write_command(cmd)?,
                Instruction::Directive(dir) => self.write_directive(dir)?,
            }
        }
        Ok(())
    }

    fn write_command(&mut self, command: &Operation<CmdOp>) -> Result<()> {
        write!(self.writer, "\t{}", command.opcode.name())?;
        if !command.operands.is_empty() {
            write!(self.writer, "\t")?;
            match command.opcode {
                CmdOp::Set => self.write_operands(command.operands.iter().rev())?,
                _ => self.write_operands(&command.operands)?,
            }
        }
        writeln!(self.writer)?;
        Ok(())
    }

    fn write_event_directive(&mut self, event: Event, label: LabelId) -> Result<()> {
        let mut dir = Operation::new(DirOp::for_event(event));
        if let Event::Interact(obj) = event {
            dir.operands.push(Operand::I32(obj));
        }
        dir.operands.push(Operand::Label(label));
        self.write_directive(&dir)
    }

    fn write_directive(&mut self, dir: &Operation<DirOp>) -> Result<()> {
        let name = dir.opcode.name();
        write!(self.writer, "\t.{}", name)?;
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

    fn write_operands<'o>(
        &mut self,
        operands: impl IntoIterator<Item = &'o Operand>,
    ) -> Result<()> {
        let mut wrap = false;
        for (i, operand) in operands.into_iter().enumerate() {
            if i > 0 {
                if wrap {
                    write!(self.writer, ",\n\t\t")?;
                } else {
                    write!(self.writer, ", ")?;
                }
            }
            self.write_operand(operand)?;
            wrap = matches!(operand, Operand::MsgCommand(_));
        }
        Ok(())
    }

    fn write_operand(&mut self, operand: &Operand) -> Result<()> {
        match operand {
            Operand::I8(i) => write!(self.writer, "{}.b", i)?,
            Operand::U8(i) => write!(self.writer, "{}.b", i)?,
            Operand::I16(i) => write!(self.writer, "{}.w", i)?,
            Operand::U16(i) => write!(self.writer, "{}.w", i)?,
            Operand::I32(i) => write!(self.writer, "{}.d", i)?,
            Operand::U32(i) => write!(self.writer, "{}.d", i)?,
            Operand::Text(text) => {
                let decoded = text.decode().unwrap();
                let escaped = decoded.replace('\n', "\\n");
                write!(self.writer, "\"{}\"", escaped)?
            }
            Operand::Label(label) => {
                write!(self.writer, "*{}", self.program.labels.get(*label).name)?
            }
            Operand::ElseLabel(label) => {
                write!(self.writer, "else *{}", self.program.labels.get(*label).name)?
            }
            Operand::Offset(off) => write!(self.writer, "*0x{:x}", off)?,
            Operand::Type(ty) => write!(self.writer, "@{}", ty.name())?,
            Operand::Expr(expr) => self.write_expr(expr)?,
            Operand::MsgCommand(cmd) => self.write_msg_command(cmd)?,
        }
        Ok(())
    }

    fn write_expr(&mut self, expr: &Operation<ExprOp>) -> Result<()> {
        let mut reverse = false;
        match expr.opcode {
            ExprOp::Imm16 | ExprOp::Imm32 | ExprOp::AddressOf => {
                return self.write_operand(&expr.operands[0])
            }
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
                reverse = true;
            }
            _ => (),
        }
        write!(self.writer, "{}", expr.opcode.name())?;
        if !expr.operands.is_empty() {
            write!(self.writer, "(")?;
            if reverse {
                self.write_operands(expr.operands.iter().rev())?;
            } else {
                self.write_operands(&expr.operands)?;
            }
            write!(self.writer, ")")?;
        }
        Ok(())
    }

    fn write_msg_command(&mut self, cmd: &Operation<AsmMsgOp>) -> Result<()> {
        match cmd.opcode {
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
