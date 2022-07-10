use crate::event::{BlockId, Expr};
use slotmap::{new_key_type, SlotMap};
use tracing::warn;

new_key_type! {
    /// A unique ID for a value definition.
    pub struct DefId;
}

pub(super) type DefinitionMap = SlotMap<DefId, Definition>;

/// A label for a variable that can hold an analyzable value.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum Label {
    /// A (bp, sp) pair for a stack value.
    Stack(i16, u8),
    /// A global variable.
    Variable(i16),
    /// The global result storage.
    Result1,
    /// The secondary global result storage.
    Result2,
}

/// A value that can be analyzed.
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub enum Value {
    /// A reference to data in the script file.
    Offset(u32),
    /// A reference to a label which could not be resolved.
    Undefined(Label),
}

/// The type of a value.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum ValueKind {
    /// The value is the address of an event subroutine.
    Event,
    /// The value is the address of an array.
    Array(ArrayKind),
    /// The value is the address of a string.
    String,
    /// The value is a reference to a bone in an object.
    ObjBone,
    /// The value is a pair of object IDs.
    ObjPair,
}

/// The type of an array element.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum ArrayKind {
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    Pointer(Box<ValueKind>),
}

impl ArrayKind {
    /// Retrieves the `ArrayKind` corresponding to an element size expression.
    pub fn from_expr(expr: &Expr) -> Self {
        match expr.value() {
            Some(size) => match size {
                -4 => Self::I32,
                -2 => Self::I16,
                -1 => Self::I8,
                1 => Self::U8,
                2 => Self::U16,
                4 => Self::U32,
                _ => {
                    warn!("Unrecognized array element size {} - declaring a byte array", size);
                    Self::U8
                }
            },
            None => {
                warn!("Array element size {:?} is not a constant - declaring a byte array", expr);
                Self::U8
            }
        }
    }

    /// Retrieves the size of each array element in bytes.
    pub fn element_size(&self) -> usize {
        match self {
            ArrayKind::I8 | ArrayKind::U8 => 1,
            ArrayKind::I16 | ArrayKind::U16 => 2,
            ArrayKind::I32 | ArrayKind::U32 | ArrayKind::Pointer(_) => 4,
        }
    }

    /// Returns `true` if each array element is a pointer.
    pub fn is_pointer(&self) -> bool {
        matches!(self, ArrayKind::Pointer(_))
    }
}

/// A definition which can be traced through a subroutine.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Definition {
    /// The label that the definition was initially assigned to.
    pub label: Label,
    /// The ID of the block that created the definition. If this is None, the definition references a
    /// function input.
    pub origin: Option<BlockId>,
    /// The best-known representation of the definition's value.
    /// This must be resolved relative to the origin block.
    pub value: Value,
}
