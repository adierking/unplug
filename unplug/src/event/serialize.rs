use super::opcodes::{Atom, CmdOp, ExprOp, MsgOp};
use super::Pointer;
use crate::common::text::{self, VecText};
use std::io;
use thiserror::Error;
use tracing::error;

/// The result type for (de)serialization operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for (de)serialization operations.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("expected an immediate value, got {0:?}")]
    ExpectedImmediate(ExprOp),

    #[error("unrecognized expression opcode: {0}")]
    UnrecognizedExpr(u8),

    #[error("unsupported expression: {0:?}")]
    UnsupportedExpr(ExprOp),

    #[error("unrecognized command opcode: {0}")]
    UnrecognizedCommand(u8),

    #[error("unsupported command: {0:?}")]
    UnsupportedCommand(CmdOp),

    #[error("unrecognized atom: {0}")]
    UnrecognizedAtom(i32),

    #[error("unsupported atom: {0:?}")]
    UnsupportedAtom(Atom),

    #[error("expected an integer value")]
    ExpectedInteger,

    #[error("expected a pointer")]
    ExpectedPointer,

    #[error("expected an atom")]
    ExpectedAtom,

    #[error("expected text")]
    ExpectedText,

    #[error("expected a command")]
    ExpectedCommand,

    #[error("expected an expression")]
    ExpectedExpr,

    #[error("expected a message")]
    ExpectedMessage,

    #[error("unrecognized message character: {0}")]
    UnrecognizedMsgChar(u16),

    #[error("invalid message character: {0}")]
    InvalidMsgChar(u16),

    #[error("unsupported message character: {0:?}")]
    UnsupportedMsgChar(MsgOp),

    #[error("command does not support variadic arguments: {0:?}")]
    VariadicArgsNotSupported(CmdOp),

    #[error("no variadic argument list active")]
    NoVariadicArgList,

    #[error("unexpected end of data")]
    EndOfData,

    #[error("end offset of argument list ({end:#x}) is before the start offset ({start:#x})")]
    InvalidEndOffset { start: u64, end: u64 },

    #[error("invalid argument list size: {0:#x}")]
    InvalidArgListSize(u64),

    #[error("read past the end of an argument list ({offset:#x} > {end:#x})")]
    PassedEndOfArgList { offset: u64, end: u64 },

    #[error("message is too large ({len} > {max})")]
    MsgTooLarge { len: u64, max: u64 },

    #[error("{0}")]
    Custom(String),

    #[error(transparent)]
    Io(Box<io::Error>),

    #[error(transparent)]
    Text(Box<text::Error>),

    #[error(transparent)]
    Other(Box<dyn std::error::Error + Send + Sync>),
}

impl Error {
    /// Creates an error which wraps an arbitrary error object.
    pub fn other(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Other(Box::from(error))
    }

    /// Creates an error which displays an arbitrary string.
    pub fn custom(s: impl Into<String>) -> Self {
        Self::Custom(s.into())
    }
}

from_error_boxed!(Error::Io, io::Error);
from_error_boxed!(Error::Text, text::Error);

/// An object which can serialize event data.
pub trait EventSerializer {
    /// Serializes a signed 8-bit integer.
    fn serialize_i8(&mut self, val: i8) -> Result<()>;

    /// Serializes an unsigned 8-bit integer.
    fn serialize_u8(&mut self, val: u8) -> Result<()>;

    /// Serializes a signed 16-bit integer.
    fn serialize_i16(&mut self, val: i16) -> Result<()>;

    /// Serializes an unsigned 16-bit integer.
    fn serialize_u16(&mut self, val: u16) -> Result<()>;

    /// Serializes a signed 32-bit integer.
    fn serialize_i32(&mut self, val: i32) -> Result<()>;

    /// Serializes an unsigned 32-bit integer.
    fn serialize_u32(&mut self, val: u32) -> Result<()>;

    /// Serializes a pointer.
    fn serialize_pointer(&mut self, ptr: Pointer) -> Result<()>;

    /// Serializes an array of signed 8-bit integers.
    fn serialize_i8_array(&mut self, arr: &[i8]) -> Result<()>;

    /// Serializes an array of unsigned 8-bit integers.
    fn serialize_u8_array(&mut self, arr: &[u8]) -> Result<()>;

    /// Serializes an array of signed 16-bit integers.
    fn serialize_i16_array(&mut self, arr: &[i16]) -> Result<()>;

    /// Serializes an array of unsigned 16-bit integers.
    fn serialize_u16_array(&mut self, arr: &[u16]) -> Result<()>;

    /// Serializes an array of 32-bit integers.
    fn serialize_i32_array(&mut self, arr: &[i32]) -> Result<()>;

    /// Serializes an array of unsigned 32-bit integers.
    fn serialize_u32_array(&mut self, arr: &[u32]) -> Result<()>;

    /// Serializes an array of pointers.
    fn serialize_pointer_array(&mut self, arr: &[Pointer]) -> Result<()>;

    /// Serializes an atom expression.
    fn serialize_atom(&mut self, atom: Atom) -> Result<()>;

    /// Serializes a text string.
    fn serialize_text(&mut self, text: &VecText) -> Result<()>;

    /// Serializes an RGBA color value.
    fn serialize_rgba(&mut self, rgba: u32) -> Result<()>;

    /// Begin serializing an expression with opcode `expr`.
    fn begin_expr(&mut self, expr: ExprOp) -> Result<()>;

    /// Finishes serializing an expression.
    fn end_expr(&mut self) -> Result<()>;

    /// Begins serializing a command with opcode `command`.
    fn begin_command(&mut self, command: CmdOp) -> Result<()>;

    /// Finishes serializing a command.
    fn end_command(&mut self) -> Result<()>;

    /// Begins serializing a variable-length argument list.
    fn begin_variadic_args(&mut self, count: usize) -> Result<()>;

    /// Finishes serializing a variable-length argument list.
    fn end_variadic_args(&mut self) -> Result<()>;

    /// Serializes a single message character.
    fn serialize_msg_char(&mut self, ch: MsgOp) -> Result<()>;
}

/// An object which can deserialize event data.
pub trait EventDeserializer {
    /// Deserializes a signed 8-bit integer and returns it.
    fn deserialize_i8(&mut self) -> Result<i8>;

    /// Deserializes an unsigned 8-bit integer and returns it.
    fn deserialize_u8(&mut self) -> Result<u8>;

    /// Deserializes a signed 16-bit integer and returns it.
    fn deserialize_i16(&mut self) -> Result<i16>;

    /// Deserializes an unsigned 16-bit integer and returns it.
    fn deserialize_u16(&mut self) -> Result<u16>;

    /// Deserializes a signed 32-bit integer and returns it.
    fn deserialize_i32(&mut self) -> Result<i32>;

    /// Deserializes an unsigned 32-bit integer and returns it.
    fn deserialize_u32(&mut self) -> Result<u32>;

    /// Deserializes a pointer and returns it.
    fn deserialize_pointer(&mut self) -> Result<Pointer>;

    /// Deserializes an array of `len` signed 8-bit integers.
    fn deserialize_i8_array(&mut self, len: usize) -> Result<Vec<i8>>;

    /// Deserializes an array of `len` unsigned 8-bit integers.
    fn deserialize_u8_array(&mut self, len: usize) -> Result<Vec<u8>>;

    /// Deserializes an array of `len` signed 16-bit integers.
    fn deserialize_i16_array(&mut self, len: usize) -> Result<Vec<i16>>;

    /// Deserializes an array of `len` unsigned 16-bit integers.
    fn deserialize_u16_array(&mut self, len: usize) -> Result<Vec<u16>>;

    /// Deserializes an array of `len` 32-bit integers.
    fn deserialize_i32_array(&mut self, len: usize) -> Result<Vec<i32>>;

    /// Deserializes an array of `len` unsigned 32-bit integers.
    fn deserialize_u32_array(&mut self, len: usize) -> Result<Vec<u32>>;

    /// Deserializes an array of up to `max_len` pointers.
    fn deserialize_pointer_array(&mut self, max_len: usize) -> Result<Vec<Pointer>>;

    /// Deserializes an atom expression and returns it.
    fn deserialize_atom(&mut self) -> Result<Atom>;

    /// Deserializes a text string and returns it.
    fn deserialize_text(&mut self) -> Result<VecText>;

    /// Deserializes an RGBA color value and returns it.
    fn deserialize_rgba(&mut self) -> Result<u32>;

    /// Begins deserializing an expression and returns its opcode.
    fn begin_expr(&mut self) -> Result<ExprOp>;

    /// Finishes deserializing an expression.
    fn end_expr(&mut self) -> Result<()>;

    /// Begins deserializing a command and returns its opcode.
    fn begin_command(&mut self) -> Result<CmdOp>;

    /// Finishes deserializing a command.
    fn end_command(&mut self) -> Result<()>;

    /// Begins deserializing a variable-length argument list.
    fn begin_variadic_args(&mut self) -> Result<()>;

    /// Checks whether more variadic arguments are available to be read.
    fn have_variadic_arg(&mut self) -> Result<bool>;

    /// Finishes deserializing a variable-length argument list.
    fn end_variadic_args(&mut self) -> Result<()>;

    /// Deserializes a single message character and returns it.
    fn deserialize_msg_char(&mut self) -> Result<MsgOp>;
}

/// An object which can be serialized to an `EventSerializer`.
pub trait SerializeEvent: Sized {
    /// The error type returned from `serialize()`.
    type Error;

    /// Serialize this object's event data.
    fn serialize(&self, ser: &mut dyn EventSerializer) -> std::result::Result<(), Self::Error>;
}

/// An object which can be deserialized from an `EventDeserializer`.
pub trait DeserializeEvent: Sized {
    /// The error type returned from `deserialize()`.
    type Error;

    /// Deserialize this type and return it.
    fn deserialize(de: &mut dyn EventDeserializer) -> std::result::Result<Self, Self::Error>;
}

/// Implements serialization for a value type.
macro_rules! impl_serialize {
    ($type:ty, $sfunc:ident, $dfunc:ident $(,$deref:tt)?) => {
        impl SerializeEvent for $type {
            type Error = Error;
            fn serialize(&self, ser: &mut dyn EventSerializer) -> Result<()> {
                ser.$sfunc($($deref)? self)
            }
        }
        impl DeserializeEvent for $type {
            type Error = Error;
            fn deserialize(de: &mut dyn EventDeserializer) -> Result<Self> {
                de.$dfunc()
            }
        }
    };
}
impl_serialize!(i8, serialize_i8, deserialize_i8, *);
impl_serialize!(u8, serialize_u8, deserialize_u8, *);
impl_serialize!(i16, serialize_i16, deserialize_i16, *);
impl_serialize!(u16, serialize_u16, deserialize_u16, *);
impl_serialize!(i32, serialize_i32, deserialize_i32, *);
impl_serialize!(u32, serialize_u32, deserialize_u32, *);
impl_serialize!(Pointer, serialize_pointer, deserialize_pointer, *);
impl_serialize!(Atom, serialize_atom, deserialize_atom, *);
impl_serialize!(VecText, serialize_text, deserialize_text);
