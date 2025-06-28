use super::pointer::{NodeBase, ReadPointer, ReadPointerBase};
use super::sobj::SObj;
use super::{Error, Pointer, Result};
use crate::common::{ReadFrom, ReadStructExt, Region};
use bumpalo::Bump;
use byteorder::{ReadBytesExt, BE};
use std::cell::RefCell;
use std::collections::{HashSet, VecDeque};
use std::io::{Read, Seek, SeekFrom};
use std::num::NonZeroU32;
use tracing::trace;

const VERSION_NONE: [u8; 4] = [0; 4];
const VERSION_001B: [u8; 4] = *b"001B";
const HEADER_SIZE: u64 = 0x20;

#[allow(unused)]
struct Header {
    file_size: u32,
    reloc_offset: u32,
    reloc_count: u32,
    root_count: u32,
    ref_count: u32,
    version: [u8; 4],
    unused: [u32; 2],
}

impl<R: Read + ?Sized> ReadFrom<R> for Header {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let file_size = reader.read_u32::<BE>()?;
        let reloc_offset = reader.read_u32::<BE>()?;
        let reloc_count = reader.read_u32::<BE>()?;
        let root_count = reader.read_u32::<BE>()?;
        let ref_count = reader.read_u32::<BE>()?;
        let mut version = [0; 4];
        reader.read_exact(&mut version)?;
        if version != VERSION_NONE && version != VERSION_001B {
            return Err(Error::UnsupportedVersion);
        }
        let mut unused = [0; 2];
        reader.read_u32_into::<BE>(&mut unused)?;
        Ok(Self { file_size, reloc_offset, reloc_count, root_count, ref_count, version, unused })
    }
}

struct QueuedNode<'a> {
    offset: u32,
    node: &'a RefCell<dyn NodeBase<'a>>,
}

struct NodeReader<'a, R: Read + Seek> {
    reader: R,
    arena: &'a Bump,
    header: Header,
    relocs: HashSet<u32>,
    queue: VecDeque<QueuedNode<'a>>,
}

impl<'a, R: Read + Seek> NodeReader<'a, R> {
    fn new(
        reader: R,
        arena: &'a Bump,
        header: Header,
        relocs: impl IntoIterator<Item = u32>,
    ) -> Self {
        Self { reader, arena, header, relocs: relocs.into_iter().collect(), queue: VecDeque::new() }
    }

    fn read_nodes(&mut self) -> Result<()> {
        // Build a list of all the node offsets by reading the offset at each relocation.
        let mut node_offsets = Vec::<u32>::with_capacity(self.relocs.len());
        for &reloc_offset in &self.relocs {
            self.reader.seek(SeekFrom::Start(reloc_offset as u64))?;
            node_offsets.push(self.reader.read_u32::<BE>()?);
        }
        node_offsets.sort();

        while let Some(node) = self.queue.pop_front() {
            // Compute the max size of the node by searching for its offset in the node list and
            // then subtracting it from the offset of the following node. This is necessary for
            // reading buffers with unknown sizes, and it also helps us check correctness. This does
            // make the assumption that pointers will never point to the middle of a node or buffer.
            self.reader.seek(SeekFrom::Start(node.offset as u64))?;
            let next_index = node_offsets.binary_search(&node.offset).map_or_else(|i| i, |i| i + 1);
            let max_size = if next_index < node_offsets.len() {
                node_offsets[next_index] - node.offset
            } else {
                self.header.reloc_offset - node.offset
            };

            trace!("Reading node at 0x{:x} with max size 0x{:x}", node.offset, max_size);
            node.node.borrow_mut().read(self, max_size as usize)?;

            // Validate the node size in debug builds.
            if cfg!(debug_assertions) {
                if let Ok(end_offset) = self.reader.stream_position() {
                    let actual_size = end_offset - node.offset as u64;
                    assert!(
                        actual_size <= max_size as u64,
                        "Actual size (0x{:x}) larger than max size (0x{:x})!",
                        actual_size,
                        max_size
                    );
                }
            }
        }
        Ok(())
    }
}

impl<R: Read + Seek> Read for NodeReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.reader.read(buf)
    }
}

impl<R: Read + Seek> Seek for NodeReader<'_, R> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.reader.seek(pos)
    }
}

impl<'a, R: Read + Seek> ReadPointerBase<'a> for NodeReader<'a, R> {
    fn arena(&self) -> &'a Bump {
        self.arena
    }

    fn read_offset(&mut self) -> Result<Option<NonZeroU32>> {
        // Get the stream position so we can make sure there's a relocation pointing here.
        let pos = self.stream_position()? as u32;
        let offset = self.read_u32::<BE>()?;
        if offset > 0 && self.relocs.contains(&pos) {
            Ok(NonZeroU32::new(offset))
        } else if offset == 0 {
            Ok(None)
        } else {
            Err(Error::MissingRelocation(pos))
        }
    }

    fn add_node(&mut self, offset: u32, node: &'a RefCell<dyn NodeBase<'a>>) {
        self.queue.push_back(QueuedNode { offset, node });
    }
}

#[derive(Debug)]
pub struct Archive<'a> {
    pub roots: Vec<Pointer<'a, SObj<'a>>>,
}

impl<'a> Archive<'a> {
    pub fn read_from<R: Read + Seek + ?Sized>(reader: &mut R, arena: &'a Bump) -> Result<Self> {
        let header = reader.read_struct::<Header>()?;

        // Use a region to put offset 0 after the header so we don't need to remember to add
        // HEADER_SIZE all over the place.
        let mut region = Region::new(reader, HEADER_SIZE, header.file_size as u64 - HEADER_SIZE);

        // Read the relocation table. We only use it to verify the validity of a pointer.
        let mut relocs = vec![0u32; header.reloc_count as usize];
        region.seek(SeekFrom::Start(header.reloc_offset as u64))?;
        region.read_u32_into::<BE>(&mut relocs)?;

        // Read the root node offsets.
        let mut root_offsets = vec![0u32; header.root_count as usize];
        region.read_u32_into::<BE>(&mut root_offsets)?;

        // Enqueue each of the root nodes.
        // TODO: Actually check the type instead of assuming each is an SObj.
        let mut node_reader = NodeReader::new(region, arena, header, relocs);
        let mut roots = vec![];
        for offset in root_offsets {
            roots.push(node_reader.read_node(offset, SObj::default())?);
        }
        node_reader.read_nodes()?;
        Ok(Self { roots })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Cursor;

    #[test]
    #[ignore = "needs to be enabled manually"]
    fn test_read_archive() {
        let bytes = fs::read("../sample.dat").unwrap();
        let mut cursor = Cursor::new(bytes);
        let arena = Bump::new();
        let archive = Archive::read_from(&mut cursor, &arena).unwrap();
        println!("{:?}", archive);
    }
}
