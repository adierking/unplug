pub mod metadata;

mod builder;
mod collision;
mod header;
mod libs;
mod reader;

pub use builder::*;
pub use collision::*;
pub use libs::*;
pub use metadata::Metadata;
pub use reader::*;

use crate::common::sfx_id;
use crate::event::script;
use std::io;
use thiserror::Error;

/// The result type for globals operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for globals operations.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("invalid globals header")]
    InvalidHeader,

    #[error("unrecognized collision shape: {0}")]
    UnrecognizedShape(i16),

    #[error("unrecognized collision material: {0}")]
    UnrecognizedMaterial(i16),

    #[error("invalid pickup sound index: {0}")]
    InvalidPickupSound(i8),

    #[error("invalid collect sound index: {0}")]
    InvalidCollectSound(i8),

    #[error(transparent)]
    Script(Box<script::Error>),

    #[error(transparent)]
    Sfx(Box<sfx_id::Error>),

    #[error(transparent)]
    Io(Box<io::Error>),
}

from_error_boxed!(Error::Script, script::Error);
from_error_boxed!(Error::Sfx, sfx_id::Error);
from_error_boxed!(Error::Io, io::Error);
