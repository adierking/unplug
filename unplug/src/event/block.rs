use super::command::Command;
use super::expr::{ObjBone, ObjPair};
use super::pointer::Pointer;
use super::serialize::{self, EventSerializer, SerializeEvent};
use crate::common::VecText;

/// A block of data in a script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Placeholder,
    Code(CodeBlock),
    Data(DataBlock),
}

impl Block {
    /// Returns whether this block is a placeholder.
    pub fn is_placeholder(&self) -> bool {
        matches!(self, Self::Placeholder)
    }

    /// Returns whether this block is a code block.
    pub fn is_code(&self) -> bool {
        self.code().is_some()
    }

    /// If this block is a code block, returns a reference to the `CodeBlock`.
    pub fn code(&self) -> Option<&CodeBlock> {
        match self {
            Self::Code(c) => Some(c),
            _ => None,
        }
    }

    /// If this block is a code block, returns a mutable reference to the `CodeBlock`.
    pub fn code_mut(&mut self) -> Option<&mut CodeBlock> {
        match self {
            Self::Code(c) => Some(c),
            _ => None,
        }
    }

    /// If this block is a code block, returns a reference to its commands.
    pub fn commands(&self) -> Option<&[Command]> {
        match self {
            Self::Code(c) => Some(&c.commands),
            _ => None,
        }
    }

    /// If this block is a code block, returns a mutable reference to its commands.
    pub fn commands_mut(&mut self) -> Option<&mut [Command]> {
        match self {
            Self::Code(c) => Some(&mut c.commands),
            _ => None,
        }
    }

    /// Returns whether this block is a data block.
    pub fn is_data(&self) -> bool {
        self.data().is_some()
    }

    /// If this block is a data block, returns a reference to the `DataBlock`.
    pub fn data(&self) -> Option<&DataBlock> {
        match self {
            Self::Data(d) => Some(d),
            _ => None,
        }
    }
}

impl Default for Block {
    fn default() -> Self {
        Self::Placeholder
    }
}

impl From<CodeBlock> for Block {
    fn from(code: CodeBlock) -> Self {
        Self::Code(code)
    }
}

impl From<DataBlock> for Block {
    fn from(data: DataBlock) -> Self {
        Self::Data(data)
    }
}

/// A [basic block] of commands in an event which has single points of entry and exit.
///
/// [basic block]: https://en.wikipedia.org/wiki/Basic_block
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodeBlock {
    /// The list of commands in the block.
    pub commands: Vec<Command>,
    /// The ID of the block to jump to after this block, if any.
    pub next_block: Option<Pointer>,
    /// The ID of the block to jump to if the block's condition fails, if any.
    pub else_block: Option<Pointer>,
}

impl CodeBlock {
    /// Constructs an empty code block.
    pub fn new() -> Self {
        Self::default()
    }
}

/// A block of data in an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataBlock {
    /// The block is an array of signed 8-bit integers.
    I8Array(Vec<i8>),
    /// The block is an array of unsigned 8-bit integers.
    U8Array(Vec<u8>),
    /// The block is an array of signed 16-bit integers.
    I16Array(Vec<i16>),
    /// The block is an array of unsigned 16-bit integers.
    U16Array(Vec<u16>),
    /// The block is an array of signed 32-bit integers.
    I32Array(Vec<i32>),
    /// The block is an array of unsigned 32-bit integers.
    U32Array(Vec<u32>),
    /// The block is an array of pointers, usually terminated by a null pointer.
    PtrArray(Vec<Pointer>),
    /// The block is a path to a bone.
    ObjBone(ObjBone),
    /// The block is a pair of object IDs.
    ObjPair(ObjPair),
    /// The block is text data.
    String(VecText),
    /// The block is a contiguous stream of varying data types.
    ///
    /// These blocks are not used by the game and currently cannot be read back in correctly.
    Variable(Vec<DataBlock>),
}

macro_rules! impl_data_block_from {
    ($type:ty, $name:ident) => {
        impl From<$type> for DataBlock {
            fn from(val: $type) -> Self {
                Self::$name(val)
            }
        }
    };
}

impl_data_block_from!(Vec<i8>, I8Array);
impl_data_block_from!(Vec<u8>, U8Array);
impl_data_block_from!(Vec<i16>, I16Array);
impl_data_block_from!(Vec<u16>, U16Array);
impl_data_block_from!(Vec<i32>, I32Array);
impl_data_block_from!(Vec<u32>, U32Array);
impl_data_block_from!(Vec<Pointer>, PtrArray);
impl_data_block_from!(ObjBone, ObjBone);
impl_data_block_from!(ObjPair, ObjPair);
impl_data_block_from!(VecText, String);
impl_data_block_from!(Vec<DataBlock>, Variable);

impl SerializeEvent for DataBlock {
    type Error = serialize::Error;
    fn serialize(&self, ser: &mut dyn EventSerializer) -> serialize::Result<()> {
        match self {
            DataBlock::I8Array(arr) => ser.serialize_i8_array(arr),
            DataBlock::U8Array(arr) => ser.serialize_u8_array(arr),
            DataBlock::I16Array(arr) => ser.serialize_i16_array(arr),
            DataBlock::U16Array(arr) => ser.serialize_u16_array(arr),
            DataBlock::I32Array(arr) => ser.serialize_i32_array(arr),
            DataBlock::U32Array(arr) => ser.serialize_u32_array(arr),
            DataBlock::PtrArray(arr) => ser.serialize_pointer_array(arr),
            DataBlock::ObjBone(bone) => bone.serialize(ser),
            DataBlock::ObjPair(pair) => pair.serialize(ser),
            DataBlock::String(s) => ser.serialize_text(s),
            DataBlock::Variable(data) => data.iter().try_for_each(|x| x.serialize(ser)),
        }
    }
}
