use super::{Error, Result};
use crate::common::ReadFrom;
use bumpalo::Bump;
use std::cell::{Ref, RefCell, RefMut};
use std::io::Read;
use std::num::NonZeroU32;

// This is some convoluted type system stuff to support the node graph (yay, Rust!). Basically, a
// Pointer can either be null or hold a reference to a node. When we read a pointer, we need to put
// the offset in a queue where we also know what type of node to read there. Each pointer also has
// to be given a reference to the default-initialized node, because we don't want to have to deal
// with needing a reference to the entire archive every time you dereference a pointer. As the
// reader goes through the queue, it will eventually initialize each node.
//
// The tricky thing here is that to support reading each node based on its type, we have to use a
// dyn-compatible trait. So we split the node traits between NodeBase (which is dyn-compatible) and
// the public Node trait (which is a marker that ensures a type conforms to the correct traits). We
// also have to take a dyn pointer to the reader, and we want to be able to read generic node types,
// so we also split that between ReadPointerBase (dyn-compatible) and ReadPointer. This makes it
// super easy to write the ReadFrom impls for each node.
//
// All of the memory is stored in a Bump arena that gets dropped when we're done with the archive,
// otherwise managing lifetimes here would become insanely difficult. Currently, we also just put
// each node into a RefCell to make borrow checking easier.

pub trait ReadPointerBase<'a>: Read {
    /// Get the arena belonging to the node graph.
    fn arena(&self) -> &'a Bump;

    /// Read a 32-bit offset and validate that it has a relocation pointing to it.
    fn read_offset(&mut self) -> Result<Option<NonZeroU32>>;

    /// Enqueue a node to be read at an offset.
    fn add_node(&mut self, offset: u32, node: &'a RefCell<dyn NodeBase<'a>>);
}

pub trait NodeBase<'a>: 'a {
    /// Read the node's data from a reader.
    fn read<'r>(&mut self, reader: &'r mut dyn ReadPointerBase<'a>) -> Result<()>;
}

// Auto-implement NodeBase for anything which supports ReadFrom<ReadPointer>.
impl<'a, T: 'a> NodeBase<'a> for T
where
    for<'x> T: ReadFrom<dyn ReadPointerBase<'a> + 'x, Error = Error> + 'a,
{
    fn read<'r>(&mut self, reader: &'r mut dyn ReadPointerBase<'a>) -> Result<()> {
        *self = T::read_from(reader)?;
        Ok(())
    }
}

/// Trait for a value that can be default-initialized using an arena.
pub trait DefaultIn<'a> {
    fn default_in(arena: &'a Bump) -> Self;
}

impl<'a, T: Default> DefaultIn<'a> for T {
    fn default_in(_arena: &'a Bump) -> Self {
        Self::default()
    }
}

/// Marker trait which ensures that a type conforms to all of the necessary traits for a node.
/// Technically this is not necessary and we could use just NodeBase everywhere, but this makes it
/// more explicit which types are actually nodes.
pub trait Node<'a>: NodeBase<'a> + DefaultIn<'a> {}

// () is a node with nothing in it. Useful for pointers to unimplemented structs.
impl Node<'_> for () {}
impl<'a, R: ReadPointer<'a> + ?Sized> ReadFrom<R> for () {
    type Error = Error;
    fn read_from(_reader: &mut R) -> Result<Self> {
        Ok(())
    }
}

/// Holds a nullable reference to a node in the graph.
#[derive(Copy, Clone, PartialEq, Eq)]
#[repr(transparent)]
pub struct Pointer<'a, T: Node<'a>>(Option<&'a RefCell<T>>);

impl<'a, T: Node<'a> + std::fmt::Debug> std::fmt::Debug for Pointer<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(n) => f.debug_tuple("Pointer").field(&*n.borrow()).finish(),
            None => f.debug_tuple("Pointer").field(&self.0).finish(),
        }
    }
}

impl<'a, T: Node<'a>> Pointer<'a, T> {
    pub fn new() -> Self {
        Self(None)
    }

    pub fn alloc(arena: &'a Bump, node: T) -> Self {
        Self(Some(arena.alloc(RefCell::new(node))))
    }

    pub fn borrow(&self) -> Option<Ref<'a, T>> {
        self.0.map(|obj| obj.borrow())
    }

    pub fn borrow_mut(&self) -> Option<RefMut<'a, T>> {
        self.0.map(|obj| obj.borrow_mut())
    }

    pub fn is_null(&self) -> bool {
        self.0.is_none()
    }
}

impl<'a, T: Node<'a>> Default for Pointer<'a, T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Extension trait for ReadPointerBase which provides the generic read_pointer().
pub trait ReadPointer<'a>: ReadPointerBase<'a> {
    /// Read a pointer from the stream.
    fn read_pointer<T: Node<'a>>(&mut self) -> Result<Pointer<'a, T>> {
        match self.read_offset()? {
            Some(offset) => self.read_node(offset.get()),
            None => Ok(Pointer(None)),
        }
    }

    /// Read a node from the stream at the given offset and return a pointer for it.
    fn read_node<T: Node<'a>>(&mut self, offset: u32) -> Result<Pointer<'a, T>> {
        let pointer = Pointer::alloc(self.arena(), T::default_in(self.arena()));
        self.add_node(offset, pointer.0.unwrap());
        Ok(pointer)
    }
}

impl<'a, R: ReadPointerBase<'a> + ?Sized> ReadPointer<'a> for R {}
