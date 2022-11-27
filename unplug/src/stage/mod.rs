mod object;
#[allow(clippy::module_inception)]
mod stage;

pub use object::*;
pub use stage::*;

use crate::event::script;
use std::io;
use thiserror::Error;

/// The result type for stage operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for stage operations.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("invalid stage header")]
    InvalidHeader,

    #[error("invalid object index: {0}")]
    InvalidObjectIndex(i32),

    #[error("unrecognized object id: {0}")]
    UnrecognizedObject(i32),

    #[error(transparent)]
    Script(Box<script::Error>),

    #[error(transparent)]
    Io(Box<io::Error>),
}

from_error_boxed!(Error::Script, script::Error);
from_error_boxed!(Error::Io, io::Error);
