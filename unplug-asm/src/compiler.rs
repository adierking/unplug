use crate::opcodes::{AsmMsgOp, NamedOpcode};
use crate::program::{Block, BlockContent, CastOperand, Operand, Operation, Program};
use crate::{Error, Result};
use smallvec::SmallVec;
use std::cmp::Ordering;
use std::ops::Deref;
use unplug::common::Text;
use unplug::event::analysis::SubroutineEffectsMap;
use unplug::event::block::Block as ScriptBlock;
use unplug::event::opcodes::{CmdOp, ExprOp, Ggte, MsgOp, OpcodeMap, TypeOp};
use unplug::event::script::{Error as ScriptError, Script, ScriptLayout};
use unplug::event::serialize::{
    DeserializeEvent, Error as SerError, EventDeserializer, Result as SerResult,
};
use unplug::event::{CodeBlock, Command, DataBlock, Pointer};

/// Differentiates cursor types and stores the underlying opcode.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum CursorKind {
    Command(CmdOp),
    Expr(ExprOp),
    MsgCommand(AsmMsgOp),
}

impl CursorKind {
    /// Returns the opcode name.
    fn name(self) -> &'static str {
        match self {
            Self::Command(cmd) => cmd.name(),
            Self::Expr(expr) => expr.name(),
            Self::MsgCommand(msg) => msg.name(),
        }
    }

    /// Returns true if the operands should be read in reversed order.
    fn has_reversed_operands(self) -> bool {
        // TODO: Put this somewhere common instead of duplicating this logic in writer.rs
        matches!(
            self,
            Self::Expr(
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
                    | ExprOp::BitXorAssign
            ) | Self::Command(CmdOp::Set)
        )
    }
}

impl From<CmdOp> for CursorKind {
    fn from(opcode: CmdOp) -> Self {
        Self::Command(opcode)
    }
}

impl From<ExprOp> for CursorKind {
    fn from(opcode: ExprOp) -> Self {
        Self::Expr(opcode)
    }
}

impl From<AsmMsgOp> for CursorKind {
    fn from(opcode: AsmMsgOp) -> Self {
        Self::MsgCommand(opcode)
    }
}

/// A trait for matching opcode types against a `CursorKind`.
trait MatchCursorKind: NamedOpcode {
    fn match_cursor(kind: CursorKind) -> Result<Self>;
}

impl MatchCursorKind for CmdOp {
    fn match_cursor(kind: CursorKind) -> Result<Self> {
        let CursorKind::Command(cmd) = kind else { return Err(Error::ExpectedCommand) };
        Ok(cmd)
    }
}

impl MatchCursorKind for ExprOp {
    fn match_cursor(kind: CursorKind) -> Result<Self> {
        let CursorKind::Expr(expr) = kind else { return Err(Error::ExpectedExpr) };
        Ok(expr)
    }
}

impl MatchCursorKind for AsmMsgOp {
    fn match_cursor(kind: CursorKind) -> Result<Self> {
        let CursorKind::MsgCommand(msg) = kind else { return Err(Error::ExpectedMessage) };
        Ok(msg)
    }
}

/// A list of operands which can be borrowed or owned.
/// Similar to `Cow` but better-suited for operand data.
#[derive(Debug, Clone)]
#[allow(variant_size_differences)]
enum CursorData<'a> {
    Borrowed(&'a [Operand]),
    Owned(SmallVec<[Operand; 2]>),
}

impl<'a> CursorData<'a> {
    /// Returns the underlying operand slice iff it is borrowed.
    fn ensure_borrowed(&self) -> Option<&'a [Operand]> {
        match self {
            Self::Borrowed(operands) => Some(operands),
            Self::Owned(_) => None,
        }
    }
}

impl<'a> Deref for CursorData<'a> {
    type Target = [Operand];
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Borrowed(data) => data,
            Self::Owned(data) => data,
        }
    }
}

/// A cursor which iterates over an operand list of any kind.
struct OperandCursor<'a> {
    /// The cursor kind and opcode of the current operation.
    kind: CursorKind,
    /// The operand list, which may be generated or borrowed from the program.
    operands: CursorData<'a>,
    /// The position of the cursor in the operand list.
    /// This always counts up from 0, even if the list is reversed.
    position: usize,
    /// True if the operands should be read in reversed order.
    reversed: bool,
}

impl<'a> OperandCursor<'a> {
    /// Creates a cursor over a borrowed operation.
    fn with_borrowed<T>(op: &'a Operation<T>) -> Self
    where
        T: NamedOpcode + Into<CursorKind>,
    {
        let kind = op.opcode.into();
        Self {
            kind,
            operands: CursorData::Borrowed(&op.operands),
            position: 0,
            reversed: kind.has_reversed_operands(),
        }
    }

    /// Creates a cursor which owns its operation.
    fn with_owned<T>(op: Operation<T>) -> Self
    where
        T: NamedOpcode + Into<CursorKind>,
    {
        let kind = op.opcode.into();
        Self {
            kind,
            operands: CursorData::Owned(op.operands),
            position: 0,
            reversed: kind.has_reversed_operands(),
        }
    }

    /// If the cursor is for an opcode of type `T`, returns it, otherwise returns an error.
    fn opcode<T: MatchCursorKind>(&self) -> Result<T> {
        T::match_cursor(self.kind)
    }

    /// Returns true if the cursor has another operand.
    fn has_next(&self) -> bool {
        self.index().is_some()
    }

    /// Returns the next operand (if any) and advances the cursor.
    fn next(&mut self) -> Option<&Operand> {
        if let Some(index) = self.index() {
            let operand = &self.operands[index];
            self.position += 1;
            Some(operand)
        } else {
            None
        }
    }

    /// Returns the index of the current operand if there is one.
    fn index(&self) -> Option<usize> {
        if self.position < self.operands.len() {
            match self.reversed {
                true => Some(self.operands.len() - 1 - self.position),
                false => Some(self.position),
            }
        } else {
            None
        }
    }

    /// Descends into an expression, returning the opcode and subcursor.
    fn enter_expr(&mut self) -> Result<(ExprOp, Self)> {
        let Some(index) = self.index() else { return Err(Error::ExpectedExpr) };
        if !matches!(self.kind, CursorKind::Command(_) | CursorKind::Expr(_)) {
            return Err(Error::ExpectedExpr);
        }
        let operands = self.operands.ensure_borrowed().ok_or(Error::ExpectedExpr)?;
        let operand = &operands[index];
        let (opcode, cursor) = match operand {
            Operand::I8(_) | Operand::U8(_) | Operand::I16(_) | Operand::U16(_) => {
                let op = Operation::with_operands(ExprOp::Imm16, [operand.clone()]);
                (op.opcode, Self::with_owned(op))
            }
            Operand::I32(_) | Operand::U32(_) | Operand::Type(_) => {
                let op = Operation::with_operands(ExprOp::Imm32, [operand.clone()]);
                (op.opcode, Self::with_owned(op))
            }
            Operand::Label(_) | Operand::ElseLabel(_) | Operand::Offset(_) => {
                let op = Operation::with_operands(ExprOp::AddressOf, [operand.clone()]);
                (op.opcode, Self::with_owned(op))
            }
            Operand::Expr(expr) => (expr.opcode, Self::with_borrowed(expr)),
            Operand::Text(_) | Operand::MsgCommand(_) => return Err(Error::ExpectedExpr),
        };
        self.position += 1;
        Ok((opcode, cursor))
    }

    /// Descends into a message command, returning the opcode and subcursor.
    fn enter_msg_command(&mut self) -> Result<(AsmMsgOp, Self)> {
        let Some(index) = self.index() else { return Err(Error::ExpectedExpr) };
        if !matches!(self.kind, CursorKind::Command(_)) {
            return Err(Error::ExpectedMessage);
        }
        let operands = self.operands.ensure_borrowed().ok_or(Error::ExpectedMessage)?;
        let operand = &operands[index];
        let (opcode, cursor) = match operand {
            Operand::Text(_) => {
                let op = Operation::with_operands(AsmMsgOp::Text, [operand.clone()]);
                (op.opcode, Self::with_owned(op))
            }
            Operand::MsgCommand(op) => (op.opcode, Self::with_borrowed(op)),
            _ => return Err(Error::ExpectedMessage),
        };
        self.position += 1;
        Ok((opcode, cursor))
    }

    /// Consumes the cursor, also validating that the opcode matches `T` and all operands were
    /// consumed.
    fn leave<T: MatchCursorKind>(self) -> Result<()> {
        T::match_cursor(self.kind)?;
        if self.has_next() {
            Err(Error::TooManyOperands {
                name: self.kind.name(),
                expected: self.position,
                actual: self.operands.len(),
            })
        } else {
            Ok(())
        }
    }
}

/// A cursor which iterates over a text string and produces a `MsgOp` for each byte.
#[derive(Debug)]
struct TextCursor {
    text: Text,
    offset: usize,
}

impl TextCursor {
    fn new(text: Text) -> Self {
        Self { text, offset: 0 }
    }

    fn next(&mut self) -> Option<MsgOp> {
        let bytes = self.text.as_bytes();
        if self.offset < bytes.len() {
            let b = bytes[self.offset];
            self.offset += 1;
            Some(Ggte::get(b).unwrap())
        } else {
            None
        }
    }
}

/// An `EventDeserializer` implementation which reads commands out of a `Program`.
struct AsmDeserializer<'a> {
    /// The program to read from.
    program: &'a Program,
    /// The block within the program to read from.
    block: &'a Block,
    /// The index of the current command within the block.
    command_index: usize,
    /// The current operand cursor, if we are reading a command.
    cursor: Option<OperandCursor<'a>>,
    /// The current text cursor, if we are reading a message string.
    text: Option<TextCursor>,
    /// Cursors for elements higher-up in the tree.
    stack: Vec<OperandCursor<'a>>,
}

impl<'a> AsmDeserializer<'a> {
    fn new(program: &'a Program, block: &'a Block) -> Self {
        Self { program, block, command_index: 0, cursor: None, text: None, stack: vec![] }
    }

    fn has_command(&self) -> bool {
        if let Some(BlockContent::Code(code)) = &self.block.content {
            self.command_index < code.len()
        } else {
            false
        }
    }

    fn next_operand(&mut self) -> Option<&Operand> {
        self.cursor.as_mut().and_then(|c| c.next())
    }

    fn deserialize_array<T: CastOperand>(&mut self, len: usize) -> SerResult<Vec<T>> {
        let mut values = vec![];
        for _ in 0..len {
            let operand = self.next_operand().ok_or(Error::ExpectedInteger)?;
            values.push(operand.cast()?);
        }
        Ok(values)
    }
}

impl EventDeserializer for AsmDeserializer<'_> {
    fn deserialize_i8(&mut self) -> SerResult<i8> {
        Ok(self.next_operand().ok_or(Error::ExpectedInteger)?.cast()?)
    }

    fn deserialize_u8(&mut self) -> SerResult<u8> {
        Ok(self.next_operand().ok_or(Error::ExpectedInteger)?.cast()?)
    }

    fn deserialize_i16(&mut self) -> SerResult<i16> {
        Ok(self.next_operand().ok_or(Error::ExpectedInteger)?.cast()?)
    }

    fn deserialize_u16(&mut self) -> SerResult<u16> {
        Ok(self.next_operand().ok_or(Error::ExpectedInteger)?.cast()?)
    }

    fn deserialize_i32(&mut self) -> SerResult<i32> {
        Ok(self.next_operand().ok_or(Error::ExpectedInteger)?.cast()?)
    }

    fn deserialize_u32(&mut self) -> SerResult<u32> {
        Ok(self.next_operand().ok_or(Error::ExpectedInteger)?.cast()?)
    }

    fn deserialize_pointer(&mut self) -> SerResult<Pointer> {
        match *self.next_operand().ok_or(SerError::EndOfData)? {
            Operand::Label(label) | Operand::ElseLabel(label) => {
                // TODO: Only allow else labels in conditionals
                Ok(Pointer::Block(self.program.labels.get(label).block.unwrap()))
            }
            Operand::Offset(o) => Ok(Pointer::Offset(o)),
            _ => Err(Error::ExpectedLabel.into()),
        }
    }

    fn deserialize_i8_array(&mut self, len: usize) -> SerResult<Vec<i8>> {
        self.deserialize_array(len)
    }

    fn deserialize_u8_array(&mut self, len: usize) -> SerResult<Vec<u8>> {
        self.deserialize_array(len)
    }

    fn deserialize_i16_array(&mut self, len: usize) -> SerResult<Vec<i16>> {
        self.deserialize_array(len)
    }

    fn deserialize_u16_array(&mut self, len: usize) -> SerResult<Vec<u16>> {
        self.deserialize_array(len)
    }

    fn deserialize_i32_array(&mut self, len: usize) -> SerResult<Vec<i32>> {
        self.deserialize_array(len)
    }

    fn deserialize_u32_array(&mut self, len: usize) -> SerResult<Vec<u32>> {
        self.deserialize_array(len)
    }

    fn deserialize_pointer_array(&mut self, max_len: usize) -> SerResult<Vec<Pointer>> {
        let mut pointers = vec![];
        for _ in 0..max_len {
            let pointer = match self.deserialize_pointer() {
                Ok(p) => p,
                Err(SerError::EndOfData) => break,
                Err(e) => return Err(e),
            };
            pointers.push(pointer)
        }
        Ok(pointers)
    }

    fn deserialize_type(&mut self) -> SerResult<TypeOp> {
        let operand = self.next_operand().ok_or(Error::ExpectedInteger)?;
        match operand {
            Operand::Type(op) => Ok(*op),
            _ => Ggte::get(operand.cast()?).map_err(SerError::UnrecognizedType),
        }
    }

    fn deserialize_text(&mut self) -> SerResult<Text> {
        match self.next_operand() {
            Some(Operand::Text(text)) => Ok(text.clone()), // TODO: Avoid this clone?
            Some(_) => Err(Error::ExpectedText.into()),
            None => Err(SerError::EndOfData),
        }
    }

    fn deserialize_rgba(&mut self) -> SerResult<u32> {
        self.deserialize_u32()
    }

    fn begin_expr(&mut self) -> SerResult<ExprOp> {
        let cursor = self.cursor.as_mut().ok_or(Error::ExpectedExpr)?;
        let (opcode, subcursor) = cursor.enter_expr()?;
        self.stack.push(self.cursor.replace(subcursor).unwrap());
        Ok(opcode)
    }

    fn end_expr(&mut self) -> SerResult<()> {
        let cursor = self.cursor.take().ok_or(Error::ExpectedExpr)?;
        cursor.leave::<ExprOp>()?;
        self.cursor = self.stack.pop();
        Ok(())
    }

    fn begin_command(&mut self) -> SerResult<CmdOp> {
        if self.cursor.is_some() {
            return Err(Error::ExpectedCommand.into());
        }
        let code = match &self.block.content {
            Some(BlockContent::Code(code)) => code,
            Some(BlockContent::Data(_)) => return Err(Error::ExpectedCommand.into()),
            _ => return Err(SerError::EndOfData),
        };
        let command = code.get(self.command_index).ok_or(SerError::EndOfData)?;
        self.cursor = Some(OperandCursor::with_borrowed(command));
        Ok(command.opcode)
    }

    fn end_command(&mut self) -> SerResult<()> {
        let cursor = self.cursor.take().ok_or_else(|| SerError::from(Error::ExpectedCommand))?;
        cursor.leave::<CmdOp>()?;
        self.command_index += 1;
        Ok(())
    }

    fn begin_call(&mut self) -> SerResult<()> {
        Ok(())
    }

    fn have_call_arg(&mut self) -> SerResult<bool> {
        let cursor = self.cursor.as_ref().ok_or(Error::ExpectedCommand)?;
        Ok(cursor.has_next())
    }

    fn end_call(&mut self) -> SerResult<()> {
        Ok(())
    }

    fn begin_msg(&mut self) -> SerResult<()> {
        Ok(())
    }

    fn deserialize_msg_char(&mut self) -> SerResult<MsgOp> {
        // If we're currently iterating over text, see if there are characters left
        let mut cursor = self.cursor.as_mut().ok_or(Error::ExpectedMessage)?;
        if let Some(text) = &mut self.text {
            if let Some(b) = text.next() {
                return Ok(b);
            }
            // For format strings, we have to emit a Format byte at the end
            self.text = None;
            if cursor.opcode::<AsmMsgOp>()? == AsmMsgOp::Format {
                return Ok(MsgOp::Format);
            }
        }

        // If we're currently in a message command, we should be done with it now
        if cursor.opcode::<AsmMsgOp>().is_ok() {
            self.cursor.replace(self.stack.pop().unwrap()).unwrap().leave::<AsmMsgOp>()?;
            cursor = self.cursor.as_mut().ok_or(Error::ExpectedMessage)?;
        }

        // Emit an end byte if we're at the end of the message
        if !cursor.has_next() {
            return Ok(MsgOp::End);
        }

        // Now enter the next message command
        let (opcode, mut subcursor) = cursor.enter_msg_command()?;

        // For strings, make a text cursor
        if let AsmMsgOp::Format | AsmMsgOp::Text = opcode {
            let operand = subcursor.next().ok_or(Error::ExpectedMessage)?;
            let Operand::Text(text) = operand else { return Err(Error::ExpectedText.into()) };
            self.text = Some(TextCursor::new(text.clone()));
        }
        self.stack.push(self.cursor.replace(subcursor).unwrap());

        // Finally, translate the command
        Ok(match opcode {
            AsmMsgOp::Speed => MsgOp::Speed,
            AsmMsgOp::Wait => MsgOp::Wait,
            AsmMsgOp::Anim => MsgOp::Anim,
            AsmMsgOp::Sfx => MsgOp::Sfx,
            AsmMsgOp::Voice => MsgOp::Voice,
            AsmMsgOp::Default => MsgOp::Default,
            AsmMsgOp::Format => MsgOp::Format,
            AsmMsgOp::Size => MsgOp::Size,
            AsmMsgOp::Color => MsgOp::Color,
            AsmMsgOp::Rgba => MsgOp::Rgba,
            AsmMsgOp::Proportional => MsgOp::Proportional,
            AsmMsgOp::Icon => MsgOp::Icon,
            AsmMsgOp::Shake => MsgOp::Shake,
            AsmMsgOp::Center => MsgOp::Center,
            AsmMsgOp::Rotate => MsgOp::Rotate,
            AsmMsgOp::Scale => MsgOp::Scale,
            AsmMsgOp::NumInput => MsgOp::NumInput,
            AsmMsgOp::Question => MsgOp::Question,
            AsmMsgOp::Stay => MsgOp::Stay,
            // There is no command for plain text, so just recurse
            AsmMsgOp::Text => return self.deserialize_msg_char(),
        })
    }

    fn end_msg(&mut self) -> SerResult<()> {
        // End the current message command if there is one
        let cursor = self.cursor.as_mut().ok_or(Error::ExpectedMessage)?;
        if cursor.opcode::<AsmMsgOp>().is_ok() {
            self.cursor.replace(self.stack.pop().unwrap()).unwrap().leave::<AsmMsgOp>()?;
        }
        Ok(())
    }
}

/// Compiles a code block.
fn compile_code(program: &Program, block: &Block) -> Result<ScriptBlock> {
    let mut code = CodeBlock::new();
    let mut deserializer = AsmDeserializer::new(program, block);
    while deserializer.has_command() {
        let cmd = Command::deserialize(&mut deserializer)
            .map_err(|e| ScriptError::WriteCommand(e.into()))?;
        code.commands.push(cmd);
    }
    code.next_block = block.next.map(Pointer::Block);
    if let Some(last) = code.commands.last() {
        if let Some(args) = last.if_args() {
            code.else_block = Some(args.else_target);
        }
    }
    Ok(ScriptBlock::Code(code))
}

/// Converts an operand into a data block.
fn into_data(program: &Program, value: &Operand) -> Result<DataBlock> {
    match value {
        Operand::I8(x) => Ok(DataBlock::I8Array(vec![*x])),
        Operand::U8(x) => Ok(DataBlock::U8Array(vec![*x])),
        Operand::I16(x) => Ok(DataBlock::I16Array(vec![*x])),
        Operand::U16(x) => Ok(DataBlock::U16Array(vec![*x])),
        Operand::I32(x) => Ok(DataBlock::I32Array(vec![*x])),
        Operand::U32(x) => Ok(DataBlock::U32Array(vec![*x])),
        Operand::Text(text) => Ok(DataBlock::String(text.clone())),
        Operand::Label(label) => {
            let block = program.labels.get(*label).block.unwrap();
            Ok(DataBlock::PtrArray(vec![Pointer::Block(block)]))
        }
        Operand::Offset(x) => Ok(DataBlock::PtrArray(vec![Pointer::Offset(*x)])),
        Operand::Type(x) => Ok(DataBlock::I32Array(vec![Ggte::value(*x).unwrap()])),
        Operand::ElseLabel(_) => Err(Error::UnexpectedElseLabel),
        Operand::Expr(_) => Err(Error::UnexpectedExpr),
        Operand::MsgCommand(_) => Err(Error::UnexpectedMessage),
    }
}

/// Tries to append `value` to existing data block `block`.
fn try_append_data(program: &Program, block: &mut DataBlock, value: &Operand) -> bool {
    match block {
        DataBlock::I8Array(array) => {
            let Operand::I8(x) = *value else { return false };
            array.push(x);
        }
        DataBlock::U8Array(array) => {
            let Operand::U8(x) = *value else { return false };
            array.push(x);
        }
        DataBlock::I16Array(array) => {
            let Operand::I16(x) = *value else { return false };
            array.push(x);
        }
        DataBlock::U16Array(array) => {
            let Operand::U16(x) = *value else { return false };
            array.push(x);
        }
        DataBlock::I32Array(array) => match *value {
            Operand::I32(x) => array.push(x),
            Operand::Type(x) => array.push(Ggte::value(x).unwrap()),
            _ => return false,
        },
        DataBlock::U32Array(array) => {
            let Operand::U32(x) = *value else { return false };
            array.push(x);
        }
        DataBlock::PtrArray(array) => {
            let ptr = match *value {
                Operand::Label(label) => Pointer::Block(program.labels.get(label).block.unwrap()),
                Operand::Offset(offset) => Pointer::Offset(offset),
                _ => return false,
            };
            array.push(ptr);
        }
        DataBlock::String(text) => match value {
            Operand::I8(b) => text.push(*b as u8),
            Operand::U8(b) => text.push(*b),
            Operand::Text(other) => text.extend(other.iter()),
            _ => return false,
        },
        DataBlock::ObjBone(_) | DataBlock::ObjPair(_) | DataBlock::Variable(_) => return false, // Not supported
    }
    true
}

/// Compiles a data block.
fn compile_data(program: &Program, data: &[Operand]) -> Result<ScriptBlock> {
    let mut blocks: Vec<DataBlock> = vec![];
    for value in data {
        if let Some(last) = blocks.last_mut() {
            if try_append_data(program, last, value) {
                continue;
            }
        }
        blocks.push(into_data(program, value)?);
    }
    match blocks.len().cmp(&1) {
        Ordering::Greater => Ok(ScriptBlock::Data(DataBlock::Variable(blocks))),
        Ordering::Equal => Ok(ScriptBlock::Data(blocks.remove(0))),
        Ordering::Less => Ok(ScriptBlock::Data(DataBlock::U8Array(vec![]))),
    }
}

/// Compiles a program into an event script.
pub fn compile(program: &Program) -> Result<Script> {
    // Compile each block individually
    let mut script_blocks = vec![];
    for block in &program.blocks {
        let script_block = match &block.content {
            Some(BlockContent::Code(_)) | None => compile_code(program, block)?,
            Some(BlockContent::Data(data)) => compile_data(program, data)?,
        };
        script_blocks.push(script_block);
    }

    // Chain the blocks together using made-up file offsets so they end up in the right order
    // TODO: Ideally we should refactor this so that we don't need to make up offsets
    let mut block_offsets = vec![0; script_blocks.len()];
    let mut offset = 0x1000; // HACK: this should be outside the stage header
    let mut current = program.first_block;
    while let Some(block_id) = current {
        *block_id.get_mut(&mut block_offsets) = offset;
        offset += 1;
        current = block_id.get(&program.blocks).next;
    }

    let layout = ScriptLayout::new(block_offsets, SubroutineEffectsMap::new());
    Ok(Script::with_blocks_and_layout(script_blocks, layout))
}
