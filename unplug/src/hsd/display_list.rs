use super::pointer::{DefaultIn, NodeBase, ReadPointerBase};
use super::{Node, Result};
use bumpalo::collections::Vec;
use bumpalo::Bump;

#[derive(Clone, PartialEq, Eq)]
pub struct DisplayList<'a> {
    bytes: Vec<'a, u8>,
}

impl<'a> std::fmt::Debug for DisplayList<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DisplayList").field("size", &self.bytes.len()).finish()
    }
}

impl<'a> DisplayList<'a> {
    pub fn new_in(arena: &'a Bump) -> Self {
        Self { bytes: Vec::new_in(arena) }
    }

    pub fn with_blocks(arena: &'a Bump, num_blocks: u16) -> Self {
        let size = num_blocks as usize * 0x20;
        let mut bytes = Vec::new_in(arena);
        bytes.resize(size, 0);
        Self { bytes }
    }
}

impl<'a> DefaultIn<'a> for DisplayList<'a> {
    fn default_in(arena: &'a Bump) -> Self {
        Self::new_in(arena)
    }
}

// Manual impl of NodeBase so we can create a display list with a size and then read it
impl<'a> NodeBase<'a> for DisplayList<'a> {
    fn read<'r>(&mut self, reader: &'r mut dyn ReadPointerBase<'a>) -> Result<()> {
        reader.read_exact(&mut self.bytes)?;
        Ok(())
    }
}

impl<'a> Node<'a> for DisplayList<'a> {}
