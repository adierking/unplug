use super::pointer::DefaultIn;
use super::pointer::{NodeBase, ReadPointerBase};
use super::{Error, Node, Pointer, ReadPointer, Result};
use crate::common::ReadFrom;
use bumpalo::collections::vec::IntoIter;
use bumpalo::collections::Vec;
use bumpalo::Bump;
use std::ops::{Deref, DerefMut};

/// Trait for values that can be used in arrays.
/// Arrays (always?) end in a sentinel value, and the type needs to specify how to detect that.
pub trait ArrayElement {
    fn is_end_of_array(&self) -> bool;
}

impl<'a, T: Node<'a>> ArrayElement for Pointer<'a, T> {
    fn is_end_of_array(&self) -> bool {
        self.is_null()
    }
}

/// An array of pointers to nodes.
pub type PointerArray<'a, T> = Array<'a, Pointer<'a, T>>;

/// An array of values terminated by a sentinel value (e.g. null pointer).
/// Typically this needs to be contained in a pointer itself (TODO: make this easier).
#[derive(Clone, PartialEq, Eq)]
pub struct Array<'a, T: Node<'a> + ArrayElement> {
    elements: Vec<'a, T>,
}

impl<'a, T: Node<'a> + ArrayElement + std::fmt::Debug> std::fmt::Debug for Array<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(&self.elements).finish()
    }
}

impl<'a, T: Node<'a> + ArrayElement> Array<'a, T> {
    pub fn new_in(arena: &'a Bump) -> Self {
        Self { elements: Vec::new_in(arena) }
    }
}

impl<'a, T: Node<'a> + ArrayElement> DefaultIn<'a> for Array<'a, T> {
    fn default_in(arena: &'a Bump) -> Self {
        Self::new_in(arena)
    }
}

impl<'a, T: Node<'a> + ArrayElement> Deref for Array<'a, T> {
    type Target = [T];
    fn deref(&self) -> &Self::Target {
        self.elements.deref()
    }
}

impl<'a, T: Node<'a> + ArrayElement> DerefMut for Array<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.elements.deref_mut()
    }
}

impl<'a, T: Node<'a> + ArrayElement> IntoIterator for Array<'a, T> {
    type Item = T;
    type IntoIter = IntoIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.elements.into_iter()
    }
}

impl<'a, R, T> ReadFrom<R> for Array<'a, T>
where
    R: ReadPointer<'a> + ?Sized,
    T: Node<'a> + ArrayElement + ReadFrom<R, Error = Error>,
{
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let mut elements = Vec::new_in(reader.arena());
        loop {
            let value = T::read_from(reader)?;
            if value.is_end_of_array() {
                break;
            }
            elements.push(value);
        }
        Ok(Self { elements })
    }
}

impl<'a, T> Node<'a> for Array<'a, T> where
    for<'x> T: Node<'a> + ArrayElement + ReadFrom<dyn ReadPointerBase<'a> + 'x, Error = Error> + 'a
{
}

/// An immutable array of bytes.
///
/// This is not compatible with `read_pointer()`. Use `ByteArray::read_pointer()` to both read and
/// specify a size.
#[derive(Copy, Clone)]
pub struct ByteArray<'a> {
    arena: &'a Bump,
    bytes: &'a [u8],
    max_len: usize,
}

impl<'a> ByteArray<'a> {
    /// Pass this to `read_pointer()` to guess the size of the buffer based on adjacent nodes.
    /// Only use this if there is no way to know the buffer size ahead of time.
    pub const UNKNOWN_LENGTH: usize = usize::MAX;

    /// Create an empty byte array.
    pub fn new_in(arena: &'a Bump) -> Self {
        Self { arena, bytes: &[], max_len: 0 }
    }

    /// Create an empty byte array with a max length set.
    pub fn with_max_len_in(arena: &'a Bump, max_len: usize) -> Self {
        Self { arena, bytes: &[], max_len }
    }

    /// Create a byte array from a span of bytes.
    pub fn with_bytes_in(arena: &'a Bump, bytes: &[u8]) -> Self {
        Self { arena, bytes: arena.alloc_slice_copy(bytes), max_len: bytes.len() }
    }

    /// Read a pointer to a buffer with the given maximum size.
    /// If the size is unknown, use `UNKNOWN_LENGTH` to guess the size.
    pub fn read_pointer<R>(reader: &mut R, max_len: usize) -> Result<Pointer<'a, Self>>
    where
        R: ReadPointer<'a> + ?Sized,
    {
        reader.read_pointer_into(Self::with_max_len_in(reader.arena(), max_len))
    }

    /// Return a slice over the bytes in the array.
    pub fn as_slice(&self) -> &'a [u8] {
        self.bytes
    }
}

impl<'a> std::fmt::Debug for ByteArray<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ByteArray").field("len", &self.bytes.len()).finish()
    }
}

impl Deref for ByteArray<'_> {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

// Manual impl of NodeBase; we can't use ReadFrom because the buffer needs to have a size assigned.
impl<'a> NodeBase<'a> for ByteArray<'a> {
    fn read<'r>(&mut self, reader: &'r mut dyn ReadPointerBase<'a>, max_size: usize) -> Result<()> {
        let len = if self.max_len == Self::UNKNOWN_LENGTH { max_size } else { self.max_len };
        let bytes = self.arena.alloc_slice_fill_default(len);
        reader.read_exact(bytes)?;
        self.bytes = bytes;
        Ok(())
    }
}

impl<'a> Node<'a> for ByteArray<'a> {}
