use crate::label::{LabelId, LabelMap};
use crate::lexer::Number;
use crate::opcodes::{AsmMsgOp, DirOp, NamedOpcode};
use crate::parser::{Ast, Item, Node, Value};
use crate::program::{
    Block, BlockContent, EntryPoint, Operand, OperandType, Operation, Program, Target, TypeHint,
};
use crate::{Error, Result};
use smol_str::SmolStr;
use std::collections::HashMap;
use unplug::common::Text;
use unplug::event::opcodes::{CmdOp, ExprOp, TypeOp};
use unplug::event::BlockId;
use unplug::stage::Event;

/// Assembles a `Program` from an AST.
pub struct ProgramAssembler<'a> {
    ast: &'a Ast,
    program: Program,
}

impl<'a> ProgramAssembler<'a> {
    /// Creates a new `ProgramAssembler` that parses `ast`.
    pub fn new(ast: &'a Ast) -> Self {
        Self { ast, program: Program::new() }
    }

    /// Parses the AST and assembles a `Program`.
    pub fn assemble(mut self) -> Result<Program> {
        if !self.ast.items.is_empty() {
            self.scan_labels()?;
            self.parse_instructions()?;
            self.split_blocks()?;
            self.prune_blocks();
            // Ensure blocks have the correct flags. This isn't strictly necessary for compilation,
            // but it allows the program to be written back out using `ProgramWriter`.
            self.program.mark_subroutines();
        }
        Ok(self.program)
    }

    /// Scans for labels and defines blocks for them.
    fn scan_labels(&mut self) -> Result<()> {
        // Always create at least one block in case the first item is not a label.
        let mut block_id = BlockId::new(0);
        self.program.blocks.push(Block::new());
        self.program.first_block = Some(block_id);
        let mut new_block = false;
        for item in &self.ast.items {
            if let Item::Label(label) = &item.data {
                if new_block {
                    block_id = self.program.insert_after(Some(block_id), Block::new());
                    // Labels right after each other refer to the same block
                    new_block = false;
                }
                self.program.labels.insert_new(label.data.as_str(), Some(block_id))?;
            } else {
                // There's content in this block, so the next label starts a new one
                new_block = true;
            }
        }
        Ok(())
    }

    /// Parses instructions into blocks.
    fn parse_instructions(&mut self) -> Result<()> {
        let mut block_id = self.program.first_block.unwrap();
        for item in &self.ast.items {
            match &item.data {
                Item::Label(label) => {
                    let labels = &self.program.labels;
                    let id = labels
                        .find_name(label.data.as_str())
                        .expect("label does not have an associated ID");
                    block_id =
                        labels.get(id).block.expect("label does not have an associated block");
                }
                Item::Command(name, values) => {
                    let opcode = Self::parse_command_opcode(name)?;
                    let command = self.parse_operation(opcode, values)?;
                    block_id = self.process_command(block_id, command);
                }
                Item::Directive(name, values) => {
                    let opcode = Self::parse_directive_opcode(name)?;
                    let dir = self.parse_operation(opcode, values)?;
                    block_id = self.process_directive(block_id, dir)?;
                }
            }
        }
        Ok(())
    }

    /// Splits code blocks so that they always end with a control-flow statement.
    fn split_blocks(&mut self) -> Result<()> {
        let mut current = self.program.first_block;
        while let Some(block_id) = current {
            let block = block_id.get_mut(&mut self.program.blocks);
            current = block.next;
            if let Some(BlockContent::Code(commands)) = &mut block.content {
                let end_index = commands.iter().position(|c| c.opcode.is_control_flow());
                if let Some(end_index) = end_index {
                    let split = commands.split_off(end_index + 1);
                    if !split.is_empty() {
                        let new_block = Block::with_code(split);
                        current = Some(self.program.insert_after(Some(block_id), new_block));
                    }
                }
            }
        }
        Ok(())
    }

    /// Filters out empty blocks by adjusting surrounding blocks to jump over them.
    fn prune_blocks(&mut self) {
        let mut prev: Option<BlockId> = None;
        let mut current = self.program.first_block;
        while let Some(block_id) = current {
            let block = block_id.get(&self.program.blocks);
            let next = block.next;
            if block.is_empty() {
                // We can't delete the block because that would shift all the IDs, but we can make
                // the previous block (or start of the program) jump over this one
                match prev {
                    Some(prev) => prev.get_mut(&mut self.program.blocks).next = next,
                    None => self.program.first_block = next,
                }
            } else {
                prev = current;
            }
            current = next;
        }
    }

    /// Processes a command encountered inside a block. If the block is not a code block, it will be
    /// split. Returns the ID of the block the command was actually added to.
    fn process_command(&mut self, mut block_id: BlockId, command: Operation<CmdOp>) -> BlockId {
        let block = block_id.get_mut(&mut self.program.blocks);
        if block.is_data() {
            block_id = self.program.insert_after(Some(block_id), Block::new());
        }
        block_id.get_mut(&mut self.program.blocks).push_command(command);
        block_id
    }

    /// Processes a directive encountered inside a block. If the directive produces data and the
    /// block is not a data block, it will be split. Returns the ID of the block the directive was
    /// actually processed in.
    fn process_directive(
        &mut self,
        mut block_id: BlockId,
        dir: Operation<DirOp>,
    ) -> Result<BlockId> {
        match dir.opcode {
            DirOp::Globals => {
                if self.program.target.replace(Target::Globals).is_some() {
                    return Err(Error::DuplicateTarget);
                }
            }
            DirOp::Stage => {
                let name_op = dir.operands.get(0).ok_or(Error::ExpectedText)?;
                let Operand::Text(name_text) = name_op else { return Err(Error::ExpectedText) };
                let name = name_text.decode()?.into_owned();
                if self.program.target.replace(Target::Stage(name)).is_some() {
                    return Err(Error::DuplicateTarget);
                }
            }
            DirOp::Byte | DirOp::Word | DirOp::Dword => {
                // Data
                let block = block_id.get_mut(&mut self.program.blocks);
                if block.is_code() {
                    block_id = self.program.insert_after(Some(block_id), Block::new());
                }
                block_id.get_mut(&mut self.program.blocks).push_data(dir.operands);
            }
            DirOp::Prologue
            | DirOp::Startup
            | DirOp::Dead
            | DirOp::Pose
            | DirOp::TimeCycle
            | DirOp::TimeUp
            | DirOp::Interact
            | DirOp::Lib => Self::process_entry_point(
                &mut self.program.entry_points,
                &self.program.labels,
                dir.opcode,
                &dir.operands,
            )?,
        }
        Ok(block_id)
    }

    /// Processes a directive that declares an entry point.
    fn process_entry_point(
        entry_points: &mut HashMap<EntryPoint, BlockId>,
        labels: &LabelMap,
        dir: DirOp,
        operands: &[Operand],
    ) -> Result<()> {
        // `.interact` and `.lib` take an argument which comes before the label opcode, so we have
        // to match the opcode to know where the label is
        let (entry_point, label_op) = match dir {
            DirOp::Prologue => (EntryPoint::Event(Event::Prologue), operands.get(0)),
            DirOp::Startup => (EntryPoint::Event(Event::Startup), operands.get(0)),
            DirOp::Dead => (EntryPoint::Event(Event::Dead), operands.get(0)),
            DirOp::Pose => (EntryPoint::Event(Event::Pose), operands.get(0)),
            DirOp::TimeCycle => (EntryPoint::Event(Event::TimeCycle), operands.get(0)),
            DirOp::TimeUp => (EntryPoint::Event(Event::TimeUp), operands.get(0)),
            DirOp::Interact => {
                let object = operands.get(0).ok_or(Error::ExpectedObjectIndex)?;
                (EntryPoint::Event(Event::Interact(object.cast()?)), operands.get(1))
            }
            DirOp::Lib => {
                let index = operands.get(0).ok_or(Error::ExpectedInteger)?;
                (EntryPoint::Lib(index.cast()?), operands.get(1))
            }
            _ => panic!("directive is not an event: {:?}", dir),
        };
        let label = label_op.ok_or(Error::ExpectedLabel)?.label()?;
        let block = labels.get(label).block.unwrap();
        match entry_points.insert(entry_point, block) {
            Some(_) => Err(Error::DuplicateEntryPoint(entry_point)),
            None => Ok(()),
        }
    }

    /// Parses an opcode and operands into an `Operation`.
    fn parse_operation<T: NamedOpcode + TypeHint>(
        &self,
        opcode: T,
        values: &[Node<Value>],
    ) -> Result<Operation<T>> {
        let mut operation = Operation::new(opcode);
        operation.operands.reserve(values.len());
        let ty = opcode.type_hint();
        for value in values {
            operation.operands.push(self.parse_operand(ty, value)?);
        }
        Ok(operation)
    }

    /// Parses a single operand.
    fn parse_operand(&self, ty: OperandType, value: &Node<Value>) -> Result<Operand> {
        match &value.data {
            Value::Number(num) => Self::parse_number(ty, *num),
            Value::Text(text) => Self::parse_text(ty, text.as_str()),
            Value::Type(name) => Self::parse_type(ty, &value.map(|_| name.clone())),
            Value::Label(name) => self.parse_label(ty, &name.data).map(Operand::Label),
            Value::ElseLabel(name) => self.parse_label(ty, &name.data).map(Operand::ElseLabel),
            Value::Offset(num) => Self::parse_offset(ty, num.data),
            Value::Function(name, values) => self.parse_function(ty, name, values),
        }
    }

    /// Parses a number of a potentially-unknown type into an operand with a known type.
    fn parse_number(ty: OperandType, num: Number) -> Result<Operand> {
        match num {
            Number::I8(x) => i8::try_from(x).map(Operand::I8).map_err(|_| Error::Invalid8(num)),
            Number::U8(x) => u8::try_from(x).map(Operand::U8).map_err(|_| Error::Invalid8(num)),
            Number::I16(x) => i16::try_from(x).map(Operand::I16).map_err(|_| Error::Invalid16(num)),
            Number::U16(x) => u16::try_from(x).map(Operand::U16).map_err(|_| Error::Invalid16(num)),
            Number::I32(x) => Ok(Operand::I32(x)),
            Number::U32(x) => Ok(Operand::U32(x)),
            Number::IAuto(x) => {
                // If we know what type we're parsing as, then forcibly parse as that type,
                // otherwise find the smallest type which fits
                match ty {
                    OperandType::Byte => Self::parse_number(ty, Number::I8(x)),
                    OperandType::Word => Self::parse_number(ty, Number::I16(x)),
                    OperandType::Dword => Self::parse_number(ty, Number::I32(x)),
                    _ => i8::try_from(x)
                        .map(Operand::I8)
                        .or_else(|_| i16::try_from(x).map(Operand::I16))
                        .or(Ok(Operand::I32(x))),
                }
            }
            Number::UAuto(x) => {
                // Same as with IAuto
                match ty {
                    OperandType::Byte => Self::parse_number(ty, Number::U8(x)),
                    OperandType::Word => Self::parse_number(ty, Number::U16(x)),
                    OperandType::Dword => Self::parse_number(ty, Number::U32(x)),
                    _ => u8::try_from(x)
                        .map(Operand::U8)
                        .or_else(|_| u16::try_from(x).map(Operand::U16))
                        .or(Ok(Operand::U32(x))),
                }
            }
        }
    }

    /// Parses a text operand.
    fn parse_text(ty: OperandType, text: &str) -> Result<Operand> {
        if !matches!(ty, OperandType::Unknown | OperandType::Byte | OperandType::Message) {
            return Err(Error::OperandTypeExpected(ty));
        }
        // TODO: This is lazy
        let unescaped = text.replace(r"\n", "\n");
        let encoded = Text::encode(&unescaped)?;
        Ok(Operand::Text(encoded))
    }

    /// Parses a @type operand.
    fn parse_type(ty: OperandType, name: &Node<SmolStr>) -> Result<Operand> {
        if !matches!(ty, OperandType::Unknown | OperandType::Dword) {
            return Err(Error::OperandTypeExpected(ty));
        }
        Ok(Operand::Type(Self::parse_type_opcode(name)?))
    }

    /// Parses a label reference operand.
    fn parse_label(&self, ty: OperandType, name: &SmolStr) -> Result<LabelId> {
        if !matches!(ty, OperandType::Unknown | OperandType::Dword) {
            return Err(Error::OperandTypeExpected(ty));
        }
        self.program
            .labels
            .find_name(name.as_str())
            .ok_or_else(|| Error::UndefinedLabel(name.clone()))
    }

    /// Parses an offset reference operand.
    fn parse_offset(ty: OperandType, num: Number) -> Result<Operand> {
        if !matches!(ty, OperandType::Unknown | OperandType::Dword) {
            return Err(Error::OperandTypeExpected(ty));
        }
        Ok(Operand::Offset(Self::parse_number(OperandType::Unknown, num)?.cast()?))
    }

    /// Parses a "function" operand, which depending on context may be an expression or message
    /// command.
    fn parse_function(
        &self,
        ty: OperandType,
        name: &Node<SmolStr>,
        values: &[Node<Value>],
    ) -> Result<Operand> {
        match ty {
            OperandType::Unknown => {
                let opcode = Self::parse_expr_opcode(name)?;
                self.parse_operation(opcode, values).map(|op| Operand::Expr(op.into()))
            }
            OperandType::Message => {
                let opcode = Self::parse_msg_opcode(name)?;
                self.parse_operation(opcode, values).map(|op| Operand::MsgCommand(op.into()))
            }
            _ => Err(Error::OperandTypeExpected(ty)),
        }
    }

    /// Parses a command name.
    fn parse_command_opcode(name: &Node<SmolStr>) -> Result<CmdOp> {
        CmdOp::get(name.data.as_str()).ok_or_else(|| Error::UnrecognizedCommand(name.data.clone()))
    }

    /// Parses a directive name.
    fn parse_directive_opcode(name: &Node<SmolStr>) -> Result<DirOp> {
        DirOp::get(name.data.as_str()).ok_or_else(|| Error::UnrecognizedCommand(name.data.clone()))
    }

    /// Parses a @type name.
    fn parse_type_opcode(name: &Node<SmolStr>) -> Result<TypeOp> {
        TypeOp::get(name.data.as_str()).ok_or_else(|| Error::UnrecognizedType(name.data.clone()))
    }

    /// Parses an expression name.
    fn parse_expr_opcode(name: &Node<SmolStr>) -> Result<ExprOp> {
        ExprOp::get(name.data.as_str())
            .ok_or_else(|| Error::UnrecognizedFunction(name.data.clone()))
    }

    /// Parses a message command name.
    fn parse_msg_opcode(name: &Node<SmolStr>) -> Result<AsmMsgOp> {
        AsmMsgOp::get(name.data.as_str())
            .ok_or_else(|| Error::UnrecognizedFunction(name.data.clone()))
    }
}
