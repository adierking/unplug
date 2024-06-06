use crate::diagnostics::{CompileOutput, Diagnostic};
use crate::opcodes::{AsmMsgOp, NamedOpcode};
use crate::program::{
    Block, BlockContent, CastOperand, EntryPoint, Located, Operand, Operation, Program, Target,
};
use crate::span::{Span, Spanned};
use crate::{Error, Result};
use smallvec::SmallVec;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::ops::Deref;
use std::rc::Rc;
use std::result::Result as StdResult;
use unplug::common::Text;
use unplug::event::analysis::SubroutineEffectsMap;
use unplug::event::block::Block as ScriptBlock;
use unplug::event::opcodes::{Atom, CmdOp, ExprOp, Ggte, MsgOp, OpcodeMap};
use unplug::event::script::{Error as ScriptError, Script, ScriptLayout};
use unplug::event::serialize::{
    DeserializeEvent, Error as SerError, EventDeserializer, Result as SerResult,
};
use unplug::event::{BlockId, CodeBlock, Command, DataBlock, Pointer};
use unplug::globals::{Libs, NUM_LIBS};
use unplug::stage::Stage;

type SharedDiagnostics = Rc<RefCell<Vec<Diagnostic>>>;

/// Differentiates cursor types and stores the underlying opcode.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum CursorKind {
    Command(CmdOp),
    Expr(ExprOp),
    MsgCommand(AsmMsgOp),
}

impl CursorKind {
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
    fn match_cursor(kind: CursorKind) -> Option<Self>;
    fn error_diagnostic(span: Span) -> Diagnostic;
}

impl MatchCursorKind for CmdOp {
    fn match_cursor(kind: CursorKind) -> Option<Self> {
        let CursorKind::Command(cmd) = kind else { return None };
        Some(cmd)
    }

    fn error_diagnostic(span: Span) -> Diagnostic {
        Diagnostic::expected_command(span)
    }
}

impl MatchCursorKind for ExprOp {
    fn match_cursor(kind: CursorKind) -> Option<Self> {
        let CursorKind::Expr(expr) = kind else { return None };
        Some(expr)
    }

    fn error_diagnostic(span: Span) -> Diagnostic {
        Diagnostic::expected_expr(span)
    }
}

impl MatchCursorKind for AsmMsgOp {
    fn match_cursor(kind: CursorKind) -> Option<Self> {
        let CursorKind::MsgCommand(msg) = kind else { return None };
        Some(msg)
    }

    fn error_diagnostic(span: Span) -> Diagnostic {
        Diagnostic::expected_msg_command(span)
    }
}

/// A list of operands which can be borrowed or owned.
/// Similar to `Cow` but better-suited for operand data.
#[derive(Debug, Clone)]
#[allow(variant_size_differences)]
enum CursorData<'a> {
    Borrowed(&'a [Located<Operand>]),
    Owned(SmallVec<[Located<Operand>; 2]>),
}

impl<'a> CursorData<'a> {
    /// Returns the underlying operand slice iff it is borrowed.
    fn ensure_borrowed(&self) -> Option<&'a [Located<Operand>]> {
        match self {
            Self::Borrowed(operands) => Some(operands),
            Self::Owned(_) => None,
        }
    }
}

impl<'a> Deref for CursorData<'a> {
    type Target = [Located<Operand>];
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
    kind: Located<CursorKind>,
    /// The operand list, which may be generated or borrowed from the program.
    operands: CursorData<'a>,
    /// The position of the cursor in the operand list.
    /// This always counts up from 0, even if the list is reversed.
    position: usize,
    /// True if the operands should be read in reversed order.
    reversed: bool,
    diagnostics: SharedDiagnostics,
}

impl<'a> OperandCursor<'a> {
    /// Creates a cursor over a borrowed operation.
    fn with_borrowed<T>(op: &'a Operation<T>, diagnostics: SharedDiagnostics) -> Self
    where
        T: NamedOpcode + Into<CursorKind>,
    {
        let kind = Located::map(op.opcode, |o| o.into());
        Self {
            kind,
            operands: CursorData::Borrowed(&op.operands),
            position: 0,
            reversed: kind.has_reversed_operands(),
            diagnostics,
        }
    }

    /// Creates a cursor which owns its operation.
    fn with_owned<T>(op: Operation<T>, diagnostics: SharedDiagnostics) -> Self
    where
        T: NamedOpcode + Into<CursorKind>,
    {
        let kind = Located::map(op.opcode, |o| o.into());
        Self {
            kind,
            operands: CursorData::Owned(op.operands),
            position: 0,
            reversed: kind.has_reversed_operands(),
            diagnostics,
        }
    }

    /// Returns the cursor's opcode if it is of type `T`.
    fn opcode<T: MatchCursorKind>(&self) -> Option<T> {
        T::match_cursor(*self.kind)
    }

    /// Returns true if the cursor's opcode is of type `T`.
    fn has_opcode<T: MatchCursorKind>(&self) -> bool {
        self.opcode::<T>().is_some()
    }

    /// Returns true if the cursor has another operand.
    fn has_next(&self) -> bool {
        self.index().is_some()
    }

    /// Returns the next operand (if any) and advances the cursor.
    fn next(&mut self) -> Option<&Located<Operand>> {
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
    fn enter_expr(&mut self) -> (ExprOp, Self) {
        match self.enter_expr_impl() {
            Ok((op, cursor)) => (op, cursor),
            Err(()) => {
                // Synthesize an expression which evaluates to 0
                let opcode = ExprOp::Imm32;
                let op =
                    Operation::with_operands(Located::new(opcode), [Located::new(Operand::I32(0))]);
                (opcode, Self::with_owned(op, Rc::clone(&self.diagnostics)))
            }
        }
    }

    fn enter_expr_impl(&mut self) -> StdResult<(ExprOp, Self), ()> {
        let Some(index) = self.index() else {
            self.report(Diagnostic::not_enough_operands(self.kind.span()));
            return Err(());
        };
        assert!(
            matches!(*self.kind, CursorKind::Command(_) | CursorKind::Expr(_)),
            "this operation does not support expressions"
        );
        let operands = self.operands.ensure_borrowed().expect("operands are not borrowed");
        let operand = &operands[index];
        let (opcode, cursor) = match &**operand {
            Operand::I8(_) | Operand::U8(_) | Operand::I16(_) | Operand::U16(_) => {
                let op = Operation::with_operands(Located::new(ExprOp::Imm16), [operand.clone()]);
                (op.opcode, Self::with_owned(op, Rc::clone(&self.diagnostics)))
            }
            Operand::I32(_) | Operand::U32(_) | Operand::Atom(_) => {
                let op = Operation::with_operands(Located::new(ExprOp::Imm32), [operand.clone()]);
                (op.opcode, Self::with_owned(op, Rc::clone(&self.diagnostics)))
            }
            Operand::Label(_) | Operand::ElseLabel(_) | Operand::Offset(_) => {
                let op =
                    Operation::with_operands(Located::new(ExprOp::AddressOf), [operand.clone()]);
                (op.opcode, Self::with_owned(op, Rc::clone(&self.diagnostics)))
            }
            Operand::Expr(expr) => {
                (expr.opcode, Self::with_borrowed(expr, Rc::clone(&self.diagnostics)))
            }
            Operand::Text(_) | Operand::MsgCommand(_) => {
                self.report(Diagnostic::expected_expr(operand.span()));
                return Err(());
            }
            Operand::Error => {
                return Err(());
            }
        };
        self.position += 1;
        Ok((*opcode, cursor))
    }

    /// Descends into a message command, returning the opcode and subcursor.
    fn enter_msg_command(&mut self) -> (AsmMsgOp, Self) {
        match self.enter_msg_command_impl() {
            Ok((op, cursor)) => (op, cursor),
            Err(()) => {
                // Synthesize an empty string
                let opcode = AsmMsgOp::Text;
                let op = Operation::with_operands(
                    Located::new(opcode),
                    [Located::new(Operand::Text(Text::new()))],
                );
                (opcode, Self::with_owned(op, Rc::clone(&self.diagnostics)))
            }
        }
    }

    fn enter_msg_command_impl(&mut self) -> StdResult<(AsmMsgOp, Self), ()> {
        let Some(index) = self.index() else {
            self.report(Diagnostic::not_enough_operands(self.kind.span()));
            return Err(());
        };
        assert!(
            matches!(*self.kind, CursorKind::Command(_)),
            "this operation does not support message commands"
        );
        let operands = self.operands.ensure_borrowed().expect("operands are not borrowed");
        let operand = &operands[index];
        let (opcode, cursor) = match &**operand {
            Operand::Text(_) => {
                let op = Operation::with_operands(Located::new(AsmMsgOp::Text), [operand.clone()]);
                (op.opcode, Self::with_owned(op, Rc::clone(&self.diagnostics)))
            }
            Operand::MsgCommand(op) => {
                (op.opcode, Self::with_borrowed(op, Rc::clone(&self.diagnostics)))
            }
            Operand::Error => {
                return Err(());
            }
            Operand::I8(_)
            | Operand::U8(_)
            | Operand::I16(_)
            | Operand::U16(_)
            | Operand::I32(_)
            | Operand::U32(_)
            | Operand::Label(_)
            | Operand::ElseLabel(_)
            | Operand::Offset(_)
            | Operand::Atom(_)
            | Operand::Expr(_) => {
                self.report(Diagnostic::expected_msg_command(operand.span()));
                return Err(());
            }
        };
        self.position += 1;
        Ok((*opcode, cursor))
    }

    /// Consumes the cursor, emitting diagnostics if the opcode does not match `T` or not all
    /// operands were consumed.
    fn leave<T: MatchCursorKind>(self) {
        if !self.has_opcode::<T>() {
            self.report(T::error_diagnostic(self.kind.span()));
        }
        if self.has_next() {
            self.report(Diagnostic::too_many_operands(self.kind.span()));
        }
    }

    /// Reports a diagnostic to the shared diagnostic list.
    fn report(&self, diagnostic: Diagnostic) {
        self.diagnostics.borrow_mut().push(diagnostic);
    }
}

impl Spanned for OperandCursor<'_> {
    fn span(&self) -> Span {
        let operands = self.operands.iter().fold(Span::EMPTY, |s, o| s.join(o.span()));
        self.kind.span().join(operands)
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
    diagnostics: SharedDiagnostics,
}

impl<'a> AsmDeserializer<'a> {
    fn new(program: &'a Program, block: &'a Block, diagnostics: SharedDiagnostics) -> Self {
        Self {
            program,
            block,
            command_index: 0,
            cursor: None,
            text: None,
            stack: vec![],
            diagnostics,
        }
    }

    fn has_command(&self) -> bool {
        if let Some(BlockContent::Code(code)) = &self.block.content {
            self.command_index < code.len()
        } else {
            false
        }
    }

    fn next_operand(&mut self) -> Option<&Located<Operand>> {
        self.cursor.as_mut().and_then(|c| c.next())
    }

    fn next_integer<I: CastOperand + Default>(&mut self) -> I {
        let Some(operand) = self.next_operand() else {
            self.report(Diagnostic::expected_integer(self.cursor_span().at_end(0)));
            return I::default();
        };
        if matches!(**operand, Operand::Error) {
            return I::default();
        }
        let span = operand.span();
        match operand.cast() {
            Ok(i) => i,
            Err(Error::ExpectedInteger) => {
                self.report(Diagnostic::expected_integer(span));
                I::default()
            }
            Err(_) => {
                self.report(Diagnostic::integer_conversion(span, I::BITS));
                I::default()
            }
        }
    }

    fn cursor_span(&self) -> Span {
        self.cursor.as_ref().expect("missing cursor").span()
    }

    /// Reports a diagnostic to the shared diagnostic list.
    fn report(&self, diagnostic: Diagnostic) {
        self.diagnostics.borrow_mut().push(diagnostic);
    }
}

impl EventDeserializer for AsmDeserializer<'_> {
    fn deserialize_i8(&mut self) -> SerResult<i8> {
        Ok(self.next_integer())
    }

    fn deserialize_u8(&mut self) -> SerResult<u8> {
        Ok(self.next_integer())
    }

    fn deserialize_i16(&mut self) -> SerResult<i16> {
        Ok(self.next_integer())
    }

    fn deserialize_u16(&mut self) -> SerResult<u16> {
        Ok(self.next_integer())
    }

    fn deserialize_i32(&mut self) -> SerResult<i32> {
        Ok(self.next_integer())
    }

    fn deserialize_u32(&mut self) -> SerResult<u32> {
        Ok(self.next_integer())
    }

    fn deserialize_pointer(&mut self) -> SerResult<Pointer> {
        let Some(operand) = self.next_operand() else {
            self.report(Diagnostic::expected_label_ref(self.cursor_span().at_end(0)));
            return Ok(Pointer::Offset(0));
        };
        let span = operand.span();
        match **operand {
            Operand::Label(label) | Operand::ElseLabel(label) => {
                // TODO: Only allow else labels in conditionals
                Ok(Pointer::Block(self.program.labels.get(label).block))
            }
            Operand::Offset(o) => Ok(Pointer::Offset(o)),
            _ => {
                self.report(Diagnostic::expected_label_ref(span));
                Ok(Pointer::Offset(0))
            }
        }
    }

    fn deserialize_i8_array(&mut self, _len: usize) -> SerResult<Vec<i8>> {
        unimplemented!()
    }

    fn deserialize_u8_array(&mut self, _len: usize) -> SerResult<Vec<u8>> {
        unimplemented!()
    }

    fn deserialize_i16_array(&mut self, _len: usize) -> SerResult<Vec<i16>> {
        unimplemented!()
    }

    fn deserialize_u16_array(&mut self, _len: usize) -> SerResult<Vec<u16>> {
        unimplemented!()
    }

    fn deserialize_i32_array(&mut self, _len: usize) -> SerResult<Vec<i32>> {
        unimplemented!()
    }

    fn deserialize_u32_array(&mut self, _len: usize) -> SerResult<Vec<u32>> {
        unimplemented!()
    }

    fn deserialize_pointer_array(&mut self, _max_len: usize) -> SerResult<Vec<Pointer>> {
        unimplemented!()
    }

    fn deserialize_atom(&mut self) -> SerResult<Atom> {
        let operand = self.next_operand().ok_or(Error::ExpectedInteger)?;
        match **operand {
            Operand::Atom(op) => Ok(op),
            _ => Ggte::get(operand.cast()?).map_err(SerError::UnrecognizedAtom),
        }
    }

    fn deserialize_text(&mut self) -> SerResult<Text> {
        let Some(operand) = self.next_operand() else {
            self.report(Diagnostic::expected_string(self.cursor_span().at_end(0)));
            return Ok(Text::new());
        };
        let span = operand.span();
        match &**operand {
            Operand::Text(text) => Ok(text.clone()), // TODO: Avoid this clone?
            _ => {
                self.report(Diagnostic::expected_string(span));
                Ok(Text::new())
            }
        }
    }

    fn deserialize_rgba(&mut self) -> SerResult<u32> {
        self.deserialize_u32()
    }

    fn begin_expr(&mut self) -> SerResult<ExprOp> {
        let cursor = self.cursor.as_mut().expect("missing cursor");
        let (opcode, subcursor) = cursor.enter_expr();
        self.stack.push(self.cursor.replace(subcursor).unwrap());
        Ok(opcode)
    }

    fn end_expr(&mut self) -> SerResult<()> {
        self.cursor.take().expect("missing cursor").leave::<ExprOp>();
        self.cursor = self.stack.pop();
        Ok(())
    }

    fn begin_command(&mut self) -> SerResult<CmdOp> {
        assert!(self.cursor.is_none(), "already in a command");
        let code = match &self.block.content {
            Some(BlockContent::Code(code)) => code,
            Some(BlockContent::Data(_)) => panic!("not in a code block"),
            _ => return Err(SerError::EndOfData),
        };
        let command = code.get(self.command_index).ok_or(SerError::EndOfData)?;
        self.cursor = Some(OperandCursor::with_borrowed(command, Rc::clone(&self.diagnostics)));
        Ok(*command.opcode)
    }

    fn end_command(&mut self) -> SerResult<()> {
        self.cursor.take().expect("missing cursor").leave::<CmdOp>();
        self.command_index += 1;
        Ok(())
    }

    fn begin_variadic_args(&mut self) -> SerResult<()> {
        Ok(())
    }

    fn have_variadic_arg(&mut self) -> SerResult<bool> {
        let cursor = self.cursor.as_ref().expect("missing cursor");
        Ok(cursor.has_next())
    }

    fn end_variadic_args(&mut self) -> SerResult<()> {
        // End the current message command if there is one
        let cursor = self.cursor.as_mut().expect("missing cursor");
        if cursor.has_opcode::<AsmMsgOp>() {
            self.cursor.replace(self.stack.pop().unwrap()).unwrap().leave::<AsmMsgOp>();
        }
        Ok(())
    }

    fn deserialize_msg_char(&mut self) -> SerResult<MsgOp> {
        // If we're currently iterating over text, see if there are characters left
        let mut cursor = self.cursor.as_mut().expect("missing cursor");
        if let Some(text) = &mut self.text {
            if let Some(b) = text.next() {
                return Ok(b);
            }
            // For format strings, we have to emit a Format byte at the end
            self.text = None;
            if cursor.opcode() == Some(AsmMsgOp::Format) {
                return Ok(MsgOp::Format);
            }
        }

        // If we're currently in a message command, we should be done with it now
        if cursor.has_opcode::<AsmMsgOp>() {
            self.cursor.replace(self.stack.pop().unwrap()).unwrap().leave::<AsmMsgOp>();
            cursor = self.cursor.as_mut().expect("missing cursor");
        }

        // Emit an end byte if we're at the end of the message
        if !cursor.has_next() {
            return Ok(MsgOp::End);
        }

        // Now enter the next message command
        let (opcode, mut subcursor) = cursor.enter_msg_command();

        // For strings, make a text cursor
        if let AsmMsgOp::Format | AsmMsgOp::Text = opcode {
            let operand = subcursor.next().ok_or(Error::ExpectedMessage)?;
            let Operand::Text(text) = &**operand else { return Err(Error::ExpectedText.into()) };
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
            AsmMsgOp::Layout => MsgOp::Layout,
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
            AsmMsgOp::Invalid => MsgOp::Invalid,
        })
    }
}

/// Compiles a code block.
fn compile_code(
    program: &Program,
    block: &Block,
    diagnostics: SharedDiagnostics,
) -> Result<ScriptBlock> {
    let mut code = CodeBlock::new();
    let mut deserializer = AsmDeserializer::new(program, block, diagnostics);
    while deserializer.has_command() {
        let cmd = Command::deserialize(&mut deserializer)
            .map_err(|e| ScriptError::WriteCommand(e.into()))?;
        code.commands.push(cmd);
    }
    if let Some(last) = code.commands.last() {
        if let Some(args) = last.if_args() {
            code.next_block = block.next.map(Pointer::Block);
            code.else_block = Some(args.else_target);
        } else if let Some(next_id) = last.goto_target() {
            code.next_block = Some(*next_id);
        } else if !last.is_control_flow() {
            code.next_block = block.next.map(Pointer::Block);
        }
    } else {
        code.next_block = block.next.map(Pointer::Block);
    }
    Ok(ScriptBlock::Code(code))
}

/// Converts an operand into a data block.
fn into_data(program: &Program, value: &Located<Operand>) -> Result<DataBlock> {
    match &**value {
        Operand::I8(x) => Ok(DataBlock::I8Array(vec![*x])),
        Operand::U8(x) => Ok(DataBlock::U8Array(vec![*x])),
        Operand::I16(x) => Ok(DataBlock::I16Array(vec![*x])),
        Operand::U16(x) => Ok(DataBlock::U16Array(vec![*x])),
        Operand::I32(x) => Ok(DataBlock::I32Array(vec![*x])),
        Operand::U32(x) => Ok(DataBlock::U32Array(vec![*x])),
        Operand::Text(text) => Ok(DataBlock::String(text.clone())),
        Operand::Label(label) => {
            let block = program.labels.get(*label).block;
            Ok(DataBlock::PtrArray(vec![Pointer::Block(block)]))
        }
        Operand::Offset(x) => Ok(DataBlock::PtrArray(vec![Pointer::Offset(*x)])),
        Operand::Atom(x) => Ok(DataBlock::I32Array(vec![Ggte::value(*x).unwrap()])),
        Operand::ElseLabel(_) => Err(Error::UnexpectedElseLabel),
        Operand::Expr(_) => Err(Error::UnexpectedExpr),
        Operand::MsgCommand(_) => Err(Error::UnexpectedMessage),
        Operand::Error => Ok(DataBlock::U8Array(vec![])),
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
            Operand::Atom(x) => array.push(Ggte::value(x).unwrap()),
            _ => return false,
        },
        DataBlock::U32Array(array) => {
            let Operand::U32(x) = *value else { return false };
            array.push(x);
        }
        DataBlock::PtrArray(array) => {
            let ptr = match *value {
                Operand::Label(label) => Pointer::Block(program.labels.get(label).block),
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
fn compile_data(program: &Program, data: &[Located<Operand>]) -> Result<ScriptBlock> {
    let mut blocks: Vec<DataBlock> = vec![];
    for value in data {
        if matches!(**value, Operand::Error) {
            continue;
        }
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
pub fn compile(program: &Program) -> CompileOutput<CompiledScript> {
    // Compile each block individually
    let mut script_blocks = vec![];
    let diagnostics = Rc::new(RefCell::new(vec![]));
    for block in &program.blocks {
        let script_block = match &block.content {
            Some(BlockContent::Code(_)) | None => {
                compile_code(program, block, Rc::clone(&diagnostics)).unwrap()
            }
            Some(BlockContent::Data(data)) => compile_data(program, data).unwrap(),
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
    let script = Script::with_blocks_and_layout(script_blocks, layout);
    let diagnostics = Rc::try_unwrap(diagnostics).unwrap().into_inner();
    if diagnostics.is_empty() {
        CompileOutput::with_result(
            CompiledScript {
                script,
                target: program.target.as_deref().cloned(),
                entry_points: program.entry_points.iter().map(|(&e, &b)| (e, *b)).collect(),
            },
            vec![],
        )
    } else {
        CompileOutput::err(diagnostics)
    }
}

/// Compiled script information.
#[derive(Clone)]
pub struct CompiledScript {
    /// The compiled script data.
    pub script: Script,
    /// The original program's target specifier.
    pub target: Option<Target>,
    /// The original program's entry points.
    pub entry_points: HashMap<EntryPoint, BlockId>,
}

impl CompiledScript {
    /// Makes a global library script.
    pub fn into_libs(self) -> Result<Libs> {
        let mut entry_points: Vec<Option<BlockId>> = vec![None; NUM_LIBS];
        for (entry_point, block) in self.entry_points {
            let EntryPoint::Lib(index) = entry_point else {
                return Err(Error::EventInGlobals);
            };
            if index < 0 || index > NUM_LIBS as i16 {
                return Err(Error::InvalidLibIndex(index));
            }
            entry_points[index as usize] = Some(block);
        }
        Ok(Libs {
            script: self.script,
            entry_points: entry_points
                .into_iter()
                .enumerate()
                .map(|(i, e)| e.ok_or(Error::LibNotDefined(i as i16)))
                .collect::<Result<Vec<_>>>()?
                .into_boxed_slice(),
        })
    }

    /// Makes a stage using non-script data from `base`.
    pub fn into_stage(self, mut base: Stage) -> Result<Stage> {
        base.clear_events();
        base.script = self.script;
        for (entry_point, block) in self.entry_points {
            match entry_point {
                EntryPoint::Event(event) => base
                    .set_event(event, Some(block))
                    .map_err(|_| Error::InvalidStageEvent(event))?,
                EntryPoint::Lib(_) => return Err(Error::LibInStage),
            }
        }
        Ok(base)
    }
}
