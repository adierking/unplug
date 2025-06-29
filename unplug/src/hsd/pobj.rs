use super::attribute::AttributeArray;
use super::display_list::DisplayList;
use super::{ByteArray, Error, Node, Pointer, ReadPointer, Result};
use crate::common::ReadFrom;
use bitflags::bitflags;
use byteorder::{ReadBytesExt, BE};

const DISPLAY_LIST_BLOCK_SIZE: usize = 0x20;

bitflags! {
    // From HSDLib
    #[derive(Default)]
    pub struct Flags: u16 {
        const SHAPESET_AVERAGE = 1 << 0;
        const SHAPESET_ADDITIVE = 1 << 1;
        const UNK_2 = 1 << 2;
        const ANIM = 1 << 3;
        const SHAPE_ANIM = 1 << 12;
        const ENVELOPE = 1 << 13;
        const CULL_BACK = 1 << 14;
        const CULL_FRONT = 1 << 15;
    }
}

#[derive(Debug, Default, Clone)]
pub struct PObj<'a> {
    pub name: Pointer<'a, ()>,
    pub next: Pointer<'a, PObj<'a>>,
    pub attributes: Pointer<'a, AttributeArray<'a>>,
    pub flags: Flags,
    pub display_list: Pointer<'a, ByteArray<'a>>,
    pub jobj: Pointer<'a, ()>,
}

impl<'a> PObj<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn vertex_size(&self) -> usize {
        match self.attributes.borrow() {
            Some(attributes) => attributes.iter().map(|a| a.display_list_size()).sum(),
            None => 0,
        }
    }

    pub fn parse_display_list(&self) -> Result<DisplayList<'a>> {
        match self.display_list.borrow() {
            Some(buffer) => DisplayList::parse(buffer.as_slice(), self.vertex_size()),
            None => Ok(DisplayList::default()),
        }
    }
}

impl<'a, R: ReadPointer<'a> + ?Sized> ReadFrom<R> for PObj<'a> {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let result = Self {
            name: reader.read_pointer()?,
            next: reader.read_pointer()?,
            attributes: reader.read_pointer()?,
            flags: Flags::from_bits_truncate(reader.read_u16::<BE>()?),
            ..Default::default()
        };
        let num_blocks = reader.read_u16::<BE>()?;
        let buffer_size = num_blocks as usize * DISPLAY_LIST_BLOCK_SIZE;
        Ok(Self {
            display_list: ByteArray::read_pointer(reader, buffer_size)?,
            jobj: reader.read_pointer()?,
            ..result
        })
    }
}

impl<'a> Node<'a> for PObj<'a> {}
