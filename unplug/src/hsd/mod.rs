use thiserror::Error;

pub mod sobj;

mod archive;
mod pointer;
mod types;

pub use archive::Archive;
pub use pointer::{Node, Pointer, ReadPointer};
pub use types::PointerArray;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("unsupported version")]
    UnsupportedVersion,

    #[error(transparent)]
    Io(Box<std::io::Error>),
}

from_error_boxed!(Error::Io, std::io::Error);

pub type Result<T> = std::result::Result<T, Error>;
