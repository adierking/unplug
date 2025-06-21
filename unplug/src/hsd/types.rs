use super::pointer::DefaultIn;
use super::{Error, Node, Pointer, ReadPointer, Result};
use crate::common::ReadFrom;
use bumpalo::collections::vec::IntoIter;
use bumpalo::collections::Vec;
use bumpalo::Bump;
use std::borrow::{Borrow, BorrowMut};
use std::ops::{Deref, DerefMut};

/// An array of node pointers terminated by a null pointer.
/// Typically this needs to be contained in a pointer itself (TODO: make this easier).
#[derive(Clone, PartialEq, Eq)]
pub struct PointerArray<'a, T: Node<'a>> {
    elements: Vec<'a, Pointer<'a, T>>,
}

impl<'a, T: Node<'a> + std::fmt::Debug> std::fmt::Debug for PointerArray<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(&self.elements).finish()
    }
}

impl<'a, T: Node<'a>> PointerArray<'a, T> {
    pub fn new_in(arena: &'a Bump) -> Self {
        Self { elements: Vec::new_in(arena) }
    }
}

impl<'a, T: Node<'a>> DefaultIn<'a> for PointerArray<'a, T> {
    fn default_in(arena: &'a Bump) -> Self {
        Self::new_in(arena)
    }
}

impl<'a, T: Node<'a>> Deref for PointerArray<'a, T> {
    type Target = [Pointer<'a, T>];
    fn deref(&self) -> &Self::Target {
        self.elements.deref()
    }
}

impl<'a, T: Node<'a>> DerefMut for PointerArray<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.elements.deref_mut()
    }
}

impl<'a, T: Node<'a>> AsRef<[Pointer<'a, T>]> for PointerArray<'a, T> {
    fn as_ref(&self) -> &[Pointer<'a, T>] {
        self.deref()
    }
}

impl<'a, T: Node<'a>> AsMut<[Pointer<'a, T>]> for PointerArray<'a, T> {
    fn as_mut(&mut self) -> &mut [Pointer<'a, T>] {
        self.deref_mut()
    }
}

impl<'a, T: Node<'a>> Borrow<[Pointer<'a, T>]> for PointerArray<'a, T> {
    fn borrow(&self) -> &[Pointer<'a, T>] {
        self.deref()
    }
}

impl<'a, T: Node<'a>> BorrowMut<[Pointer<'a, T>]> for PointerArray<'a, T> {
    fn borrow_mut(&mut self) -> &mut [Pointer<'a, T>] {
        self.deref_mut()
    }
}

impl<'a, T: Node<'a>> IntoIterator for PointerArray<'a, T> {
    type Item = Pointer<'a, T>;
    type IntoIter = IntoIter<'a, Pointer<'a, T>>;

    fn into_iter(self) -> Self::IntoIter {
        self.elements.into_iter()
    }
}

impl<'a, T: Node<'a>, R: ReadPointer<'a> + ?Sized> ReadFrom<R> for PointerArray<'a, T> {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let mut elements = Vec::new_in(reader.arena());
        loop {
            let ptr = reader.read_pointer::<T>()?;
            if ptr.is_null() {
                break;
            }
            elements.push(ptr);
        }
        Ok(Self { elements })
    }
}

impl<'a, T: Node<'a>> Node<'a> for PointerArray<'a, T> {}
