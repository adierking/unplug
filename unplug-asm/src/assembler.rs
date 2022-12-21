use crate::ast::{self, Ast, Expr, IdentClass, IntValue, Item};
use crate::label::{LabelId, LabelMap};
use crate::opcodes::{AsmMsgOp, DirOp, NamedOpcode};
use crate::program::{
    Block, BlockContent, EntryPoint, Located, Operand, OperandType, Operation, Program, Target,
    TypeHint,
};
use crate::span::Spanned;
use crate::{Error, Result};
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
            if let Item::LabelDecl(label) = item {
                if new_block {
                    block_id = self.program.insert_after(Some(block_id), Block::new());
                    // Labels right after each other refer to the same block
                    new_block = false;
                }
                self.program.labels.insert_new(label.name.as_str(), block_id, label.name.span())?;
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
            match item {
                Item::LabelDecl(label) => {
                    let labels = &self.program.labels;
                    let id = labels
                        .find_name(label.name.as_str())
                        .expect("label does not have an associated ID");
                    block_id = labels.get(id).block;
                }
                Item::Command(cmd) => match cmd.name.class() {
                    IdentClass::Default => {
                        let opcode = Self::parse_command_opcode(&cmd.name)?;
                        let command = self.parse_operation(opcode, &cmd.operands)?;
                        block_id = self.process_command(block_id, command);
                    }
                    IdentClass::Directive => {
                        let opcode = Self::parse_directive_opcode(&cmd.name)?;
                        let dir = self.parse_operation(opcode, &cmd.operands)?;
                        block_id = self.process_directive(block_id, dir)?;
                    }
                    IdentClass::Type => return Err(Error::UnexpectedExpr),
                },
                Item::Error => panic!("error item in AST"),
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
        match *dir.opcode {
            DirOp::Globals => {
                if self.program.target.replace(Target::Globals).is_some() {
                    return Err(Error::DuplicateTarget);
                }
            }
            DirOp::Stage => {
                let name_op = dir.operands.get(0).ok_or(Error::ExpectedText)?;
                let Operand::Text(name_text) = &**name_op else { return Err(Error::ExpectedText) };
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
            DirOp::Invalid => (),
        }
        Ok(block_id)
    }

    /// Processes a directive that declares an entry point.
    fn process_entry_point(
        entry_points: &mut HashMap<EntryPoint, BlockId>,
        labels: &LabelMap,
        dir: Located<DirOp>,
        operands: &[Located<Operand>],
    ) -> Result<()> {
        // `.interact` and `.lib` take an argument which comes before the label opcode, so we have
        // to match the opcode to know where the label is
        let (entry_point, label_op) = match *dir {
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
        let block = labels.get(label).block;
        match entry_points.insert(entry_point, block) {
            Some(_) => Err(Error::DuplicateEntryPoint(entry_point)),
            None => Ok(()),
        }
    }

    /// Parses an opcode and operands into an `Operation`.
    fn parse_operation<T: NamedOpcode + TypeHint>(
        &self,
        opcode: Located<T>,
        operands: &[ast::Operand],
    ) -> Result<Operation<T>> {
        let mut operation = Operation::new(opcode);
        operation.operands.reserve(operands.len());
        let ty = opcode.type_hint();
        for operand in operands {
            operation
                .operands
                .push(Located::with_span(self.parse_operand(ty, operand)?, operand.span()));
        }
        Ok(operation)
    }

    /// Parses a single operand.
    fn parse_operand(&self, ty: OperandType, operand: &ast::Operand) -> Result<Operand> {
        match &operand.expr {
            Expr::IntLiteral(i) => Self::parse_integer(ty, i.value()),
            Expr::StrLiteral(s) => Self::parse_text(ty, s),
            Expr::Variable(id) => Self::parse_var_expr(ty, id),
            Expr::LabelRef(label) => self.parse_label(ty, &label.name).map(Operand::Label),
            Expr::ElseLabel(label) => self.parse_label(ty, &label.name).map(Operand::ElseLabel),
            Expr::OffsetRef(off) => Self::parse_offset(ty, off),
            Expr::FunctionCall(call) => self.parse_function(ty, call),
            Expr::Error => panic!("error expr in AST"),
        }
    }

    /// Parses a number of a potentially-unknown type into an operand with a known type.
    fn parse_integer(ty: OperandType, int: IntValue) -> Result<Operand> {
        match int {
            IntValue::I8(x) => i8::try_from(x).map(Operand::I8).map_err(|_| Error::Invalid8(int)),
            IntValue::U8(x) => u8::try_from(x).map(Operand::U8).map_err(|_| Error::Invalid8(int)),
            IntValue::I16(x) => {
                i16::try_from(x).map(Operand::I16).map_err(|_| Error::Invalid16(int))
            }
            IntValue::U16(x) => {
                u16::try_from(x).map(Operand::U16).map_err(|_| Error::Invalid16(int))
            }
            IntValue::I32(x) => Ok(Operand::I32(x)),
            IntValue::U32(x) => Ok(Operand::U32(x)),
            IntValue::IAuto(x) => {
                // If we know what type we're parsing as, then forcibly parse as that type,
                // otherwise find the smallest type which fits
                match ty {
                    OperandType::Byte => Self::parse_integer(ty, IntValue::I8(x)),
                    OperandType::Word => Self::parse_integer(ty, IntValue::I16(x)),
                    OperandType::Dword => Self::parse_integer(ty, IntValue::I32(x)),
                    _ => i8::try_from(x)
                        .map(Operand::I8)
                        .or_else(|_| i16::try_from(x).map(Operand::I16))
                        .or(Ok(Operand::I32(x))),
                }
            }
            IntValue::UAuto(x) => {
                // Same as with IAuto
                match ty {
                    OperandType::Byte => Self::parse_integer(ty, IntValue::U8(x)),
                    OperandType::Word => Self::parse_integer(ty, IntValue::U16(x)),
                    OperandType::Dword => Self::parse_integer(ty, IntValue::U32(x)),
                    _ => u8::try_from(x)
                        .map(Operand::U8)
                        .or_else(|_| u16::try_from(x).map(Operand::U16))
                        .or(Ok(Operand::U32(x))),
                }
            }
            IntValue::Error => panic!("error int in AST"),
        }
    }

    /// Parses a text operand.
    fn parse_text(ty: OperandType, s: &ast::StrLiteral) -> Result<Operand> {
        if !matches!(ty, OperandType::Unknown | OperandType::Byte | OperandType::Message) {
            return Err(Error::OperandTypeExpected(ty));
        }
        let unescaped = s.to_unescaped();
        let encoded = Text::encode(&unescaped)?;
        Ok(Operand::Text(encoded))
    }

    /// Parses a variable reference expression.
    fn parse_var_expr(ty: OperandType, id: &ast::Ident) -> Result<Operand> {
        if !matches!(ty, OperandType::Unknown | OperandType::Dword | OperandType::Message) {
            return Err(Error::OperandTypeExpected(ty));
        }
        Ok(match id.class() {
            IdentClass::Default => {
                if ty == OperandType::Message {
                    let cmd = Operation::new(Self::parse_msg_opcode(id)?);
                    Operand::MsgCommand(cmd.into())
                } else {
                    let expr = Operation::new(Self::parse_expr_opcode(id)?);
                    Operand::Expr(expr.into())
                }
            }
            IdentClass::Directive => return Err(Error::UnexpectedDirective),
            IdentClass::Type => Operand::Type(*Self::parse_type_opcode(id)?),
        })
    }

    /// Parses a label reference operand.
    fn parse_label(&self, ty: OperandType, id: &ast::Ident) -> Result<LabelId> {
        if !matches!(ty, OperandType::Unknown | OperandType::Dword) {
            return Err(Error::OperandTypeExpected(ty));
        }
        self.program
            .labels
            .find_name(id.as_str())
            .ok_or_else(|| Error::UndefinedLabel(id.as_str().into()))
    }

    /// Parses an offset reference operand.
    fn parse_offset(ty: OperandType, offset: &ast::OffsetRef) -> Result<Operand> {
        if !matches!(ty, OperandType::Unknown | OperandType::Dword) {
            return Err(Error::OperandTypeExpected(ty));
        }
        Ok(Operand::Offset(
            Self::parse_integer(OperandType::Unknown, offset.offset.value())?.cast()?,
        ))
    }

    /// Parses a "function" operand, which depending on context may be an expression or message
    /// command.
    fn parse_function(&self, ty: OperandType, call: &ast::FunctionCall) -> Result<Operand> {
        match ty {
            OperandType::Unknown => {
                let opcode = Self::parse_expr_opcode(&call.name)?;
                self.parse_operation(opcode, &call.operands).map(|o| Operand::Expr(o.into()))
            }
            OperandType::Message => {
                let opcode = Self::parse_msg_opcode(&call.name)?;
                self.parse_operation(opcode, &call.operands).map(|o| Operand::MsgCommand(o.into()))
            }
            _ => Err(Error::OperandTypeExpected(ty)),
        }
    }

    /// Parses a command name.
    fn parse_command_opcode(id: &ast::Ident) -> Result<Located<CmdOp>> {
        CmdOp::get(id.as_str())
            .map(|op| Located::with_span(op, id.span()))
            .ok_or_else(|| Error::UnrecognizedCommand(id.as_str().into()))
    }

    /// Parses a directive name.
    fn parse_directive_opcode(id: &ast::Ident) -> Result<Located<DirOp>> {
        DirOp::get(id.as_str())
            .map(|op| Located::with_span(op, id.span()))
            .ok_or_else(|| Error::UnrecognizedDirective(id.as_str().into()))
    }

    /// Parses a @type name.
    fn parse_type_opcode(id: &ast::Ident) -> Result<Located<TypeOp>> {
        TypeOp::get(id.as_str())
            .map(|op| Located::with_span(op, id.span()))
            .ok_or_else(|| Error::UnrecognizedType(id.as_str().into()))
    }

    /// Parses an expression name.
    fn parse_expr_opcode(id: &ast::Ident) -> Result<Located<ExprOp>> {
        ExprOp::get(id.as_str())
            .map(|op| Located::with_span(op, id.span()))
            .ok_or_else(|| Error::UnrecognizedFunction(id.as_str().into()))
    }

    /// Parses a message command name.
    fn parse_msg_opcode(id: &ast::Ident) -> Result<Located<AsmMsgOp>> {
        AsmMsgOp::get(id.as_str())
            .map(|op| Located::with_span(op, id.span()))
            .ok_or_else(|| Error::UnrecognizedFunction(id.as_str().into()))
    }
}
