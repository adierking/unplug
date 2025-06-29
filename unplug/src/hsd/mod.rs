use thiserror::Error;

pub mod attribute;
pub mod display_list;
pub mod dobj;
pub mod jobj;
pub mod pobj;
pub mod sobj;

mod archive;
mod array;
mod pointer;

pub use archive::Archive;
pub use array::{Array, ByteArray, PointerArray};
pub use pointer::{Node, Pointer, ReadPointer};

#[derive(Error, Debug)]
#[non_exhaustive]
#[allow(variant_size_differences)]
pub enum Error {
    #[error("unsupported version")]
    UnsupportedVersion,

    #[error("not enough data")]
    NotEnoughData,

    #[error("missing relocation for pointer at 0x{0:x}")]
    MissingRelocation(u32),

    #[error("unsupported opcode: 0x{0:x}")]
    UnsupportedOpcode(u8),

    #[error("unsupported primitive type: {0}")]
    UnsupportedPrimitiveType(u8),

    #[error("unsupported attribute name: {0}")]
    UnsupportedAttributeName(u32),

    #[error("unsupported attribute type: {0}")]
    UnsupportedAttributeType(u32),

    #[error("unsupported component count for {0:?}: {1}")]
    UnsupportedComponentCount(attribute::AttributeName, u32),

    #[error("unsupported component type for {0:?}: {1}")]
    UnsupportedComponentType(attribute::AttributeName, u32),

    #[error(transparent)]
    Io(Box<std::io::Error>),
}

from_error_boxed!(Error::Io, std::io::Error);

pub type Result<T> = std::result::Result<T, Error>;
