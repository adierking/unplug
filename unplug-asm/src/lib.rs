#![warn(
    absolute_paths_not_starting_with_crate,
    elided_lifetimes_in_paths,
    explicit_outlives_requirements,
    trivial_casts,
    trivial_numeric_casts,
    unconditional_recursion,
    unreachable_patterns,
    unreachable_pub,
    unused_import_braces,
    unused_lifetimes,
    unused_must_use,
    unused_qualifications,
    variant_size_differences
)]

pub mod assembler;
pub mod compiler;
pub mod label;
pub mod lexer;
pub mod opcodes;
pub mod parser;
pub mod program;
pub mod writer;

pub use compiler::compile;

use lexer::Number;
use program::{EntryPoint, OperandType};
use smol_str::SmolStr;
use std::io;
use std::sync::Arc;
use thiserror::Error;
use unplug::common::text;
use unplug::event::command;
use unplug::event::script;
use unplug::event::serialize;
use unplug::from_error_boxed;

/// The result type for ASM operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for ASM operations.
#[derive(Debug, Error)]
#[non_exhaustive]
#[allow(variant_size_differences)]
pub enum Error {
    #[error("{0} cannot be converted to an 8-bit signed integer")]
    CannotConvertToI8(Number),

    #[error("{0} cannot be converted to an 8-bit unsigned integer")]
    CannotConvertToU8(Number),

    #[error("{0} cannot be converted to a 16-bit signed integer")]
    CannotConvertToI16(Number),

    #[error("{0} cannot be converted to a 16-bit unsigned integer")]
    CannotConvertToU16(Number),

    #[error("{0} cannot be converted to a 32-bit signed integer")]
    CannotConvertToI32(Number),

    #[error("{0} cannot be converted to a 32-bit unsigned integer")]
    CannotConvertToU32(Number),

    #[error("entry point is defined more than once: {0:?}")]
    DuplicateEntryPoint(EntryPoint),

    #[error("label is defined more than once: \"{0}\"")]
    DuplicateLabel(Arc<str>),

    #[error("the program target is defined more than once")]
    DuplicateTarget,

    #[error("expected a command")]
    ExpectedCommand,

    #[error("expected an expression")]
    ExpectedExpr,

    #[error("expected an integer")]
    ExpectedInteger,

    #[error("expected a label")]
    ExpectedLabel,

    #[error("expected a message operand")]
    ExpectedMessage,

    #[error("expected an object index")]
    ExpectedObjectIndex,

    #[error("expected a text string")]
    ExpectedText,

    #[error("invalid 8-bit integer: {0}")]
    Invalid8(Number),

    #[error("invalid 16-bit integer: {0}")]
    Invalid16(Number),

    #[error("not enough operands for \"{name}\" (expected {expected}, got {actual})")]
    NotEnoughOperands { name: &'static str, expected: usize, actual: usize },

    #[error("operand type mismatch: expected {0}")]
    OperandTypeExpected(OperandType),

    #[error("too many operands for \"{name}\" (expected {expected}, got {actual})")]
    TooManyOperands { name: &'static str, expected: usize, actual: usize },

    #[error("undefined label: \"{0}\"")]
    UndefinedLabel(SmolStr),

    #[error("else labels can only appear in conditionals")]
    UnexpectedElseLabel,

    #[error("unexpected end of operand list")]
    UnexpectedEnd,

    #[error("unexpected expression")]
    UnexpectedExpr,

    #[error("unexpected message command")]
    UnexpectedMessage,

    #[error("unrecognized command: \"{0}\"")]
    UnrecognizedCommand(SmolStr),

    #[error("unrecognized directive: \".{0}\"")]
    UnrecognizedDirective(SmolStr),

    #[error("unrecognized function: \"{0}\"")]
    UnrecognizedFunction(SmolStr),

    #[error("unrecognized type: \"@{0}\"")]
    UnrecognizedType(SmolStr),

    #[error(transparent)]
    Command(Box<command::Error>),

    #[error(transparent)]
    Encoding(Box<text::Error>),

    #[error(transparent)]
    Io(Box<io::Error>),

    #[error(transparent)]
    Script(Box<script::Error>),

    #[error(transparent)]
    Serialize(Box<serialize::Error>),
}

from_error_boxed!(Error::Command, command::Error);
from_error_boxed!(Error::Encoding, text::Error);
from_error_boxed!(Error::Io, io::Error);
from_error_boxed!(Error::Script, script::Error);
from_error_boxed!(Error::Serialize, serialize::Error);

impl From<Error> for serialize::Error {
    fn from(e: Error) -> Self {
        match e {
            Error::Serialize(inner) => *inner,
            _ => serialize::Error::other(e),
        }
    }
}
