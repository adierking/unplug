// NOTE: This is intentionally incomplete and only implemented enough to retrieve sound names from
// New Play Control's cb_robo.brsar. Fully supporting this format is not useful.

use crate::audio::{Error, Result};
use crate::common::{ReadFrom, Region};
use byteorder::{ReadBytesExt, BE};
use log::{debug, error};
use std::any;
use std::ffi::CString;
use std::io::{Read, Seek, SeekFrom};

const BRSAR_MAGIC: u32 = 0x52534152; // 'RSAR'
const BRSAR_VERSION: u16 = 0x0104;

const BIG_ENDIAN: u16 = 0xfeff;

const SECTION_HEADER_SIZE: u64 = 0x8;

/// Reads a count followed by a list of tagged offsets to structs.
fn read_list<T, R, E>(reader: &mut R) -> Result<Vec<T>>
where
    T: ReadFrom<R, Error = E>,
    R: Read + Seek,
    Error: From<E>,
{
    let count = reader.read_u32::<BE>()? as usize;
    let mut offsets = Vec::with_capacity(count);
    for _ in 0..count {
        offsets.push(TaggedOffset::read_from(reader)?);
    }
    let mut results = Vec::with_capacity(count);
    for offset in offsets {
        if let Some(offset) = offset.offset() {
            reader.seek(SeekFrom::Start(offset))?;
            results.push(T::read_from(reader)?);
        } else {
            error!("Invalid {} offset: {:?}", any::type_name::<T>(), offset);
            return Err(Error::InvalidBrsar);
        }
    }
    Ok(results)
}

/// A nullable offset which is tagged with a type value.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TaggedOffset {
    /// A raw memory address. If this appears in a file it is probably expressing a null pointer.
    Pointer(u32),
    /// An offset relative to the start of the current section.
    Relative(u32),
}

impl TaggedOffset {
    /// Gets the offset within the current section if available.
    fn offset(&self) -> Option<u64> {
        match *self {
            Self::Pointer(_) => None,
            Self::Relative(off) => Some(off.into()),
        }
    }
}

impl<R: Read + ?Sized> ReadFrom<R> for TaggedOffset {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let ty = reader.read_u8()?;
        let _pad = reader.read_u24::<BE>()?;
        let val = reader.read_u32::<BE>()?;
        match ty {
            0 => Ok(Self::Pointer(val)),
            1 => Ok(Self::Relative(val)),
            _ => {
                error!("Invalid BRSAR offset type: {}", ty);
                Err(Error::InvalidBrsar)
            }
        }
    }
}

/// The file header.
#[derive(Debug, Copy, Clone)]
struct FileHeader {
    /// Magic number (`BRSAR_MAGIC`).
    _magic: u32,
    /// 0xfeff if big endian, 0xfffe if little endian
    _endian: u16,
    /// File version (we expect 0x104).
    _version: u16,
    /// Total file size.
    _total_size: u32,
    /// Size of this header.
    _header_size: u16,
    /// Number of sections in the file.
    _num_sections: u16,
    /// Offset to the SYMB section.
    symb_offset: u32,
    /// Size of the SYMB section.
    _symb_size: u32,
    /// Offset to the INFO section.
    info_offset: u32,
    /// Size of the INFO section.
    _info_size: u32,
    /// Offset to the FILE section.
    _file_offset: u32,
    /// Size of the FILE section.
    _file_size: u32,
}

impl<R: Read + ?Sized> ReadFrom<R> for FileHeader {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let magic = reader.read_u32::<BE>()?;
        if magic != BRSAR_MAGIC {
            error!("Invalid BRSAR magic");
            return Err(Error::InvalidBrsar);
        }
        let endian = reader.read_u16::<BE>()?;
        if endian != BIG_ENDIAN {
            error!("Only big-endian BRSAR is supported");
            return Err(Error::InvalidBrsar);
        }
        let version = reader.read_u16::<BE>()?;
        if version != BRSAR_VERSION {
            error!("Only BRSAR version {:#x} is supported", BRSAR_VERSION);
            return Err(Error::InvalidBrsar);
        }
        let total_size = reader.read_u32::<BE>()?;
        let header_size = reader.read_u16::<BE>()?;
        let num_sections = reader.read_u16::<BE>()?;
        if num_sections < 3 {
            error!("BRSAR data must have at least 3 sections");
            return Err(Error::InvalidBrsar);
        }
        Ok(Self {
            _magic: magic,
            _endian: endian,
            _version: version,
            _total_size: total_size,
            _header_size: header_size,
            _num_sections: num_sections,
            symb_offset: reader.read_u32::<BE>()?,
            _symb_size: reader.read_u32::<BE>()?,
            info_offset: reader.read_u32::<BE>()?,
            _info_size: reader.read_u32::<BE>()?,
            _file_offset: reader.read_u32::<BE>()?,
            _file_size: reader.read_u32::<BE>()?,
        })
    }
}

/// The header at the beginning of each BRSAR section.
#[derive(Debug, Copy, Clone)]
struct SectionHeader {
    /// The section identifier.
    _magic: u32,
    /// The size of the section data, including this header.
    size: u32,
}

impl<R: Read + ?Sized> ReadFrom<R> for SectionHeader {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self { _magic: reader.read_u32::<BE>()?, size: reader.read_u32::<BE>()? })
    }
}

/// The header at the beginning of the SYMB section.
#[derive(Debug, Copy, Clone)]
struct SymbHeader {
    /// The offset to the list of string offsets.
    names_offset: u32,
    _mask_offset_1: u32,
    _mask_offset_2: u32,
    _mask_offset_3: u32,
    _mask_offset_4: u32,
}

impl<R: Read + ?Sized> ReadFrom<R> for SymbHeader {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self {
            names_offset: reader.read_u32::<BE>()?,
            _mask_offset_1: reader.read_u32::<BE>()?,
            _mask_offset_2: reader.read_u32::<BE>()?,
            _mask_offset_3: reader.read_u32::<BE>()?,
            _mask_offset_4: reader.read_u32::<BE>()?,
        })
    }
}

/// The SYMB section, containing strings referenced elsewhere in the file.
#[derive(Debug, Clone)]
struct SymbSection {
    _header: SymbHeader,
    names: Vec<String>,
}

impl<R: Read + Seek + ?Sized> ReadFrom<R> for SymbSection {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        // Wrap the reader in a region which locks it to the section data. All seeks will now be
        // relative to the start of the section.
        let section_header = SectionHeader::read_from(reader)?;
        let start_offset = reader.seek(SeekFrom::Current(0))?;
        let mut section =
            Region::new(reader, start_offset, section_header.size as u64 - SECTION_HEADER_SIZE);
        let header = SymbHeader::read_from(&mut section)?;

        // Unfortunately we can't use `read_list()` here because the offsets aren't tagged...
        section.seek(SeekFrom::Start(header.names_offset as u64))?;
        let num_names = section.read_u32::<BE>()? as usize;
        let mut name_offsets = Vec::with_capacity(num_names);
        for _ in 0..num_names {
            name_offsets.push(section.read_u32::<BE>()?);
        }
        let mut names = Vec::with_capacity(num_names);
        for offset in name_offsets {
            section.seek(SeekFrom::Start(offset as u64))?;
            let name = CString::read_from(&mut section)?.to_string_lossy().into_owned();
            names.push(name);
        }

        Ok(Self { _header: header, names })
    }
}

/// The header at the beginning of the INFO section.
#[derive(Debug, Copy, Clone)]
struct InfoHeader {
    /// The offset to the sound list.
    sounds: TaggedOffset,
    /// The offset to the bank list.
    _banks: TaggedOffset,
    /// The offset to the players list.
    _players: TaggedOffset,
    /// The offset to the collections list.
    collections: TaggedOffset,
    /// The offset to the groups list.
    groups: TaggedOffset,
    /// The offset to the sound counts list.
    _sound_counts: TaggedOffset,
}

impl<R: Read + ?Sized> ReadFrom<R> for InfoHeader {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self {
            sounds: TaggedOffset::read_from(reader)?,
            _banks: TaggedOffset::read_from(reader)?,
            _players: TaggedOffset::read_from(reader)?,
            collections: TaggedOffset::read_from(reader)?,
            groups: TaggedOffset::read_from(reader)?,
            _sound_counts: TaggedOffset::read_from(reader)?,
        })
    }
}

/// Defines a sound.
#[derive(Debug, Copy, Clone)]
pub struct Sound {
    pub name_index: i32,
    pub collection_index: u32,
    pub player_index: u32,
    pub info: TaggedOffset,
    pub volume: u8,
    pub priority: u8,
    pub kind: u8,
    pub remote_filter: u8,
    pub unk_18: u8,
    pub control_type: u8,
    pub offset2: u32,
    pub user1: u32,
    pub user2: u32,
    pub pan_mode: u8,
    pub pan_curve: u8,
    pub actor_player_index: u8,
}

impl<R: Read + ?Sized> ReadFrom<R> for Sound {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let sound = Sound {
            name_index: reader.read_i32::<BE>()?,
            collection_index: reader.read_u32::<BE>()?,
            player_index: reader.read_u32::<BE>()?,
            info: TaggedOffset::read_from(reader)?,
            volume: reader.read_u8()?,
            priority: reader.read_u8()?,
            kind: reader.read_u8()?,
            remote_filter: reader.read_u8()?,
            unk_18: reader.read_u8()?,
            control_type: reader.read_u8()?,
            offset2: {
                let _pad = reader.read_u16::<BE>()?;
                reader.read_u32::<BE>()?
            },
            user1: reader.read_u32::<BE>()?,
            user2: reader.read_u32::<BE>()?,
            pan_mode: reader.read_u8()?,
            pan_curve: reader.read_u8()?,
            actor_player_index: reader.read_u8()?,
        };
        let _pad = reader.read_u8()?;
        Ok(sound)
    }
}

/// The header for a collection entry.
#[derive(Debug, Copy, Clone)]
pub struct CollectionHeader {
    pub file_length: u32,
    pub audio_length: u32,
    pub entry_num: i32,
    pub external_name: TaggedOffset,
    pub group_links: TaggedOffset,
}

impl<R: Read + ?Sized> ReadFrom<R> for CollectionHeader {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self {
            file_length: reader.read_u32::<BE>()?,
            audio_length: reader.read_u32::<BE>()?,
            entry_num: reader.read_i32::<BE>()?,
            external_name: TaggedOffset::read_from(reader)?,
            group_links: TaggedOffset::read_from(reader)?,
        })
    }
}

/// Links a collection to a group.
#[derive(Debug, Copy, Clone)]
pub struct CollectionGroupLink {
    pub index: u32,
    pub sub_index: u32,
}

impl<R: Read + ?Sized> ReadFrom<R> for CollectionGroupLink {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self { index: reader.read_u32::<BE>()?, sub_index: reader.read_u32::<BE>()? })
    }
}

/// Defines a collection.
#[derive(Debug, Clone)]
pub struct Collection {
    pub header: CollectionHeader,
    pub groups: Vec<CollectionGroupLink>,
}

impl Collection {
    fn read_from(reader: &mut (impl Read + Seek), header: CollectionHeader) -> Result<Self> {
        let mut groups = vec![];
        if let Some(offset) = header.group_links.offset() {
            reader.seek(SeekFrom::Start(offset))?;
            groups = read_list(reader)?;
        }
        Ok(Self { header, groups })
    }
}

/// Defines a group.
#[derive(Debug, Copy, Clone)]
pub struct Group {
    pub name_index: i32,
    pub unk_04: i32,
    pub unk_08: u32,
    pub unk_0c: u32,
    pub rwsd_offset: u32,
    pub rwsd_size: u32,
    pub rwar_offset: u32,
    pub rwar_size: u32,
    pub unk_20: TaggedOffset,
}

impl<R: Read + ?Sized> ReadFrom<R> for Group {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self {
            name_index: reader.read_i32::<BE>()?,
            unk_04: reader.read_i32::<BE>()?,
            unk_08: reader.read_u32::<BE>()?,
            unk_0c: reader.read_u32::<BE>()?,
            rwsd_offset: reader.read_u32::<BE>()?,
            rwsd_size: reader.read_u32::<BE>()?,
            rwar_offset: reader.read_u32::<BE>()?,
            rwar_size: reader.read_u32::<BE>()?,
            unk_20: TaggedOffset::read_from(reader)?,
        })
    }
}

/// The INFO section, containing definitions for everything in the BRSAR.
#[derive(Debug, Clone)]
struct InfoSection {
    sounds: Vec<Sound>,
    collections: Vec<Collection>,
    groups: Vec<Group>,
}

impl<R: Read + Seek + ?Sized> ReadFrom<R> for InfoSection {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        // Wrap the reader in a region which locks it to the section data. All seeks will now be
        // relative to the start of the section.
        let section_header = SectionHeader::read_from(reader)?;
        let start_offset = reader.seek(SeekFrom::Current(0))?;
        let mut section =
            Region::new(reader, start_offset, section_header.size as u64 - SECTION_HEADER_SIZE);
        let header = InfoHeader::read_from(&mut section)?;

        let mut sounds = vec![];
        if let Some(offset) = header.sounds.offset() {
            section.seek(SeekFrom::Start(offset))?;
            sounds = read_list(&mut section)?;
        }

        let mut collections = vec![];
        if let Some(offset) = header.collections.offset() {
            section.seek(SeekFrom::Start(offset))?;
            let headers: Vec<CollectionHeader> = read_list(&mut section)?;
            collections.reserve_exact(headers.len());
            for header in headers {
                collections.push(Collection::read_from(&mut section, header)?);
            }
        }

        let mut groups = vec![];
        if let Some(offset) = header.groups.offset() {
            section.seek(SeekFrom::Start(offset))?;
            groups = read_list(&mut section)?;
        }

        Ok(Self { sounds, collections, groups })
    }
}

/// A Binary Revolution Sound ARchive (BRSAR), containing sounds used by Wii games.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Brsar {
    pub symbols: Vec<String>,
    pub sounds: Vec<Sound>,
    pub collections: Vec<Collection>,
    pub groups: Vec<Group>,
}

impl Brsar {
    /// Gets the symbol name at `index`. Returns an empty string if the index is negative.
    pub fn symbol(&self, index: i32) -> &str {
        if index >= 0 {
            &self.symbols[index as usize]
        } else {
            ""
        }
    }
}

impl<R: Read + Seek + ?Sized> ReadFrom<R> for Brsar {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let header = FileHeader::read_from(reader)?;

        reader.seek(SeekFrom::Start(header.symb_offset as u64))?;
        let symb = SymbSection::read_from(reader)?;

        reader.seek(SeekFrom::Start(header.info_offset as u64))?;
        let info = InfoSection::read_from(reader)?;

        debug!(
            "Loaded BRSAR with {} sounds, {} collections, and {} groups",
            info.sounds.len(),
            info.collections.len(),
            info.groups.len()
        );
        Ok(Self {
            symbols: symb.names,
            sounds: info.sounds,
            collections: info.collections,
            groups: info.groups,
        })
    }
}
