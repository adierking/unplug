use super::opcodes::{CmdOp, ExprOp, MsgOp, TypeOp};
use super::Pointer;
use crate::common::Text;
use std::io;
use thiserror::Error;
use tracing::error;

/// The result type for (de)serialization operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for (de)serialization operations.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("unrecognized expression opcode: {0}")]
    UnrecognizedExpr(u8),

    #[error("unsupported expression: {0:?}")]
    UnsupportedExpr(ExprOp),

    #[error("unrecognized command opcode: {0}")]
    UnrecognizedCommand(u8),

    #[error("unsupported command: {0:?}")]
    UnsupportedCommand(CmdOp),

    #[error("unrecognized type code: {0}")]
    UnrecognizedType(i32),

    #[error("unsupported type: {0:?}")]
    UnsupportedType(TypeOp),

    #[error("expected a constant type value, got {0:?} instead")]
    ExpectedType(ExprOp),

    #[error("unrecognized message character: {0}")]
    UnrecognizedMsgChar(u16),

    #[error("invalid message character: {0}")]
    InvalidMsgChar(u16),

    #[error("unsupported message character: {0:?}")]
    UnsupportedMsgChar(MsgOp),

    #[error("message end offset ({end:#x}) is before the start offset ({start:#x})")]
    InvalidMsgOffset { start: u64, end: u64 },

    #[error("read past the end of the message ({offset:#x} > {end:#x})")]
    PassedEndOfMsg { offset: u64, end: u64 },

    #[error("message is too large ({len} > {max})")]
    MsgTooLarge { len: u64, max: u64 },

    #[error(transparent)]
    Io(Box<io::Error>),

    #[error(transparent)]
    Other(Box<dyn std::error::Error + Send + Sync>),
}

impl Error {
    /// Creates an error which wraps an arbitrary error object.
    pub fn other(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Other(Box::from(error))
    }
}

from_error_boxed!(Error::Io, io::Error);

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

    /// Serializes a type expression.
    fn serialize_type(&mut self, ty: TypeOp) -> Result<()>;

    /// Serializes a null-terminated text string.
    fn serialize_text(&mut self, text: &Text) -> Result<()>;

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

    /// Begins serializing a `call()` command's argument list.
    fn begin_call(&mut self) -> Result<()>;

    /// Finishes serializing a `call()` command's argument list.
    fn end_call(&mut self) -> Result<()>;

    /// Begins serializing a message.
    fn begin_msg(&mut self) -> Result<()>;

    /// Serializes a single message character.
    fn serialize_msg_char(&mut self, ch: MsgOp) -> Result<()>;

    /// Finishes serializing a message.
    fn end_msg(&mut self) -> Result<()>;
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

    /// Deserializes a type expression and returns it.
    fn deserialize_type(&mut self) -> Result<TypeOp>;

    /// Deserializes a null-terminated text string and returns it.
    fn deserialize_text(&mut self) -> Result<Text>;

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

    /// Begins deserializing a `call()` command's argument list.
    fn begin_call(&mut self) -> Result<()>;

    /// Checks whether more `call()` arguments are available to be read.
    fn have_call_arg(&mut self) -> Result<bool>;

    /// Finishes deserializing a `call()` command's argument list.
    fn end_call(&mut self) -> Result<()>;

    /// Begins deserializing a message.
    fn begin_msg(&mut self) -> Result<()>;

    /// Deserializes a single message character and returns it.
    fn deserialize_msg_char(&mut self) -> Result<MsgOp>;

    /// Finishes deserializing a message.
    fn end_msg(&mut self) -> Result<()>;
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

/// Implements serialization for a primitive type.
macro_rules! impl_serialize {
    ($type:ty, $sfunc:ident, $dfunc:ident) => {
        impl SerializeEvent for $type {
            type Error = Error;
            fn serialize(&self, ser: &mut dyn EventSerializer) -> Result<()> {
                ser.$sfunc(*self)
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
impl_serialize!(i8, serialize_i8, deserialize_i8);
impl_serialize!(u8, serialize_u8, deserialize_u8);
impl_serialize!(i16, serialize_i16, deserialize_i16);
impl_serialize!(u16, serialize_u16, deserialize_u16);
impl_serialize!(i32, serialize_i32, deserialize_i32);
impl_serialize!(u32, serialize_u32, deserialize_u32);
impl_serialize!(Pointer, serialize_pointer, deserialize_pointer);
impl_serialize!(TypeOp, serialize_type, deserialize_type);

impl SerializeEvent for Text {
    type Error = Error;
    fn serialize(&self, ser: &mut dyn EventSerializer) -> Result<()> {
        ser.serialize_text(self)
    }
}
impl DeserializeEvent for Text {
    type Error = Error;
    fn deserialize(de: &mut dyn EventDeserializer) -> Result<Self> {
        de.deserialize_text()
    }
}
