use crate::label::LabelMap;
use crate::opcodes::{AsmMsgOp, DataOp, NamedOpcode};
use crate::operand::{Operand, Operation};
use crate::Result;
use std::collections::HashSet;
use std::io::Write;
use unplug::common::Text;
use unplug::event::opcodes::{CmdOp, ExprOp, MsgOp, TypeOp};
use unplug::event::script::{BlockOffsetMap, ScriptLayout};
use unplug::event::serialize::{
    Error as SerError, EventSerializer, Result as SerResult, SerializeEvent,
};
use unplug::event::{Block, BlockId, DataBlock, Pointer, Script};

/// Encapsulates various operation types.
enum AnyOperation {
    Command(Operation<CmdOp>),
    Expr(Operation<ExprOp>),
    MsgCommand(Operation<AsmMsgOp>),
    Data(Operation<DataOp>),
}

impl AnyOperation {
    /// Appends `operand` onto the operation.
    fn push_operand(&mut self, operand: Operand) {
        match self {
            Self::Command(op) => op.operands.push(operand),
            Self::Expr(op) => op.operands.push(operand),
            Self::MsgCommand(op) => op.operands.push(operand),
            Self::Data(op) => op.operands.push(operand),
        }
    }

    /// Consumes this wrapper and returns the inner command.
    /// ***Panics*** if the operation is not a command.
    fn into_command(self) -> Operation<CmdOp> {
        match self {
            Self::Command(op) => op,
            _ => panic!("expected a command operation"),
        }
    }

    /// Consumes this wrapper and returns the inner expression.
    /// ***Panics*** if the operation is not an expression.
    fn into_expr(self) -> Operation<ExprOp> {
        match self {
            Self::Expr(op) => op,
            _ => panic!("expected an expr operation"),
        }
    }

    /// Consumes this wrapper and returns the inner message command.
    /// ***Panics*** if the operation is not a message command.
    fn into_msg_command(self) -> Operation<AsmMsgOp> {
        match self {
            Self::MsgCommand(op) => op,
            _ => panic!("expected a message command"),
        }
    }

    /// Consumes this wrapper and returns the inner data definition.
    /// ***Panics*** if the operation is not data.
    fn into_data(self) -> Operation<DataOp> {
        match self {
            Self::Data(op) => op,
            _ => panic!("expected data"),
        }
    }
}

impl From<Operation<CmdOp>> for AnyOperation {
    fn from(op: Operation<CmdOp>) -> Self {
        Self::Command(op)
    }
}

impl From<Operation<ExprOp>> for AnyOperation {
    fn from(op: Operation<ExprOp>) -> Self {
        Self::Expr(op)
    }
}

impl From<Operation<AsmMsgOp>> for AnyOperation {
    fn from(op: Operation<AsmMsgOp>) -> Self {
        Self::MsgCommand(op)
    }
}

impl From<Operation<DataOp>> for AnyOperation {
    fn from(op: Operation<DataOp>) -> Self {
        Self::Data(op)
    }
}

/// Holds assembly code produced by `AsmSerializer`.
#[derive(Debug, Default, Clone)]
struct SerializedAsm {
    /// The assembly commands in order of their appearance in the source file.
    commands: Vec<Operation<CmdOp>>,
    /// Data definitions in order of their appearance in the source file.
    data: Vec<Operation<DataOp>>,
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
            Some(AnyOperation::Data(data)) => self.asm.data.push(data),
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
            let label = self
                .labels
                .find_block_or_insert(block, || format!("loc_{}", block.index()))
                .unwrap();
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

    /// If there is no active operation or the current data operation does not match, makes `op` the
    /// current data operation.
    fn begin_data(&mut self, op: DataOp) {
        match &self.operation {
            Some(AnyOperation::Data(data)) if data.opcode == op => return,
            Some(AnyOperation::Data(_)) => self.end_data(),
            Some(_) => return,
            None => (),
        }
        if self.operation.is_none() {
            self.operation = Some(Operation::new(op).into());
        }
    }

    /// Ends the current data operation and appends it to the program.
    fn end_data(&mut self) {
        let data = self.end_operation().into_data();
        self.asm.data.push(data);
    }

    /// Serializes an array of data values of type `op`.
    fn serialize_array<T: Copy + Into<Operand>>(&mut self, op: DataOp, arr: &[T]) -> SerResult<()> {
        self.begin_data(op);
        if let Some(AnyOperation::Data(data)) = &mut self.operation {
            arr.iter().for_each(|&x| data.operands.push(x.into()));
            Ok(())
        } else {
            Err(SerError::custom(format!("unexpected {:?} array", op)))
        }
    }
}

impl EventSerializer for AsmSerializer<'_> {
    fn serialize_i8(&mut self, val: i8) -> SerResult<()> {
        self.begin_data(DataOp::Byte);
        self.push_operand(Operand::I8(val));
        Ok(())
    }

    fn serialize_u8(&mut self, val: u8) -> SerResult<()> {
        self.begin_data(DataOp::Byte);
        self.push_operand(Operand::U8(val));
        Ok(())
    }

    fn serialize_i16(&mut self, val: i16) -> SerResult<()> {
        self.begin_data(DataOp::Word);
        self.push_operand(Operand::I16(val));
        Ok(())
    }

    fn serialize_u16(&mut self, val: u16) -> SerResult<()> {
        self.begin_data(DataOp::Word);
        self.push_operand(Operand::U16(val));
        Ok(())
    }

    fn serialize_i32(&mut self, val: i32) -> SerResult<()> {
        self.begin_data(DataOp::Dword);
        self.push_operand(Operand::I32(val));
        Ok(())
    }

    fn serialize_u32(&mut self, val: u32) -> SerResult<()> {
        self.begin_data(DataOp::Dword);
        self.push_operand(Operand::U32(val));
        Ok(())
    }

    fn serialize_pointer(&mut self, ptr: Pointer) -> SerResult<()> {
        self.begin_data(DataOp::Dword);
        let reference = self.make_reference(ptr);
        self.push_operand(reference);
        Ok(())
    }

    fn serialize_i8_array(&mut self, arr: &[i8]) -> SerResult<()> {
        self.serialize_array(DataOp::Byte, arr)
    }

    fn serialize_u8_array(&mut self, arr: &[u8]) -> SerResult<()> {
        self.serialize_array(DataOp::Byte, arr)
    }

    fn serialize_i16_array(&mut self, arr: &[i16]) -> SerResult<()> {
        self.serialize_array(DataOp::Word, arr)
    }

    fn serialize_u16_array(&mut self, arr: &[u16]) -> SerResult<()> {
        self.serialize_array(DataOp::Word, arr)
    }

    fn serialize_i32_array(&mut self, arr: &[i32]) -> SerResult<()> {
        self.serialize_array(DataOp::Dword, arr)
    }

    fn serialize_u32_array(&mut self, arr: &[u32]) -> SerResult<()> {
        self.serialize_array(DataOp::Dword, arr)
    }

    fn serialize_pointer_array(&mut self, arr: &[Pointer]) -> SerResult<()> {
        self.begin_data(DataOp::Dword);
        if let Some(AnyOperation::Data(mut data)) = self.operation.take() {
            arr.iter().for_each(|&ptr| data.operands.push(self.make_reference(ptr)));
            self.operation = Some(AnyOperation::Data(data));
            Ok(())
        } else {
            Err(SerError::custom("unexpected pointer array"))
        }
    }

    fn serialize_type(&mut self, ty: TypeOp) -> SerResult<()> {
        self.begin_data(DataOp::Dword);
        self.push_operand(Operand::Type(ty));
        Ok(())
    }

    fn serialize_text(&mut self, text: &Text) -> SerResult<()> {
        self.begin_data(DataOp::Byte);
        self.push_operand(Operand::Text(text.clone()));
        Ok(())
    }

    fn serialize_rgba(&mut self, rgba: u32) -> SerResult<()> {
        self.begin_data(DataOp::Dword);
        self.serialize_u32(rgba)
    }

    fn begin_expr(&mut self, expr: ExprOp) -> SerResult<()> {
        self.begin_operation(Operation::new(expr));
        Ok(())
    }

    fn end_expr(&mut self) -> SerResult<()> {
        let expr = self.end_operation().into_expr();
        self.push_operand(Operand::Expr(expr));
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
            self.push_operand(Operand::MsgCommand(op));
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
            self.push_operand(Operand::MsgCommand(op));
        }
        Ok(())
    }
}

/// A block of instructions corresponding to a script block.
pub struct AsmCodeBlock {
    pub src_block: BlockId,
    pub commands: Vec<Operation<CmdOp>>,
}

/// A subroutine made up of multiple `AsmBlock`s.
pub struct Subroutine {
    pub src_block: BlockId,
    pub src_offset: u32,
    pub blocks: Vec<AsmCodeBlock>,
}

/// A block of data corresponding to a script block.
pub struct AsmDataBlock {
    pub src_block: BlockId,
    pub src_offset: u32,
    pub data: Vec<Operation<DataOp>>,
}

/// An assembly program consisting of subroutines and labels.
#[derive(Default)]
pub struct Program {
    pub subroutines: Vec<Subroutine>,
    pub data: Vec<AsmDataBlock>,
    pub labels: LabelMap,
}

impl Program {
    fn new() -> Self {
        Self::default()
    }
}

/// Builds up a `Program` from script data.
pub struct ProgramBuilder<'a> {
    script: &'a Script,
    program: Program,
    visited: HashSet<BlockId>,
    queue: Vec<BlockId>,
}

impl<'a> ProgramBuilder<'a> {
    pub fn new(script: &'a Script) -> Self {
        Self { script, program: Program::new(), visited: HashSet::new(), queue: vec![] }
    }

    /// Adds the script event beginning at `block_id` to the program.
    pub fn add_event(&mut self, block_id: BlockId) -> Result<()> {
        self.add_block(block_id)?;
        let name = format!("evt_{}", block_id.index());
        self.program.labels.rename_or_insert(block_id, name)?;
        Ok(())
    }

    /// Finishes building the program, consumes this builder, and returns the built program.
    pub fn finish(mut self) -> Program {
        if let Some(layout) = self.script.layout() {
            self.sort_blocks(layout);
        }
        self.program
    }

    /// Add the block at `block_id` to the program.
    fn add_block(&mut self, block_id: BlockId) -> Result<()> {
        if self.visited.contains(&block_id) {
            return Ok(());
        }
        match self.script.block(block_id) {
            Block::Placeholder => panic!("Block {:?} is a placeholder", block_id),
            Block::Code(_) => self.add_subroutine(block_id),
            Block::Data(data) => self.add_data(block_id, data),
        }
    }

    /// Add the subroutine at `entry_point` to the program.
    fn add_subroutine(&mut self, entry_point: BlockId) -> Result<()> {
        let name = format!("sub_{}", entry_point.index());
        self.program.labels.rename_or_insert(entry_point, name)?;

        let order = self.script.reverse_postorder(entry_point);
        let mut subroutine = Subroutine { src_block: entry_point, src_offset: 0, blocks: vec![] };
        for block in order {
            self.add_code(&mut subroutine, block)?;
        }

        self.program.subroutines.push(subroutine);
        while let Some(block) = self.queue.pop() {
            self.add_block(block)?;
        }
        Ok(())
    }

    /// Adds the data block at `block_id` to the program.
    fn add_data(&mut self, block_id: BlockId, data: &DataBlock) -> Result<()> {
        if !self.visited.insert(block_id) {
            return Ok(());
        }

        let mut ser = AsmSerializer::new(self.script, &mut self.program.labels);
        data.serialize(&mut ser)?;

        let serialized = ser.finish();
        let block = AsmDataBlock { src_block: block_id, src_offset: 0, data: serialized.data };
        self.program.data.push(block);
        self.queue.extend(serialized.refs);
        assert!(serialized.commands.is_empty());
        Ok(())
    }

    /// Adds the code block at `block_id` to `subroutine`.
    fn add_code(&mut self, subroutine: &mut Subroutine, block_id: BlockId) -> Result<()> {
        if !self.visited.insert(block_id) {
            return Ok(());
        }

        // This is modeled after ScriptWriter
        let mut ser = AsmSerializer::new(self.script, &mut self.program.labels);
        let code = self.script.block(block_id).code().expect("Expected a code block");
        for command in &code.commands {
            command.serialize(&mut ser)?;
        }

        let serialized = ser.finish();
        let block = AsmCodeBlock { src_block: block_id, commands: serialized.commands };
        subroutine.blocks.push(block);
        self.queue.extend(serialized.refs);
        assert!(serialized.data.is_empty());

        // If execution can flow directly out of this block into another one, it MUST be written next
        if code.commands.is_empty() || !code.commands.last().unwrap().is_goto() {
            if let Some(Pointer::Block(next)) = code.next_block {
                assert!(!self.visited.contains(&next));
                self.add_code(subroutine, next)?;
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
        for subroutine in &mut self.program.subroutines {
            if let Some(offset) = src_offsets.try_get(subroutine.src_block) {
                subroutine.src_offset = offset;
            }
        }
        for data in &mut self.program.data {
            if let Some(offset) = src_offsets.try_get(data.src_block) {
                data.src_offset = offset;
            }
        }
        self.program.subroutines.sort_by_key(|s| s.src_offset);
        self.program.data.sort_by_key(|s| s.src_offset);
    }
}

/// Writes out a program as human-readable assembly code.
pub struct ProgramWriter<'a, W: Write> {
    writer: W,
    program: &'a Program,
}

impl<'a, W: Write> ProgramWriter<'a, W> {
    pub fn new(writer: W, program: &'a Program) -> Self {
        Self { writer, program }
    }

    pub fn write(mut self) -> Result<()> {
        for (i, subroutine) in self.program.subroutines.iter().enumerate() {
            if i > 0 {
                write!(self.writer, "\n\n")?;
            }
            for block in &subroutine.blocks {
                self.write_code(block)?;
            }
        }
        writeln!(self.writer)?;
        for data in &self.program.data {
            writeln!(self.writer)?;
            self.write_data(data)?;
        }
        let _ = self.writer.flush();
        Ok(())
    }

    fn write_code(&mut self, block: &AsmCodeBlock) -> Result<()> {
        if let Some(id) = self.program.labels.find_block(block.src_block) {
            writeln!(self.writer, "{}:", self.program.labels.get(id).name)?;
        }
        for command in &block.commands {
            self.write_command(command)?;
        }
        Ok(())
    }

    fn write_data(&mut self, block: &AsmDataBlock) -> Result<()> {
        if let Some(id) = self.program.labels.find_block(block.src_block) {
            writeln!(self.writer, "{}:", self.program.labels.get(id).name)?;
        }
        for data in &block.data {
            write!(self.writer, "\t.{}\t", data.opcode.name())?;
            self.write_operands(&data.operands)?;
            writeln!(self.writer)?;
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
