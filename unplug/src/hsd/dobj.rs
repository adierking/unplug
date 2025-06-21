use super::pobj::PObj;
use super::{Error, Node, Pointer, ReadPointer, Result};
use crate::common::ReadFrom;

#[derive(Debug, Default, Clone)]
pub struct DObj<'a> {
    pub name: Pointer<'a, ()>,
    pub next: Pointer<'a, DObj<'a>>,
    pub materials: Pointer<'a, ()>,
    pub polygons: Pointer<'a, PObj<'a>>,
}

impl<'a, R: ReadPointer<'a> + ?Sized> ReadFrom<R> for DObj<'a> {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self {
            name: reader.read_pointer()?,
            next: reader.read_pointer()?,
            materials: reader.read_pointer()?,
            polygons: reader.read_pointer()?,
        })
    }
}

impl<'a> Node<'a> for DObj<'a> {}
