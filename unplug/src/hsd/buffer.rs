use super::pointer::{DefaultIn, NodeBase, ReadPointerBase};
use super::{Node, Pointer, ReadPointer, Result};
use bumpalo::collections::Vec;
use bumpalo::Bump;
use std::ops::{Deref, DerefMut};

/// A variable-size byte buffer.
///
/// This is not compatible with `read_pointer()`. Use `Buffer::read_pointer_unknown_size()` or
/// `Buffer::read_pointer_known_size()` to read a pointer to a buffer.
#[derive(Clone, PartialEq, Eq)]
pub struct Buffer<'a> {
    bytes: Vec<'a, u8>,
}

impl<'a> std::fmt::Debug for Buffer<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Buffer").field("size", &self.bytes.len()).finish()
    }
}

impl<'a> Buffer<'a> {
    /// Create an empty buffer in an arena.
    pub fn new_in(arena: &'a Bump) -> Self {
        Self { bytes: bumpalo::vec![in arena] }
    }

    /// Create a buffer in an arena with a known maximum size.
    pub fn with_size_in(arena: &'a Bump, size: usize) -> Self {
        Self { bytes: bumpalo::vec![in arena; 0; size] }
    }

    /// Read a pointer to a buffer with an unknown size.
    pub fn read_pointer_unknown_size<R>(reader: &mut R) -> Result<Pointer<'a, Self>>
    where
        R: ReadPointer<'a> + ?Sized,
    {
        reader.read_pointer_into(Self::new_in(reader.arena()))
    }

    /// Read a pointer to a buffer with a known size.
    pub fn read_pointer_known_size<R>(reader: &mut R, size: usize) -> Result<Pointer<'a, Self>>
    where
        R: ReadPointer<'a> + ?Sized,
    {
        reader.read_pointer_into(Self::with_size_in(reader.arena(), size))
    }
}

impl<'a> DefaultIn<'a> for Buffer<'a> {
    fn default_in(arena: &'a Bump) -> Self {
        Self::new_in(arena)
    }
}

impl Deref for Buffer<'_> {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        &self.bytes
    }
}

impl DerefMut for Buffer<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.bytes
    }
}

// Manual impl of NodeBase; we can't use ReadFrom because the buffer needs to have a size assigned.
impl<'a> NodeBase<'a> for Buffer<'a> {
    fn read<'r>(&mut self, reader: &'r mut dyn ReadPointerBase<'a>, max_size: usize) -> Result<()> {
        if self.bytes.is_empty() {
            self.bytes.resize(max_size, 0);
        }
        reader.read_exact(&mut self.bytes)?;
        Ok(())
    }
}

impl<'a> Node<'a> for Buffer<'a> {}
