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

pub mod label;
pub mod lexer;
pub mod opcodes;
pub mod parser;
pub mod program;
pub mod writer;

use std::io;
use std::sync::Arc;
use thiserror::Error;
use unplug::event::command;
use unplug::event::serialize;
use unplug::event::BlockId;
use unplug::from_error_boxed;

/// The result type for ASM operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for ASM operations.
#[derive(Debug, Error)]
#[non_exhaustive]
#[allow(variant_size_differences)]
pub enum Error {
    #[error("block already has a label: {0:?}")]
    BlockHasLabel(BlockId),

    #[error("duplicate label: \"{0}\"")]
    DuplicateLabel(Arc<str>),

    #[error(transparent)]
    Command(Box<command::Error>),

    #[error(transparent)]
    Io(Box<io::Error>),

    #[error(transparent)]
    Serialize(Box<serialize::Error>),
}

from_error_boxed!(Error::Command, command::Error);
from_error_boxed!(Error::Io, io::Error);
from_error_boxed!(Error::Serialize, serialize::Error);
