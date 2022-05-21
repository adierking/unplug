use super::banner::{self, Banner};
use super::dol::{self, DolHeader};
use super::fst::{self, EditFile, EntryId, FileStringTable, FileTree, FstEntryKind, OpenFile};
use crate::common::io::{copy_within, fill, read_fixed_string, write_fixed_string};
use crate::common::{self, ReadFrom, ReadSeek, ReadWriteSeek, Region, WriteTo};
use byteorder::{ReadBytesExt, WriteBytesExt, BE};
use encoding_rs::mem;
use std::cmp;
use std::convert::TryFrom;
use std::ffi::CString;
use std::fmt;
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use thiserror::Error;
use tracing::{debug, trace};

const GCN_MAGIC: u32 = 0xc2339f3d;
const WII_MAGIC: u32 = 0x5d1c9ea3;

// Optimal alignment is on 32KB boundaries (cluster size)
// PiA doesn't actually use this but some other games (e.g. Melee) do
// Aligning files also leaves some room for adjacent files to grow
const DVD_OPTIMAL_ALIGN: u32 = 0x8000;

/// Path to the banner file that the GameCube looks for.
const BANNER_PATH: &str = "opening.bnr";

/// The result type for disc operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for disc operations.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("invalid DVD magic")]
    InvalidMagic,

    #[error("overlapping move region")]
    OverlappingMove,

    #[error("not enough space available in disc (need {0:#x} bytes)")]
    NotEnoughSpace(u32),

    #[error("Wii DVDs are not supported")]
    WiiNotSupported,

    #[error(transparent)]
    Banner(Box<banner::Error>),

    #[error(transparent)]
    Fst(Box<fst::Error>),

    #[error(transparent)]
    Dol(Box<dol::Error>),

    #[error(transparent)]
    Io(Box<io::Error>),
}

from_error_boxed!(Error::Banner, banner::Error);
from_error_boxed!(Error::Fst, fst::Error);
from_error_boxed!(Error::Dol, dol::Error);
from_error_boxed!(Error::Io, io::Error);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct DiscHeader {
    game_code: [u8; 4],
    maker_code: [u8; 2],
    disc_id: u8,
    version: u8,
    audio_streaming: u8,
    stream_buffer_size: u8,
    unused_00a: [u8; 0x0e],
    wii_magic: u32,
    gcn_magic: u32,
    game_name: CString,
    debug_monitor_offset: u32,
    debug_monitor_addr: u32,
    unused_408: [u8; 0x18],
    dol_offset: u32,
    fst_offset: u32,
    fst_size: u32,
    fst_max_size: u32,
    user_position: u32,
    user_size: u32,
    disc_size: u32,
    unused_43c: u32,
}

impl DiscHeader {
    /// Constructs a new `DiscHeader` with all fields initialized to zero.
    fn new() -> Self {
        Self::default()
    }
}

impl<R: Read + ?Sized> ReadFrom<R> for DiscHeader {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let mut header = Self::new();
        reader.read_exact(&mut header.game_code)?;
        reader.read_exact(&mut header.maker_code)?;
        header.disc_id = reader.read_u8()?;
        header.version = reader.read_u8()?;
        header.audio_streaming = reader.read_u8()?;
        header.stream_buffer_size = reader.read_u8()?;
        reader.read_exact(&mut header.unused_00a)?;
        header.wii_magic = reader.read_u32::<BE>()?;
        if header.wii_magic == WII_MAGIC {
            return Err(Error::WiiNotSupported);
        }
        header.gcn_magic = reader.read_u32::<BE>()?;
        if header.gcn_magic != GCN_MAGIC {
            return Err(Error::InvalidMagic);
        }
        header.game_name = read_fixed_string(&mut *reader, 0x3e0)?;
        header.debug_monitor_offset = reader.read_u32::<BE>()?;
        header.debug_monitor_addr = reader.read_u32::<BE>()?;
        reader.read_exact(&mut header.unused_408)?;
        header.dol_offset = reader.read_u32::<BE>()?;
        header.fst_offset = reader.read_u32::<BE>()?;
        header.fst_size = reader.read_u32::<BE>()?;
        header.fst_max_size = reader.read_u32::<BE>()?;
        header.user_position = reader.read_u32::<BE>()?;
        header.user_size = reader.read_u32::<BE>()?;
        header.disc_size = reader.read_u32::<BE>()?;
        header.unused_43c = reader.read_u32::<BE>()?;
        Ok(header)
    }
}

impl<W: Write + ?Sized> WriteTo<W> for DiscHeader {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_all(&self.game_code)?;
        writer.write_all(&self.maker_code)?;
        writer.write_u8(self.disc_id)?;
        writer.write_u8(self.version)?;
        writer.write_u8(self.audio_streaming)?;
        writer.write_u8(self.stream_buffer_size)?;
        writer.write_all(&self.unused_00a)?;
        writer.write_u32::<BE>(self.wii_magic)?;
        writer.write_u32::<BE>(self.gcn_magic)?;
        write_fixed_string(&mut *writer, &self.game_name, 0x3e0)?;
        writer.write_u32::<BE>(self.debug_monitor_offset)?;
        writer.write_u32::<BE>(self.debug_monitor_addr)?;
        writer.write_all(&self.unused_408)?;
        writer.write_u32::<BE>(self.dol_offset)?;
        writer.write_u32::<BE>(self.fst_offset)?;
        writer.write_u32::<BE>(self.fst_size)?;
        writer.write_u32::<BE>(self.fst_max_size)?;
        writer.write_u32::<BE>(self.user_position)?;
        writer.write_u32::<BE>(self.user_size)?;
        writer.write_u32::<BE>(self.disc_size)?;
        writer.write_u32::<BE>(self.unused_43c)?;
        Ok(())
    }
}

/// A stream for reading and manipulating GameCube DVD data (e.g. a .iso).
pub struct DiscStream<S: ReadSeek> {
    header: Box<DiscHeader>,
    free_regions: Vec<DiscRegion>,
    pub stream: S,
    pub files: FileTree,
}

impl<S: ReadSeek> DiscStream<S> {
    /// Constructs a new `DiscStream` which reads existing disc data from `stream`.
    /// `DiscStream` does its own buffering, so `stream` should not be buffered.
    pub fn open(stream: S) -> Result<Self> {
        let mut stream = BufReader::new(stream);
        stream.seek(SeekFrom::Start(0))?;
        let header = DiscHeader::read_from(&mut stream)?;
        debug!("Found FST at {:#x} (size = {:#x})", header.fst_offset, header.fst_size);
        stream.seek(SeekFrom::Start(header.fst_offset as u64))?;
        let fst = FileStringTable::read_from(&mut stream.by_ref().take(header.fst_size as u64))?;
        let free_regions = Self::find_free_regions(&header, &fst);
        let files = FileTree::from_fst(&fst)?;
        Ok(Self { header: header.into(), free_regions, stream: stream.into_inner(), files })
    }

    /// Returns the disc's game ID (e.g. "GGTE01").
    pub fn game_id(&self) -> String {
        let mut game_id = [0u8; 6];
        game_id[..4].copy_from_slice(&self.header.game_code);
        game_id[4..].copy_from_slice(&self.header.maker_code);
        mem::decode_latin1(&game_id).into()
    }

    /// Returns the DOL header and a stream that can be used to read the main.dol file.
    pub fn open_dol(&mut self) -> Result<(DolHeader, Box<dyn ReadSeek + '_>)> {
        let start = self.header.dol_offset as u64;
        self.stream.seek(SeekFrom::Start(start))?;
        let header = DolHeader::read_from(&mut BufReader::new(&mut self.stream))?;
        let len = header.file_size() as u64;
        let region: Box<dyn ReadSeek> = Box::new(Region::new(&mut self.stream, start, len));
        Ok((header, region))
    }

    /// Returns a list of unused areas in the disc sorted by offset.
    pub fn free_regions(&self) -> &[DiscRegion] {
        &self.free_regions
    }

    /// Reads the disc's banner file.
    pub fn read_banner(&mut self) -> Result<Banner> {
        let mut reader = BufReader::new(self.open_file_at(BANNER_PATH)?);
        Ok(Banner::read_from(&mut reader)?)
    }

    /// Returns the maximum amount of space that a file can be grown to occupy without overwriting
    /// in-use data on the disc.
    pub fn max_file_size(&self, id: EntryId) -> Result<u32> {
        let file = self.files.file(id)?;
        Ok(self.max_region_size(file.offset, file.size))
    }

    /// Returns the maximum amount of space that a region can be grown to occupy without overwriting
    /// in-use data on the disc.
    pub fn max_region_size(&self, offset: u32, size: u32) -> u32 {
        match self.free_regions.binary_search_by_key(&offset, |r| r.offset) {
            Ok(i) => self.free_regions[i].size,
            Err(i) => {
                // i is the index of the next free region
                let end = offset + size;
                if i > 0 {
                    // Check if the region is inside a free region
                    let prev = self.free_regions[i - 1];
                    let prev_end = prev.offset + prev.size;
                    if offset >= prev.offset && end < prev_end {
                        return prev_end - offset;
                    }
                }
                if i < self.free_regions.len() {
                    // If a free region immediately follows, add its size onto ours
                    let next = self.free_regions[i];
                    if next.offset == end {
                        return size + next.size;
                    }
                }
                size
            }
        }
    }

    /// Finds the largest free region in the disc which has `size` bytes available and whose offset
    /// is aligned to the power-of-two `align`.
    ///
    /// The returned region may be larger than the requested size if more free space is available.
    pub fn allocate(&self, size: u32, align: u32) -> Result<DiscRegion> {
        self.free_regions
            .iter()
            .filter_map(|r| {
                let aligned_offset = common::align(r.offset, align);
                let padding = aligned_offset - r.offset;
                if padding >= r.size {
                    return None;
                }
                let aligned_size = r.size - padding;
                if aligned_size >= size {
                    Some(DiscRegion::new(aligned_offset, aligned_size))
                } else {
                    None
                }
            })
            .max_by_key(|r| r.size)
            .ok_or(Error::NotEnoughSpace(size))
    }

    /// Calculates a list of unused areas in the disc sorted by offset.
    fn find_free_regions(header: &DiscHeader, fst: &FileStringTable) -> Vec<DiscRegion> {
        // Collect all of the regions used by each file
        let mut used_regions: Vec<_> = fst
            .entries
            .iter()
            .filter(|e| e.kind == FstEntryKind::File && e.size_or_next > 0)
            .map(|e| DiscRegion::new(e.offset_or_parent, e.size_or_next))
            .collect();

        // Assume that everything up to and including the FST is unusable
        used_regions.push(DiscRegion::new(0, header.fst_offset + header.fst_size));

        // Add a zero-size region for the end of the disc so that we cover free space at the end
        let disc_end = header.user_size + header.disc_size;
        used_regions.push(DiscRegion::new(disc_end, 0));

        // Sort by offset and then invert the list
        used_regions.sort_unstable_by_key(|r| r.offset);
        let mut free_regions = vec![];
        for regions in used_regions.windows(2) {
            let offset = regions[0].offset + regions[0].size;
            let size = regions[1].offset - offset;
            if size != 0 {
                free_regions.push(DiscRegion::new(offset, size));
            }
        }
        free_regions
    }
}

impl<S: ReadWriteSeek> DiscStream<S> {
    /// Moves file `id` to `new_offset` and updates the FST.
    pub fn move_file(&mut self, id: EntryId, new_offset: u32) -> Result<()> {
        let size = self.files.file(id)?.size;
        self.move_and_resize_file(id, new_offset, size)
    }

    /// Moves file `id` to `new_offset`, resizes it to `new_size`, and updates the FST.
    pub fn move_and_resize_file(
        &mut self,
        id: EntryId,
        new_offset: u32,
        new_size: u32,
    ) -> Result<()> {
        let file = self.files.file(id)?;
        let old_offset = file.offset;
        let old_size = file.size;
        debug!(
            "Moving {} (size {:#x}) from {:#x} to {:#x}",
            file.name, old_size, old_offset, new_offset
        );

        // Supporting overlapping regions would make this quite a bit more complicated. For example,
        // we'd have to somehow ignore this file during the max_region_size() calculation.
        if new_offset < old_offset + old_size && new_offset + new_size > old_offset {
            return Err(Error::OverlappingMove);
        }

        let available = self.max_region_size(new_offset, 0);
        if new_size > available {
            return Err(Error::NotEnoughSpace(new_size));
        }

        // Copy the file data first
        trace!("Copy begin");
        copy_within(
            &mut self.stream,
            old_offset as u64,
            cmp::min(old_size, new_size) as u64,
            new_offset as u64,
        )?;
        trace!("Copy end");

        // Update the FST to point to the moved data
        let file = self.files.file_mut(id).unwrap();
        file.offset = new_offset;
        file.size = new_size;
        self.commit_file_tree()?;

        // The FST was successfully updated, so we can now safely wipe the old region
        self.stream.seek(SeekFrom::Start(old_offset as u64))?;
        fill(&mut self.stream, 0, old_size as u64)?;
        Ok(())
    }

    /// Resizes file `id` to `new_size`, potentially moving it to a new location if there is no space
    /// available.
    pub fn resize_file(&mut self, id: EntryId, new_size: u32) -> Result<()> {
        let file = self.files.file(id)?;
        let old_offset = file.offset;
        let old_size = file.size;
        if new_size == old_size {
            return Ok(());
        }
        debug!("Resizing {} from {:#x} to {:#x}", file.name, old_size, new_size);

        // If the file is growing, make sure we have enough space for it and move it if not
        if new_size > old_size {
            let max_size = self.max_region_size(old_offset, old_size);
            if new_size > max_size {
                let new_offset = self.allocate(new_size, DVD_OPTIMAL_ALIGN)?.offset;
                self.move_and_resize_file(id, new_offset, new_size)?;
                return Ok(());
            }
        }

        let file = self.files.file_mut(id).unwrap();
        file.size = new_size;
        self.commit_file_tree()?;

        // If the file shrunk, zero out the removed data at the end
        if new_size < old_size {
            let fill_offset = old_offset + new_size;
            let fill_size = old_size - new_size;
            debug!("Clearing {:#x} bytes at {:#x}", fill_size, fill_offset);
            self.stream.seek(SeekFrom::Start(fill_offset as u64))?;
            fill(&mut self.stream, 0, fill_size as u64)?;
        }
        Ok(())
    }

    /// Replaces file `id` with the contents of `reader`.
    pub fn replace_file(&mut self, id: EntryId, mut reader: (impl Read + Seek)) -> Result<()> {
        let start_pos = reader.seek(SeekFrom::Current(0))?;
        let end_pos = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(start_pos))?;

        let size = u32::try_from(end_pos - start_pos).expect("File size overflow");
        self.resize_file(id, size)?;

        debug!("Writing {:#x} bytes to {}", size, self.files.file(id).unwrap().name);
        let mut writer = self.edit_file(id)?;
        std::io::copy(&mut reader, &mut writer)?;
        Ok(())
    }

    /// Replaces the file at `path` with the contents of `reader`.
    pub fn replace_file_at(&mut self, path: &str, reader: (impl Read + Seek)) -> Result<()> {
        self.replace_file(self.files.at(path)?, reader)
    }

    /// Rebuilds the disc's File String Table (FST), writes it out, and updates internal state.
    pub fn commit_file_tree(&mut self) -> Result<()> {
        debug!("Rebuilding disc FST");
        let (fst, _) = self.files.to_fst()?;

        // Make sure the FST can fit
        let disk_size = fst.disk_size();
        let max_fst_size = self.max_region_size(self.header.fst_offset, self.header.fst_size);
        if disk_size > max_fst_size {
            return Err(Error::NotEnoughSpace(disk_size));
        }

        // Write the new FST
        self.stream.seek(SeekFrom::Start(self.header.fst_offset as u64))?;
        let mut buf = BufWriter::new(&mut self.stream);
        fst.write_to(&mut buf)?;
        buf.flush()?;
        drop(buf);

        // If it got smaller, zero out the space at the end
        if disk_size < self.header.fst_size {
            let fill_size = (self.header.fst_size - disk_size) as u64;
            fill(&mut self.stream, 0, fill_size)?;
        }

        // Update the FST size in the header if it changed
        if disk_size != self.header.fst_size {
            let mut new_header = self.header.clone();
            new_header.fst_size = disk_size;
            self.stream.seek(SeekFrom::Start(0))?;
            let mut buf = BufWriter::new(&mut self.stream);
            new_header.write_to(&mut buf)?;
            buf.flush()?;
            self.header = new_header;
        }

        // Everything succeeded, so update our free list
        self.free_regions = Self::find_free_regions(&self.header, &fst);
        Ok(())
    }
}

impl<S: ReadSeek> OpenFile for DiscStream<S> {
    fn open_file(&mut self, id: EntryId) -> fst::Result<Box<dyn ReadSeek + '_>> {
        self.files.file(id)?.open(&mut self.stream)
    }

    fn into_file<'s>(self, id: EntryId) -> fst::Result<Box<dyn ReadSeek + 's>>
    where
        Self: 's,
    {
        self.files.file(id)?.open(self.stream)
    }

    fn open_file_at(&mut self, path: &str) -> fst::Result<Box<dyn ReadSeek + '_>> {
        self.files.file_at(path)?.open(&mut self.stream)
    }

    fn into_file_at<'s>(self, path: &str) -> fst::Result<Box<dyn ReadSeek + 's>>
    where
        Self: 's,
    {
        self.files.file_at(path)?.open(self.stream)
    }
}

impl<S: ReadWriteSeek> EditFile for DiscStream<S> {
    fn edit_file(&mut self, id: EntryId) -> fst::Result<Box<dyn ReadWriteSeek + '_>> {
        let file = self.files.file(id)?;
        let max_size = self.max_region_size(file.offset, file.size);
        Ok(file.edit(&mut self.stream, max_size))
    }

    fn edit_file_at(&mut self, path: &str) -> fst::Result<Box<dyn ReadWriteSeek + '_>> {
        let file = self.files.file_at(path)?;
        let max_size = self.max_region_size(file.offset, file.size);
        Ok(file.edit(&mut self.stream, max_size))
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct DiscRegion {
    pub offset: u32,
    pub size: u32,
}

impl DiscRegion {
    pub fn new(offset: u32, size: u32) -> Self {
        Self { offset, size }
    }
}

impl fmt::Debug for DiscRegion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({:#x}, {:#x})", self.offset, self.size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_write_and_read;

    #[test]
    fn test_write_and_read_disc_header() {
        assert_write_and_read!(DiscHeader {
            game_code: [0; 4],
            maker_code: [1; 2],
            disc_id: 2,
            version: 3,
            audio_streaming: 4,
            stream_buffer_size: 5,
            unused_00a: [6; 0x0e],
            wii_magic: 0,
            gcn_magic: GCN_MAGIC,
            game_name: CString::new("test").unwrap(),
            debug_monitor_offset: 8,
            debug_monitor_addr: 9,
            unused_408: [10; 0x18],
            dol_offset: 11,
            fst_offset: 12,
            fst_size: 13,
            fst_max_size: 14,
            user_position: 15,
            user_size: 16,
            disc_size: 17,
            unused_43c: 18,
        });
    }
}
