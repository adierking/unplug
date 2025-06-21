use thiserror::Error;

pub mod dobj;
pub mod jobj;
pub mod pobj;
pub mod sobj;

mod archive;
mod array;
mod buffer;
mod pointer;

pub use archive::Archive;
pub use array::PointerArray;
pub use buffer::Buffer;
pub use pointer::{Node, Pointer, ReadPointer};

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
