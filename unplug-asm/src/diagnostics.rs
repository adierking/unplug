use crate::ast::{self, Else, IntValue, LParen};
use crate::lexer::Token;
use crate::span::{Span, Spanned};
use crate::{Error, Result};
use num_enum::IntoPrimitive;
use std::borrow::Cow;
use std::fmt::{self, Display, Formatter};

/// A label in a diagnostic which pairs a span in the source with an optional tag string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    span: Span,
    tag: Option<Cow<'static, str>>,
}

impl Label {
    /// Creates a new label without a tag.
    pub fn new(span: impl Spanned) -> Self {
        Self { span: span.span(), tag: None }
    }

    /// Creates a new label with a tag.
    pub fn with_tag(span: impl Spanned, tag: impl Into<Cow<'static, str>>) -> Self {
        Self { span: span.span(), tag: Some(tag.into()) }
    }

    /// Returns the label's tag, if any.
    pub fn tag(&self) -> Option<&str> {
        self.tag.as_deref()
    }
}

impl Spanned for Label {
    fn span(&self) -> Span {
        self.span
    }
}

/// A message emitted by a compilation stage. Currently all diagnostics are considered errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    code: DiagnosticCode,
    message: String,
    note: Option<String>,
    labels: Vec<Label>,
}

impl Diagnostic {
    /// Returns a code describing the general error.
    pub fn code(&self) -> DiagnosticCode {
        self.code
    }

    /// Returns true if this diagnostic represents an error.
    pub fn is_err(&self) -> bool {
        matches!(self.code, DiagnosticCode::Error(_))
    }

    /// Returns the message to display.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns a note to display at the end, if any.
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// Returns the message's labels.
    pub fn labels(&self) -> &[Label] {
        &self.labels
    }
}

impl Spanned for Diagnostic {
    fn span(&self) -> Span {
        self.labels.first().map(|l| l.span()).unwrap_or_default()
    }
}

/// Output from a compilation stage including a potential result and any emitted diagnostics.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct CompileOutput<T> {
    /// The compilation result. If this is present, the stage either succeeded or produced a partial
    /// result. If this is not present, the stage failed.
    pub result: Option<T>,
    /// Diagnostics that were emitted during the stage.
    pub diagnostics: Vec<Diagnostic>,
}

impl<T> CompileOutput<T> {
    /// Creates an output with a result.
    pub fn with_result(result: T, diagnostics: Vec<Diagnostic>) -> Self {
        Self { result: Some(result), diagnostics }
    }

    /// Creates an error output without a result.
    pub fn err(diagnostics: Vec<Diagnostic>) -> Self {
        Self { result: None, diagnostics }
    }

    /// Returns true if the output has a result.
    pub fn has_result(&self) -> bool {
        self.result.is_some()
    }

    /// Returns true if the output did not produce a result.
    pub fn is_err(&self) -> bool {
        self.result.is_none()
    }

    /// Consumes the output and returns the inner value, discarding the diagnostics.
    /// ***Panics*** if the result was not successful.
    pub fn unwrap(self) -> T {
        self.result.unwrap()
    }

    /// Consumes the output and maps it to a `Result`, discarding the diagnostics.
    pub fn try_unwrap(self) -> Result<T> {
        self.result.ok_or(Error::CompileFailed)
    }
}

/// A diagnostic code.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum DiagnosticCode {
    Warning(WarningCode),
    Error(ErrorCode),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, IntoPrimitive)]
#[repr(u32)]
pub enum WarningCode {
    Reserved,
    SignExtended,
}

/// General diagnostic codes.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, IntoPrimitive)]
#[repr(u32)]
pub enum ErrorCode {
    // TODO: Define fixed code numbers before stabilization
    InternalError,
    InvalidToken,
    IntegerOutOfRange,
    IntegerConversion,
    UnterminatedString,
    UnterminatedComment,
    MissingDeref,
    MissingDerefTarget,
    UnclosedParenthesis,
    MissingComma,
    NotEnoughOperands,
    ExpectedNewline,
    ExpectedExpr,
    ExpectedCommand,
    ExpectedMsgCommand,
    ExpectedItem,
    ExpectedInteger,
    ExpectedString,
    ExpectedLabelRef,
    ExpectedIdent,
    TooManyOperands,
    UnexpectedToken,
    UnexpectedExpr,
    UnexpectedMsgCommand,
    UnexpectedValueName,
    UnexpectedString,
    UnexpectedLabelRef,
    UnexpectedElseLabel,
    UnexpectedOffset,
    UnexpectedFunction,
    UndefinedLabel,
    DuplicateLabel,
    DuplicateTarget,
    MissingStageName,
    MissingLibIndex,
    DuplicateEntryPoint,
    MissingEntryPointSubroutine,
    StageEventInGlobals,
    LibInStage,
    UndefinedLib,
    MissingEventObject,
    InvalidEventObject,
    UnrecognizedCommand,
    UnrecognizedDirective,
    UnrecognizedAtom,
    UnrecognizedFunction,
    UnrecognizedMsgCommand,
    UnsupportedCommand,
    UnsupportedAtom,
    UnsupportedFunction,
    UnsupportedMsgCommand,
}

impl From<WarningCode> for DiagnosticCode {
    fn from(value: WarningCode) -> Self {
        Self::Warning(value)
    }
}

impl From<ErrorCode> for DiagnosticCode {
    fn from(value: ErrorCode) -> Self {
        Self::Error(value)
    }
}

impl Display for DiagnosticCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Warning(code) => write!(f, "W{:03}", u32::from(code)),
            Self::Error(code) => write!(f, "E{:03}", u32::from(code)),
        }
    }
}

/// Macro for quickly declaring diagnostic constructors.
macro_rules! diagnostics {
    {
        $(
            $fn_name:ident ( $( $key:ident : $type:ty),* ) {
                code : $code:expr $(,)+
                message : $message:literal $(,)+
                $( note : $note:literal $(,)+ )?
                labels : [ $( $span:ident $(-> $tag:literal)? ),* $(,)* ] $(,)*
            }
        )*
    } => {
        impl Diagnostic {
            $(
                pub fn $fn_name ( $($key : $type),* ) -> Self {
                    Self {
                        code: $code.into(),
                        message: format!($message),
                        note: diagnostics!(@note $($note)?),
                        labels: vec![ $( diagnostics!(@label $span $(-> $tag)?) ),* ],
                    }
                }
            )*
        }
    };

    (@note $note:literal) => { Some(format!($note)) };
    (@note) => { None };

    (@label $span:ident -> $tag:literal) => { Label::with_tag($span, format!($tag)) };
    (@label $span:ident) => { Label::new($span) };
}

diagnostics! {
    // === Warnings ===

    sign_extended(span: Span, actual: IntValue) {
        code: WarningCode::SignExtended,
        message: "value will be sign-extended and interpreted as {actual}",
        labels: [span -> "remove the type suffix or use {actual}"],
    }

    // === Errors ===

    internal_error(span: Span, message: &str) {
        code: ErrorCode::InternalError,
        message: "internal error: {message}",
        labels: [span],
    }

    invalid_token(span: Span) {
        code: ErrorCode::InvalidToken,
        message: "invalid token",
        labels: [span -> "delete this"],
    }

    integer_out_of_range(span: Span) {
        code: ErrorCode::IntegerOutOfRange,
        message: "integer literal out of range",
        note: "integers cannot exceed 32 bits",
        labels: [span],
    }

    integer_conversion(span: Span, bits: usize) {
        code: ErrorCode::IntegerConversion,
        message: "integer cannot fit in {bits} bits",
        labels: [span],
    }

    unterminated_string(span: Span) {
        code: ErrorCode::UnterminatedString,
        message: "unterminated string literal",
        note: "add a `\"` at the end of the string",
        labels: [span],
    }

    unterminated_comment(start: Span) {
        code: ErrorCode::UnterminatedComment,
        message: "unterminated block comment",
        note: "add a `*/` at the end of the comment",
        labels: [start],
    }

    missing_deref(else_token: Else) {
        code: ErrorCode::MissingDeref,
        message: "missing `*` after `else`",
        labels: [else_token],
    }

    missing_deref_target(deref_token: Span) {
        code: ErrorCode::MissingDerefTarget,
        message: "missing label or offset after `*`",
        note: "add a label or number, e.g. `*my_label`, `*0x10`",
        labels: [deref_token],
    }

    unclosed_parenthesis(lparen_token: LParen, suggested: Span) {
        code: ErrorCode::UnclosedParenthesis,
        message: "unclosed parenthesis",
        labels: [
            lparen_token,
            suggested -> "try adding a `)` here",
        ],
    }

    missing_comma(span: Span) {
        code: ErrorCode::MissingComma,
        message: "missing `,` after operand",
        labels: [span -> "try adding a `,` here"],
    }

    not_enough_operands(command: Span) {
        code: ErrorCode::NotEnoughOperands,
        message: "not enough operands for command",
        labels: [command],
    }

    expected_newline(span: Span) {
        code: ErrorCode::ExpectedNewline,
        message: "expected a newline after the operands",
        labels: [span],
    }

    expected_expr(span: Span) {
        code: ErrorCode::ExpectedExpr,
        message: "expected an expression",
        labels: [span],
    }

    expected_command(span: Span) {
        code: ErrorCode::ExpectedCommand,
        message: "expected a command",
        labels: [span],
    }

    expected_msg_command(span: Span) {
        code: ErrorCode::ExpectedMsgCommand,
        message: "expected a message command",
        labels: [span],
    }

    expected_item(span: Span) {
        code: ErrorCode::ExpectedItem,
        message: "expected a command, directive, or label declaration",
        labels: [span],
    }

    expected_integer(span: Span) {
        code: ErrorCode::ExpectedInteger,
        message: "expected an integer literal",
        labels: [span],
    }

    expected_string(span: Span) {
        code: ErrorCode::ExpectedString,
        message: "expected a string literal",
        labels: [span],
    }

    expected_label_ref(span: Span) {
        code: ErrorCode::ExpectedLabelRef,
        message: "expected a label reference",
        labels: [span],
    }

    expected_ident(span: Span) {
        code: ErrorCode::ExpectedIdent,
        message: "expected an identifier",
        labels: [span],
    }

    too_many_operands(command: Span) {
        code: ErrorCode::TooManyOperands,
        message: "too many operands for command",
        labels: [command],
    }

    unexpected_token(token: &Token, span: impl Spanned) {
        code: ErrorCode::UnexpectedToken,
        message: "unexpected {token}",
        labels: [span -> "delete this"],
    }

    unexpected_expr(expr: Span) {
        code: ErrorCode::UnexpectedExpr,
        message: "unexpected expression",
        labels: [expr],
    }

    unexpected_msg_command(command: Span) {
        code: ErrorCode::UnexpectedMsgCommand,
        message: "unexpected message command",
        labels: [command],
    }

    unexpected_value_name(value: Span) {
        code: ErrorCode::UnexpectedValueName,
        message: "unexpected value name",
        labels: [value],
    }

    unexpected_string_literal(literal: Span) {
        code: ErrorCode::UnexpectedString,
        message: "unexpected string literal",
        labels: [literal],
    }

    unexpected_label_ref(label: Span) {
        code: ErrorCode::UnexpectedLabelRef,
        message: "unexpected label reference",
        labels: [label],
    }

    unexpected_else_label(label: Span) {
        code: ErrorCode::UnexpectedElseLabel,
        message: "unexpected `else` label reference",
        labels: [label],
    }

    unexpected_offset_ref(offset: Span) {
        code: ErrorCode::UnexpectedOffset,
        message: "unexpected offset reference",
        labels: [offset],
    }

    unexpected_function_call(func: Span) {
        code: ErrorCode::UnexpectedFunction,
        message: "unexpected function call",
        labels: [func],
    }

    undefined_label(name: &ast::Ident) {
        code: ErrorCode::UndefinedLabel,
        message: "undefined label: `{name}`",
        labels: [name],
    }

    duplicate_label(name: &ast::Ident, prev: Span) {
        code: ErrorCode::DuplicateLabel,
        message: "label `{name}` is declared more than once",
        labels: [
            name -> "give this a unique name",
            prev -> "previously declared here",
        ],
    }

    duplicate_target(this: Span, prev: Span) {
        code: ErrorCode::DuplicateTarget,
        message: "duplicate target specifier",
        labels: [
            this -> "this conflicts",
            prev -> "with this",
        ],
    }

    missing_stage_name(specifier: Span) {
        code: ErrorCode::MissingStageName,
        message: "target specifier is missing a stage name",
        note: "add a stage file name, e.g. `.stage \"stage07\"`",
        labels: [specifier],
    }

    missing_lib_index(span: Span) {
        code: ErrorCode::MissingLibIndex,
        message: "library function declaration is missing an index",
        note: "add an index first, e.g. `.lib 123, *my_lib`",
        labels: [span],
    }

    duplicate_entry_point(this: Span, prev: Span) {
        code: ErrorCode::DuplicateEntryPoint,
        message: "duplicate entry point",
        labels: [
            this -> "this conflicts",
            prev -> "with this",
        ],
    }

    missing_entry_point_subroutine(span: Span) {
        code: ErrorCode::MissingEntryPointSubroutine,
        message: "missing entry point subroutine",
        note: "add a label reference, e.g. `*evt_startup`",
        labels: [span],
    }

    stage_event_in_globals(decl: &ast::Command) {
        code: ErrorCode::StageEventInGlobals,
        message: "globals scripts cannot define stage events",
        note: "only `.lib` entry points are permitted",
        labels: [decl],
    }

    lib_in_stage(decl: &ast::Command) {
        code: ErrorCode::LibInStage,
        message: "stage scripts cannot define library functions",
        note: "`.lib` entry points are not permitted",
        labels: [decl],
    }

    undefined_lib(index: i16) {
        code: ErrorCode::UndefinedLib,
        message: "library function is not defined: {index}",
        note: "declare it with `.lib {index}, *label`",
        labels: [],
    }

    missing_event_object(span: Span) {
        code: ErrorCode::MissingEventObject,
        message: "interaction event is missing an object index",
        note: "add an object index first, e.g. `.interact 123, *label`",
        labels: [span],
    }

    invalid_event_object(index: i32) {
        code: ErrorCode::InvalidEventObject,
        message: "interaction event has an invalid object index: {index}",
        labels: [],
    }

    unrecognized_command(ident: &ast::Ident) {
        code: ErrorCode::UnrecognizedCommand,
        message: "unrecognized command: `{ident}`",
        labels: [ident],
    }

    unrecognized_directive(ident: &ast::Ident) {
        code: ErrorCode::UnrecognizedDirective,
        message: "unrecognized directive: `{ident}`",
        labels: [ident],
    }

    unrecognized_atom(ident: &ast::Ident) {
        code: ErrorCode::UnrecognizedAtom,
        message: "unrecognized atom: `{ident}`",
        labels: [ident],
    }

    unrecognized_function(ident: &ast::Ident) {
        code: ErrorCode::UnrecognizedFunction,
        message: "unrecognized function: `{ident}`",
        labels: [ident],
    }

    unrecognized_msg_command(ident: &ast::Ident) {
        code: ErrorCode::UnrecognizedMsgCommand,
        message: "unrecognized message command: `{ident}`",
        labels: [ident],
    }

    unsupported_command(ident: &ast::Ident) {
        code: ErrorCode::UnsupportedCommand,
        message: "command is not supported by the target game: `{ident}`",
        labels: [ident],
    }

    unsupported_atom(ident: &ast::Ident) {
        code: ErrorCode::UnsupportedAtom,
        message: "atom is not supported by the target game: `{ident}`",
        labels: [ident],
    }

    unsupported_function(ident: &ast::Ident) {
        code: ErrorCode::UnsupportedFunction,
        message: "function is not supported by the target game: `{ident}`",
        labels: [ident],
    }

    unsupported_msg_command(ident: &ast::Ident) {
        code: ErrorCode::UnsupportedMsgCommand,
        message: "message command is not supported by the target game: `{ident}`",
        labels: [ident],
    }
}
