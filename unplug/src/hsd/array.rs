use super::pointer::DefaultIn;
use super::{Error, Node, Pointer, ReadPointer, Result};
use crate::common::ReadFrom;
use crate::hsd::pointer::ReadPointerBase;
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
