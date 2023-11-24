use crate::ast::{self, Ast, Expr, IdentClass, IntValue, Item};
use crate::diagnostics::{CompileOutput, Diagnostic};
use crate::label::LabelId;
use crate::opcodes::{DirOp, NamedOpcode};
use crate::program::{
    Block, BlockContent, CastOperand, EntryPoint, Located, Operand, OperandType, Operation,
    Program, Target, TypeHint,
};
use crate::span::Spanned;
use crate::Error;
use std::borrow::Cow;
use unplug::common::Text;
use unplug::event::opcodes::CmdOp;
use unplug::event::BlockId;
use unplug::stage::Event;

/// Assembles a `Program` from an AST.
pub struct ProgramAssembler<'a> {
    ast: &'a Ast,
    program: Program,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> ProgramAssembler<'a> {
    /// Creates a new `ProgramAssembler` that parses `ast`.
    pub fn new(ast: &'a Ast) -> Self {
        Self { ast, program: Program::new(), diagnostics: vec![] }
    }

    /// Parses the AST and assembles a `Program`.
    pub fn assemble(mut self) -> CompileOutput<Program> {
        if !self.ast.items.is_empty() {
            self.scan_labels();
            self.parse_instructions();
            self.split_blocks();
            self.prune_blocks();
            // Ensure blocks have the correct flags. This isn't strictly necessary for compilation,
            // but it allows the program to be written back out using `ProgramWriter`.
            self.program.mark_subroutines();
        }
        if self.diagnostics.is_empty() {
            CompileOutput::with_result(self.program, self.diagnostics)
        } else {
            CompileOutput::err(self.diagnostics)
        }
    }

    /// Scans for labels and defines blocks for them.
    fn scan_labels(&mut self) {
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
                let name = &label.name;
                if self.program.labels.insert_new(name.as_str(), block_id, name.span()).is_err() {
                    let prev_id = self.program.labels.find_name(name.as_str()).unwrap();
                    let prev = self.program.labels.get(prev_id);
                    self.report(Diagnostic::duplicate_label(name, prev.span));
                }
            } else {
                // There's content in this block, so the next label starts a new one
                new_block = true;
            }
        }
    }

    /// Parses instructions into blocks.
    fn parse_instructions(&mut self) {
        let mut block_id = self.program.first_block.unwrap();
        for item in &self.ast.items {
            match item {
                Item::LabelDecl(label) => {
                    let labels = &self.program.labels;
                    if let Some(id) = labels.find_name(label.name.as_str()) {
                        block_id = labels.get(id).block;
                    }
                }

                Item::Command(cmd) => match cmd.name.class() {
                    IdentClass::Default => {
                        let opcode = self.parse_opcode(&cmd.name, Diagnostic::unrecognized_command);
                        let command = self.parse_operation(opcode, &cmd.operands);
                        block_id = self.process_command(block_id, command);
                    }

                    IdentClass::Directive => {
                        let opcode =
                            self.parse_opcode(&cmd.name, Diagnostic::unrecognized_directive);
                        let dir = self.parse_operation(opcode, &cmd.operands);
                        match self.process_directive(block_id, dir) {
                            Ok(next) => block_id = next,
                            Err(dir) => {
                                // Replace the directive with an invalid command to indicate to
                                // later stages that the directive failed
                                let command = dir.with_opcode(CmdOp::Invalid);
                                block_id = self.process_command(block_id, command);
                            }
                        }
                    }

                    IdentClass::Type => {
                        self.report(Diagnostic::expected_command(cmd.name.span()));
                        // Treat this as a command with an invalid opcode
                        let opcode = Located::with_span(CmdOp::Invalid, cmd.name.span());
                        let command = self.parse_operation(opcode, &cmd.operands);
                        block_id = self.process_command(block_id, command);
                    }
                },

                Item::Error => {
                    // Treat this as a command with an invalid opcode
                    let opcode = Located::with_span(CmdOp::Invalid, item.span());
                    block_id = self.process_command(block_id, Operation::new(opcode));
                }
            }
        }
    }

    /// Splits code blocks so that they always end with a control-flow statement.
    fn split_blocks(&mut self) {
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
    /// block is not a data block, it will be split. On success, returns the ID of the block the
    /// directive was actually processed in, and on failure, returns the directive that was passed.
    fn process_directive(
        &mut self,
        mut block_id: BlockId,
        dir: Operation<DirOp>,
    ) -> Result<BlockId, Operation<DirOp>> {
        match *dir.opcode {
            DirOp::Globals => {
                let target = Located::with_span(Target::Globals, dir.span());
                if let Some(prev) = self.program.target.replace(target) {
                    self.report(Diagnostic::duplicate_target(dir.span(), prev.span()));
                    return Err(dir);
                }
            }

            DirOp::Stage => {
                let Some(name_op) = dir.operands.get(0) else {
                    self.report(Diagnostic::missing_stage_name(dir.span()));
                    return Err(dir);
                };
                let Ok(name) = self.expect_string(name_op) else {
                    return Err(dir);
                };
                let target = Located::with_span(Target::Stage(name.into_owned()), dir.span());
                if let Some(prev) = self.program.target.replace(target) {
                    self.report(Diagnostic::duplicate_target(dir.span(), prev.span()));
                    return Err(dir);
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
            | DirOp::Lib => {
                if self.process_entry_point(&dir).is_err() {
                    return Err(dir);
                }
            }

            DirOp::Invalid => return Err(dir),
        }
        Ok(block_id)
    }

    /// Processes a directive that declares an entry point and returns whether it was successful.
    fn process_entry_point(&mut self, dir: &Operation<DirOp>) -> Result<(), ()> {
        // `.interact` and `.lib` take an argument which comes before the label opcode, so we have
        // to match the opcode to know where the label is
        let (entry_point, label_op) = match *dir.opcode {
            DirOp::Prologue => (EntryPoint::Event(Event::Prologue), dir.operands.get(0)),
            DirOp::Startup => (EntryPoint::Event(Event::Startup), dir.operands.get(0)),
            DirOp::Dead => (EntryPoint::Event(Event::Dead), dir.operands.get(0)),
            DirOp::Pose => (EntryPoint::Event(Event::Pose), dir.operands.get(0)),
            DirOp::TimeCycle => (EntryPoint::Event(Event::TimeCycle), dir.operands.get(0)),
            DirOp::TimeUp => (EntryPoint::Event(Event::TimeUp), dir.operands.get(0)),
            DirOp::Interact => {
                let Some(object) = dir.operands.get(0) else {
                    self.report(Diagnostic::missing_event_object(dir.span()));
                    return Err(());
                };
                let index = self.expect_integer(object)?;
                (EntryPoint::Event(Event::Interact(index)), dir.operands.get(1))
            }
            DirOp::Lib => {
                let Some(num) = dir.operands.get(0) else {
                    self.report(Diagnostic::missing_lib_index(dir.span()));
                    return Err(());
                };
                (EntryPoint::Lib(self.expect_integer(num)?), dir.operands.get(1))
            }
            _ => panic!("directive is not an event: {:?}", dir),
        };
        let Some(label_op) = label_op else {
            self.report(Diagnostic::missing_entry_point_subroutine(dir.span()));
            return Err(());
        };
        let label = self.expect_label(label_op)?;
        let block = self.program.labels.get(label).block;
        let located = Located::with_span(block, dir.span());
        if let Some(existing) = self.program.entry_points.insert(entry_point, located) {
            self.report(Diagnostic::duplicate_entry_point(dir.span(), existing.span()));
            Err(())
        } else {
            Ok(())
        }
    }

    /// Parses an opcode and operands into an `Operation`.
    fn parse_operation<T: NamedOpcode + TypeHint>(
        &mut self,
        opcode: Located<T>,
        operands: &[ast::Operand],
    ) -> Operation<T> {
        let mut operation = Operation::new(opcode);
        operation.operands.reserve(operands.len());
        let hint = opcode.type_hint();
        for operand in operands {
            operation.operands.push(self.parse_operand(operand, hint));
        }
        operation
    }

    /// Parses a single operand.
    fn parse_operand(&mut self, operand: &ast::Operand, hint: OperandType) -> Located<Operand> {
        let result = match &operand.expr {
            Expr::IntLiteral(i) => self.parse_integer(*i, hint),
            Expr::StrLiteral(s) => self.parse_text(s, hint),
            Expr::Variable(id) => self.parse_var_expr(id, hint),
            Expr::LabelRef(label) => self.parse_label(label, hint),
            Expr::ElseLabel(label) => self.parse_else_label(label, hint),
            Expr::OffsetRef(off) => self.parse_offset(off, hint),
            Expr::FunctionCall(call) => self.parse_function(call, hint),
            Expr::Error => Err(()),
        };
        Located::with_span(result.unwrap_or(Operand::Error), operand.expr.span())
    }

    /// Parses a number of a potentially-unknown type into an operand with a known type.
    /// Returns `None` if parsing failed.
    fn parse_integer(&mut self, int: ast::IntLiteral, hint: OperandType) -> Result<Operand, ()> {
        let (bits, operand) = match int.value() {
            IntValue::I8(x) => (8, i8::try_from(x).map(Operand::I8).ok()),
            IntValue::U8(x) => (8, u8::try_from(x).map(Operand::U8).ok()),

            IntValue::I16(x) => (16, i16::try_from(x).map(Operand::I16).ok()),
            IntValue::U16(x) => (16, u16::try_from(x).map(Operand::U16).ok()),

            IntValue::I32(x) => (32, Some(Operand::I32(x))),
            IntValue::U32(x) => (32, Some(Operand::U32(x))),

            IntValue::IAuto(x) => {
                // If we know what type we're parsing as, then forcibly parse as that type,
                // otherwise find the smallest type which fits
                match hint {
                    OperandType::Byte => {
                        return self.parse_integer(int.with_value(IntValue::I8(x)), hint);
                    }
                    OperandType::Word => {
                        return self.parse_integer(int.with_value(IntValue::I16(x)), hint);
                    }
                    OperandType::Dword => {
                        return self.parse_integer(int.with_value(IntValue::I32(x)), hint);
                    }
                    _ => {
                        return Ok(i8::try_from(x)
                            .map(Operand::I8)
                            .or_else(|_| i16::try_from(x).map(Operand::I16))
                            .unwrap_or(Operand::I32(x)));
                    }
                }
            }

            IntValue::UAuto(x) => {
                // Same as with IAuto
                match hint {
                    OperandType::Byte => {
                        return self.parse_integer(int.with_value(IntValue::U8(x)), hint);
                    }
                    OperandType::Word => {
                        return self.parse_integer(int.with_value(IntValue::U16(x)), hint);
                    }
                    OperandType::Dword => {
                        return self.parse_integer(int.with_value(IntValue::U32(x)), hint);
                    }
                    _ => {
                        return Ok(u8::try_from(x)
                            .map(Operand::U8)
                            .or_else(|_| u16::try_from(x).map(Operand::U16))
                            .unwrap_or(Operand::U32(x)));
                    }
                }
            }

            IntValue::Error => return Err(()),
        };

        if operand.is_none() {
            self.report(Diagnostic::integer_conversion(int.span(), bits));
        }
        operand.ok_or(())
    }

    /// Parses a text operand.
    fn parse_text(&mut self, s: &ast::StrLiteral, hint: OperandType) -> Result<Operand, ()> {
        if !matches!(hint, OperandType::Unknown | OperandType::Byte | OperandType::Message) {
            self.report(Diagnostic::unexpected_string_literal(s.span()));
            return Err(());
        }
        let unescaped = s.to_unescaped();
        let encoded = Text::encode(&unescaped).unwrap(); // TODO: handle encoding errors
        Ok(Operand::Text(encoded))
    }

    /// Parses a variable reference expression.
    fn parse_var_expr(&mut self, id: &ast::Ident, hint: OperandType) -> Result<Operand, ()> {
        if !matches!(hint, OperandType::Unknown | OperandType::Dword | OperandType::Message) {
            self.report(Diagnostic::unexpected_value_name(id.span()));
            return Err(());
        }
        match id.class() {
            IdentClass::Default => {
                if hint == OperandType::Message {
                    let cmd =
                        Operation::new(self.parse_opcode(id, Diagnostic::unrecognized_msg_command));
                    Ok(Operand::MsgCommand(cmd.into()))
                } else {
                    let expr =
                        Operation::new(self.parse_opcode(id, Diagnostic::unrecognized_function));
                    Ok(Operand::Expr(expr.into()))
                }
            }
            IdentClass::Directive => {
                self.report(Diagnostic::expected_expr(id.span()));
                Err(())
            }
            IdentClass::Type => {
                Ok(Operand::Type(*self.parse_opcode(id, Diagnostic::unrecognized_type)))
            }
        }
    }

    /// Parses a label reference operand.
    fn parse_label(&mut self, label: &ast::LabelRef, hint: OperandType) -> Result<Operand, ()> {
        if !matches!(hint, OperandType::Unknown | OperandType::Dword) {
            self.report(Diagnostic::unexpected_label_ref(label.span()));
            return Err(());
        }
        if let Some(id) = self.program.labels.find_name(label.name.as_str()) {
            Ok(Operand::Label(id))
        } else {
            self.report(Diagnostic::undefined_label(&label.name));
            Err(())
        }
    }

    /// Parses an else label reference operand.
    fn parse_else_label(
        &mut self,
        label: &ast::ElseLabel,
        hint: OperandType,
    ) -> Result<Operand, ()> {
        if !matches!(hint, OperandType::Unknown | OperandType::Dword) {
            self.report(Diagnostic::unexpected_else_label(label.span()));
            return Err(());
        }
        if let Some(id) = self.program.labels.find_name(label.name.as_str()) {
            Ok(Operand::ElseLabel(id))
        } else {
            self.report(Diagnostic::undefined_label(&label.name));
            Err(())
        }
    }

    /// Parses an offset reference operand.
    fn parse_offset(&mut self, offset: &ast::OffsetRef, hint: OperandType) -> Result<Operand, ()> {
        if !matches!(hint, OperandType::Unknown | OperandType::Dword) {
            self.report(Diagnostic::unexpected_offset_ref(offset.span()));
        }
        let value = self.parse_integer(offset.offset, OperandType::Unknown)?;
        let located = Located::with_span(value, offset.span());
        Ok(Operand::Offset(self.expect_integer(&located)?))
    }

    /// Parses a "function" operand, which depending on context may be an expression or message
    /// command.
    fn parse_function(
        &mut self,
        call: &ast::FunctionCall,
        hint: OperandType,
    ) -> Result<Operand, ()> {
        match hint {
            OperandType::Unknown => {
                let opcode = self.parse_opcode(&call.name, Diagnostic::unrecognized_function);
                Ok(Operand::Expr(self.parse_operation(opcode, &call.operands).into()))
            }
            OperandType::Message => {
                let opcode = self.parse_opcode(&call.name, Diagnostic::unrecognized_msg_command);
                Ok(Operand::MsgCommand(self.parse_operation(opcode, &call.operands).into()))
            }
            _ => {
                self.report(Diagnostic::unexpected_function_call(call.span()));
                Err(())
            }
        }
    }

    /// Parses an opcode of any type from `id`. If parsing fails, `error` is invoked to obtain a
    /// diagnostic to report and this will return an invalid opcode.
    fn parse_opcode<T, F>(&mut self, id: &ast::Ident, error: F) -> Located<T>
    where
        T: NamedOpcode,
        F: FnOnce(&ast::Ident) -> Diagnostic,
    {
        let op = T::get(id.as_str()).unwrap_or_default();
        if op == T::default() {
            self.report(error(id));
        }
        Located::with_span(op, id.span())
    }

    /// If an operand is a string, extracts it, otherwise reports a diagnostic and returns an error.
    fn expect_string<'s>(&mut self, operand: &'s Located<Operand>) -> Result<Cow<'s, str>, ()> {
        match &**operand {
            Operand::Text(s) => Ok(s.decode().unwrap()),
            Operand::Error => Err(()),
            _ => {
                self.report(Diagnostic::expected_string(operand.span()));
                Err(())
            }
        }
    }

    /// If an operand is a label reference, extracts it, otherwise reports a diagnostic and returns
    /// an error.
    fn expect_label(&mut self, operand: &Located<Operand>) -> Result<LabelId, ()> {
        match &**operand {
            Operand::Label(id) => Ok(*id),
            Operand::Error => Err(()),
            _ => {
                self.report(Diagnostic::expected_label_ref(operand.span()));
                Err(())
            }
        }
    }

    /// If an operand is an integer, extracts it and casts it to `I`. If the operand is not an
    /// integer or it cannot be narrowed, reports a diagnostic and returns an error.
    fn expect_integer<I: CastOperand>(&mut self, operand: &Located<Operand>) -> Result<I, ()> {
        if matches!(**operand, Operand::Error) {
            return Err(());
        }
        match operand.cast() {
            Ok(i) => Ok(i),
            Err(Error::ExpectedInteger) => {
                self.report(Diagnostic::expected_integer(operand.span()));
                Err(())
            }
            Err(_) => {
                self.report(Diagnostic::integer_conversion(operand.span(), I::BITS));
                Err(())
            }
        }
    }

    /// Reports a diagnostic.
    fn report(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }
}
