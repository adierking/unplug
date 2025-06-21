use super::pointer::{DefaultIn, NodeBase, ReadPointerBase};
use super::{Node, Pointer, ReadPointer, Result};
use bumpalo::collections::Vec;
use bumpalo::Bump;
use std::ops::{Deref, DerefMut};

/// A variable-size byte buffer.
///
/// The size needs to be specified when the buffer is created, so to read a buffer given a size, you
/// can use `Buffer::read_pointer()`.
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
        Self { bytes: Vec::new_in(arena) }
    }

    /// Create a buffer in an arena, zero-initialized up to the given size.
    pub fn with_size_in(arena: &'a Bump, size: usize) -> Self {
        let mut bytes = Vec::new_in(arena);
        bytes.resize(size, 0);
        Self { bytes }
    }

    /// Read a pointer to a buffer with the given size.
    pub fn read_pointer<R>(reader: &mut R, size: usize) -> Result<Pointer<'a, Self>>
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
    fn read<'r>(&mut self, reader: &'r mut dyn ReadPointerBase<'a>) -> Result<()> {
        reader.read_exact(&mut self.bytes)?;
        Ok(())
    }
}

impl<'a> Node<'a> for Buffer<'a> {}
