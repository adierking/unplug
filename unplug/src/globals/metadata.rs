use super::{Error, Result};
use crate::common::{ReadFrom, WriteTo};
use bitflags::bitflags;
use byteorder::{ByteOrder, ReadBytesExt, WriteBytesExt, BE, LE};
use std::convert::TryInto;
use std::ffi::{CStr, CString};
use std::io::{self, Read, Seek, SeekFrom, Write};
use unplug_data::stage::NUM_REAL_STAGES;

const NUM_PICKUP_SOUNDS: usize = 4;
const NUM_COLLECT_SOUNDS: usize = 3;
const NUM_ITEMS: usize = 159;
const NUM_ACTORS: usize = 64;
const NUM_ATCS: usize = 9;
const NUM_SUITS: usize = 9;
const NUM_LETICKERS: usize = 20;
const NUM_STICKERS: usize = 24;
const NUM_STATS: usize = 13;

/// The total number of strings inside metadata, for initializing collection capacities.
const NUM_STRINGS: usize = 583;

/// Wraps a reader and adds support for reading and caching string offsets.
/// The actual strings can be read later using `read_next_string()` to minimize the number of seeks.
struct StringReader<R: Read> {
    inner: R,
    /// The list of offsets that were read.
    offsets: Vec<u32>,
    /// The index of the next unread string.
    index: usize,
}

impl<R: Read> StringReader<R> {
    /// Constructs a new `StringReader<R>` which wraps `inner`.
    fn new(inner: R) -> Self {
        Self { inner, offsets: Vec::with_capacity(NUM_STRINGS), index: 0 }
    }

    /// Reads a 32-bit string offset and caches it.
    /// Returns an empty `CString` for convenience purposes.
    fn read_string_offset(&mut self) -> io::Result<CString> {
        let offset = self.inner.read_u32::<LE>()?;
        self.offsets.push(offset);
        Ok(CString::default())
    }
}

impl<R: Read + Seek> StringReader<R> {
    /// Reads the next unread string (in order the offsets were read).
    fn read_next_string(&mut self) -> io::Result<CString> {
        assert!(self.index < self.offsets.len(), "no more unread strings");
        let offset = self.offsets[self.index];
        self.index += 1;
        self.inner.seek(SeekFrom::Start(offset as u64))?;
        Ok(CString::read_from(&mut self.inner)?)
    }
}

impl<R: Read> Read for StringReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl<R: Read + Seek> Seek for StringReader<R> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.inner.seek(pos)
    }
}

/// Wraps a writer and adds support for writing placeholder string offsets which can be filled in
/// after the strings are actually written.
struct StringWriter<W: Write + Seek> {
    inner: W,
    /// The list of strings that need to be written. The first element of the tuple is the offset of
    /// the string's placeholder offset.
    strings: Vec<(u64, CString)>,
}

impl<W: Write + Seek> StringWriter<W> {
    /// Constructs a new `StringWriter<W>` which wraps `inner`.
    fn new(inner: W) -> Self {
        Self { inner, strings: Vec::with_capacity(NUM_STRINGS) }
    }

    /// Writes a placeholder string offset.
    fn write_string_offset(&mut self, string: &CStr) -> io::Result<()> {
        let offset = self.inner.seek(SeekFrom::Current(0))?;
        self.strings.push((offset, string.to_owned()));
        self.inner.write_u32::<LE>(0)?;
        Ok(())
    }

    /// Writes out the actual strings and fills in the placeholder offsets.
    fn write_strings(&mut self) -> io::Result<()> {
        let mut cur_offset = self.inner.seek(SeekFrom::Current(0))?;
        for (ptr_offset, string) in &self.strings {
            let str_offset = cur_offset;
            string.write_to(&mut self.inner)?;
            cur_offset += string.as_bytes_with_nul().len() as u64;
            self.inner.seek(SeekFrom::Start(*ptr_offset))?;
            self.inner.write_u32::<LE>(str_offset.try_into().expect("string offset overflow"))?;
            self.inner.seek(SeekFrom::Start(cur_offset))?;
        }
        Ok(())
    }
}

impl<W: Write + Seek> Write for StringWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<W: Write + Seek> Seek for StringWriter<W> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.inner.seek(pos)
    }
}

/// The metadata partition header.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct MetadataHeader {
    unk_00_offset: u32,
    unk_04_offset: u32,
    unk_08_offset: u32,
    unk_0c_offset: u32,
    unk_10_offset: u32,
    unk_14_offset: u32,
    unk_18_offset: u32,
    unk_1c_offset: u32,
    pickup_sounds_offset: u32,
    collect_sounds_offset: u32,
    items_offset: u32,
    actors_offset: u32,
    atcs_offset: u32,
    suits_offset: u32,
    stages_offset: u32,
    unused_3c: u32,
    unused_40: u32,
    unused_44: u32,
    letickers_offset: u32,
    stickers_offset: u32,
    stats_offset: u32,
}

impl<R: Read> ReadFrom<R> for MetadataHeader {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self {
            unk_00_offset: reader.read_u32::<LE>()?,
            unk_04_offset: reader.read_u32::<LE>()?,
            unk_08_offset: reader.read_u32::<LE>()?,
            unk_0c_offset: reader.read_u32::<LE>()?,
            unk_10_offset: reader.read_u32::<LE>()?,
            unk_14_offset: reader.read_u32::<LE>()?,
            unk_18_offset: reader.read_u32::<LE>()?,
            unk_1c_offset: reader.read_u32::<LE>()?,
            pickup_sounds_offset: reader.read_u32::<LE>()?,
            collect_sounds_offset: reader.read_u32::<LE>()?,
            items_offset: reader.read_u32::<LE>()?,
            actors_offset: reader.read_u32::<LE>()?,
            atcs_offset: reader.read_u32::<LE>()?,
            suits_offset: reader.read_u32::<LE>()?,
            stages_offset: reader.read_u32::<LE>()?,
            unused_3c: reader.read_u32::<LE>()?,
            unused_40: reader.read_u32::<LE>()?,
            unused_44: reader.read_u32::<LE>()?,
            letickers_offset: reader.read_u32::<LE>()?,
            stickers_offset: reader.read_u32::<LE>()?,
            stats_offset: reader.read_u32::<LE>()?,
        })
    }
}

impl<W: Write> WriteTo<W> for MetadataHeader {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_u32::<LE>(self.unk_00_offset)?;
        writer.write_u32::<LE>(self.unk_04_offset)?;
        writer.write_u32::<LE>(self.unk_08_offset)?;
        writer.write_u32::<LE>(self.unk_0c_offset)?;
        writer.write_u32::<LE>(self.unk_10_offset)?;
        writer.write_u32::<LE>(self.unk_14_offset)?;
        writer.write_u32::<LE>(self.unk_18_offset)?;
        writer.write_u32::<LE>(self.unk_1c_offset)?;
        writer.write_u32::<LE>(self.pickup_sounds_offset)?;
        writer.write_u32::<LE>(self.collect_sounds_offset)?;
        writer.write_u32::<LE>(self.items_offset)?;
        writer.write_u32::<LE>(self.actors_offset)?;
        writer.write_u32::<LE>(self.atcs_offset)?;
        writer.write_u32::<LE>(self.suits_offset)?;
        writer.write_u32::<LE>(self.stages_offset)?;
        writer.write_u32::<LE>(self.unused_3c)?;
        writer.write_u32::<LE>(self.unused_40)?;
        writer.write_u32::<LE>(self.unused_44)?;
        writer.write_u32::<LE>(self.letickers_offset)?;
        writer.write_u32::<LE>(self.stickers_offset)?;
        writer.write_u32::<LE>(self.stats_offset)?;
        Ok(())
    }
}

bitflags! {
    #[derive(Default)]
    pub struct ItemFlags: u16 {
        /// The item is junk. (unused)
        const JUNK = 1 << 0;
        /// The item is visible in Chibi-Vision.
        const CHIBI_VISION = 1 << 1;
        /// The item is visible on the inventory screen.
        const INVENTORY = 1 << 2;
    }
}

/// Metadata which describes a collectable item.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Item {
    /// The item's display name.
    pub name: CString,
    /// The item's description (shown in the inventory and the shop).
    pub description: CString,
    /// The item's flags.
    pub flags: ItemFlags,
    /// The amount of time it takes to pick up the item in hundredths of seconds.
    pub pickup_delay: u16,
    /// The item's shop price.
    pub price: u16,
    /// The number of happy points rewarded if the player throws the item away.
    pub junk_exp: u16,
    /// The amount of money rewarded if the player throws the item away.
    pub junk_money: u16,
    /// The `pickup_sounds` index of the sound to play when the item is picked up. (-1 = none)
    pub pickup_sound: i8,
    /// The `collect_sounds` index of the sound to play when the item is collected. (-1 = none)
    pub collect_sound: i8,
}

impl Item {
    /// Constructs an empty `Item`.
    pub fn new() -> Self {
        Self::default()
    }

    fn read_strings(&mut self, reader: &mut StringReader<impl Read + Seek>) -> io::Result<()> {
        self.name = reader.read_next_string()?;
        self.description = reader.read_next_string()?;
        Ok(())
    }
}

impl<R: Read> ReadFrom<StringReader<R>> for Item {
    type Error = Error;
    fn read_from(reader: &mut StringReader<R>) -> Result<Self> {
        Ok(Self {
            name: reader.read_string_offset()?,
            description: reader.read_string_offset()?,
            flags: ItemFlags::from_bits_truncate(reader.read_u16::<BE>()?),
            pickup_delay: reader.read_u16::<BE>()?,
            price: reader.read_u16::<BE>()?,
            junk_exp: reader.read_u16::<BE>()?,
            junk_money: reader.read_u16::<BE>()?,
            pickup_sound: reader.read_i8()?,
            collect_sound: reader.read_i8()?,
        })
    }
}

impl<W: Write + Seek> WriteTo<StringWriter<W>> for Item {
    type Error = Error;
    fn write_to(&self, writer: &mut StringWriter<W>) -> Result<()> {
        writer.write_string_offset(&self.name)?;
        writer.write_string_offset(&self.description)?;
        writer.write_u16::<BE>(self.flags.bits())?;
        writer.write_u16::<BE>(self.pickup_delay)?;
        writer.write_u16::<BE>(self.price)?;
        writer.write_u16::<BE>(self.junk_exp)?;
        writer.write_u16::<BE>(self.junk_money)?;
        if self.pickup_sound >= NUM_PICKUP_SOUNDS as i8 {
            return Err(Error::InvalidPickupSound(self.pickup_sound));
        }
        writer.write_i8(self.pickup_sound)?;
        if self.collect_sound >= NUM_COLLECT_SOUNDS as i8 {
            return Err(Error::InvalidCollectSound(self.collect_sound));
        }
        writer.write_i8(self.collect_sound)?;
        Ok(())
    }
}

/// Metadata which describes an actor.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Actor {
    /// The actor's display name.
    pub name: CString,
}

impl Actor {
    /// Constructs an empty `Actor`.
    pub fn new() -> Self {
        Self::default()
    }

    fn read_strings(&mut self, reader: &mut StringReader<impl Read + Seek>) -> io::Result<()> {
        self.name = reader.read_next_string()?;
        Ok(())
    }
}

impl<R: Read> ReadFrom<StringReader<R>> for Actor {
    type Error = Error;
    fn read_from(reader: &mut StringReader<R>) -> Result<Self> {
        Ok(Self { name: reader.read_string_offset()? })
    }
}

impl<W: Write + Seek> WriteTo<StringWriter<W>> for Actor {
    type Error = Error;
    fn write_to(&self, writer: &mut StringWriter<W>) -> Result<()> {
        writer.write_string_offset(&self.name)?;
        Ok(())
    }
}

/// Metadata which describes an attachment (ATC) item.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Atc {
    /// The attachment's display name.
    pub name: CString,
    /// The attachment's description (unused?).
    pub description: CString,
    /// The attachment's shop price.
    pub price: u16,
}

impl Atc {
    /// Constructs an empty `Atc`.
    pub fn new() -> Self {
        Self::default()
    }

    fn read_strings(&mut self, reader: &mut StringReader<impl Read + Seek>) -> io::Result<()> {
        self.name = reader.read_next_string()?;
        self.description = reader.read_next_string()?;
        Ok(())
    }
}

impl<R: Read> ReadFrom<StringReader<R>> for Atc {
    type Error = Error;
    fn read_from(reader: &mut StringReader<R>) -> Result<Self> {
        let atc = Self {
            name: reader.read_string_offset()?,
            description: reader.read_string_offset()?,
            price: reader.read_u16::<BE>()?,
        };
        reader.read_u16::<BE>()?; // Padding
        Ok(atc)
    }
}

impl<W: Write + Seek> WriteTo<StringWriter<W>> for Atc {
    type Error = Error;
    fn write_to(&self, writer: &mut StringWriter<W>) -> Result<()> {
        writer.write_string_offset(&self.name)?;
        writer.write_string_offset(&self.description)?;
        writer.write_u16::<BE>(self.price)?;
        writer.write_u16::<BE>(0)?; // Padding
        Ok(())
    }
}

/// Metadata which describes a suit item.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Suit {
    /// The suit's display name.
    pub name: CString,
}

impl Suit {
    /// Constructs an empty `Suit`.
    pub fn new() -> Self {
        Self::default()
    }

    fn read_strings(&mut self, reader: &mut StringReader<impl Read + Seek>) -> io::Result<()> {
        self.name = reader.read_next_string()?;
        Ok(())
    }
}

impl<R: Read> ReadFrom<StringReader<R>> for Suit {
    type Error = Error;
    fn read_from(reader: &mut StringReader<R>) -> Result<Self> {
        Ok(Self { name: reader.read_string_offset()? })
    }
}

impl<W: Write + Seek> WriteTo<StringWriter<W>> for Suit {
    type Error = Error;
    fn write_to(&self, writer: &mut StringWriter<W>) -> Result<()> {
        writer.write_string_offset(&self.name)?;
        Ok(())
    }
}

/// Metadata which describes a stage.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Stage {
    /// The stage's display name (shown on the loading screen).
    pub name: CString,
    /// The stage's description (shown on the level select menu).
    pub description: CString,
}

impl Stage {
    /// Constructs an empty `Stage`.
    pub fn new() -> Self {
        Self::default()
    }

    fn read_strings(&mut self, reader: &mut StringReader<impl Read + Seek>) -> io::Result<()> {
        self.name = reader.read_next_string()?;
        self.description = reader.read_next_string()?;
        Ok(())
    }
}

impl<R: Read> ReadFrom<StringReader<R>> for Stage {
    type Error = Error;
    fn read_from(reader: &mut StringReader<R>) -> Result<Self> {
        Ok(Self { name: reader.read_string_offset()?, description: reader.read_string_offset()? })
    }
}

impl<W: Write + Seek> WriteTo<StringWriter<W>> for Stage {
    type Error = Error;
    fn write_to(&self, writer: &mut StringWriter<W>) -> Result<()> {
        writer.write_string_offset(&self.name)?;
        writer.write_string_offset(&self.description)?;
        Ok(())
    }
}

/// Metadata which describes a utilibot.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Leticker {
    /// The utilibot's display name.
    pub name: CString,
    /// The utilibot's description (shown in the shop).
    pub description: CString,
    /// The utilibot's shop price.
    pub price: u16,
}

impl Leticker {
    /// Constructs an empty `Leticker`.
    pub fn new() -> Self {
        Self::default()
    }

    fn read_strings(&mut self, reader: &mut StringReader<impl Read + Seek>) -> io::Result<()> {
        self.name = reader.read_next_string()?;
        self.description = reader.read_next_string()?;
        Ok(())
    }
}

impl<R: Read> ReadFrom<StringReader<R>> for Leticker {
    type Error = Error;
    fn read_from(reader: &mut StringReader<R>) -> Result<Self> {
        let leticker = Self {
            name: reader.read_string_offset()?,
            description: reader.read_string_offset()?,
            price: reader.read_u16::<BE>()?,
        };
        reader.read_u16::<BE>()?; // Padding
        Ok(leticker)
    }
}

impl<W: Write + Seek> WriteTo<StringWriter<W>> for Leticker {
    type Error = Error;
    fn write_to(&self, writer: &mut StringWriter<W>) -> Result<()> {
        writer.write_string_offset(&self.name)?;
        writer.write_string_offset(&self.description)?;
        writer.write_u16::<BE>(self.price)?;
        writer.write_u16::<BE>(0)?; // Padding
        Ok(())
    }
}

/// Metadata which describes a sticker.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Sticker {
    /// The sticker's display name.
    pub name: CString,
    /// The sticker's description (shown in the sticker menu).
    pub description: CString,
    /// The index of the flag which determines whether the sticker is unlocked.
    pub flag_index: u32,
}

impl Sticker {
    /// Constructs an empty `Sticker`.
    pub fn new() -> Self {
        Self::default()
    }

    fn read_strings(&mut self, reader: &mut StringReader<impl Read + Seek>) -> io::Result<()> {
        self.name = reader.read_next_string()?;
        self.description = reader.read_next_string()?;
        Ok(())
    }
}

impl<R: Read> ReadFrom<StringReader<R>> for Sticker {
    type Error = Error;
    fn read_from(reader: &mut StringReader<R>) -> Result<Self> {
        Ok(Self {
            name: reader.read_string_offset()?,
            description: reader.read_string_offset()?,
            flag_index: reader.read_u32::<BE>()?,
        })
    }
}

impl<W: Write + Seek> WriteTo<StringWriter<W>> for Sticker {
    type Error = Error;
    fn write_to(&self, writer: &mut StringWriter<W>) -> Result<()> {
        writer.write_string_offset(&self.name)?;
        writer.write_string_offset(&self.description)?;
        writer.write_u32::<BE>(self.flag_index)?;
        Ok(())
    }
}

/// Metadata which describes a stat on the stats menu.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Stat {
    /// The stat's display name.
    pub name: CString,
    /// The stat's description (unused?).
    pub description: CString,
}

impl Stat {
    /// Constructs an empty `Stat`.
    pub fn new() -> Self {
        Self::default()
    }

    fn read_strings(&mut self, reader: &mut StringReader<impl Read + Seek>) -> io::Result<()> {
        self.name = reader.read_next_string()?;
        self.description = reader.read_next_string()?;
        Ok(())
    }
}

impl<R: Read> ReadFrom<StringReader<R>> for Stat {
    type Error = Error;
    fn read_from(reader: &mut StringReader<R>) -> Result<Self> {
        Ok(Self { name: reader.read_string_offset()?, description: reader.read_string_offset()? })
    }
}

impl<W: Write + Seek> WriteTo<StringWriter<W>> for Stat {
    type Error = Error;
    fn write_to(&self, writer: &mut StringWriter<W>) -> Result<()> {
        writer.write_string_offset(&self.name)?;
        writer.write_string_offset(&self.description)?;
        Ok(())
    }
}

/// The metadata stored in globals.bin.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Metadata {
    pub unk_00: [u32; 39],
    pub unk_04: [u32; 4],
    pub unk_08: [u32; 3],
    pub unk_0c: [u32; 2],
    pub unk_10: [u32; 3],
    pub unk_14: [u32; 4],
    pub unk_18: [u32; 3],
    pub unk_1c: [u32; 9],
    pub pickup_sounds: [u32; NUM_PICKUP_SOUNDS],
    pub collect_sounds: [u32; NUM_COLLECT_SOUNDS],
    pub items: Box<[Item]>,
    pub actors: Box<[Actor]>,
    pub atcs: Box<[Atc]>,
    pub suits: Box<[Suit]>,
    pub stages: Box<[Stage]>,
    pub letickers: Box<[Leticker]>,
    pub stickers: Box<[Sticker]>,
    pub stats: Box<[Stat]>,
}

impl Metadata {
    pub fn new() -> Self {
        Self {
            unk_00: [0; 39],
            unk_04: [0; 4],
            unk_08: [0; 3],
            unk_0c: [0; 2],
            unk_10: [0; 3],
            unk_14: [0; 4],
            unk_18: [0; 3],
            unk_1c: [0; 9],
            pickup_sounds: [0; NUM_PICKUP_SOUNDS],
            collect_sounds: [0; NUM_COLLECT_SOUNDS],
            items: vec![Item::new(); NUM_ITEMS].into_boxed_slice(),
            actors: vec![Actor::new(); NUM_ACTORS].into_boxed_slice(),
            atcs: vec![Atc::new(); NUM_ATCS].into_boxed_slice(),
            suits: vec![Suit::new(); NUM_SUITS].into_boxed_slice(),
            stages: vec![Stage::new(); NUM_REAL_STAGES].into_boxed_slice(),
            letickers: vec![Leticker::new(); NUM_LETICKERS].into_boxed_slice(),
            stickers: vec![Sticker::new(); NUM_STICKERS].into_boxed_slice(),
            stats: vec![Stat::new(); NUM_STATS].into_boxed_slice(),
        }
    }
}

impl Default for Metadata {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: Read + Seek> ReadFrom<R> for Metadata {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        assert_eq!(reader.seek(SeekFrom::Current(0))?, 0);
        let header = MetadataHeader::read_from(reader)?;
        let mut metadata = Self::new();

        reader.seek(SeekFrom::Start(header.unk_00_offset as u64))?;
        reader.read_u32_into::<LE>(&mut metadata.unk_00)?;
        reader.seek(SeekFrom::Start(header.unk_04_offset as u64))?;
        reader.read_u32_into::<LE>(&mut metadata.unk_04)?;
        reader.seek(SeekFrom::Start(header.unk_08_offset as u64))?;
        reader.read_u32_into::<LE>(&mut metadata.unk_08)?;
        reader.seek(SeekFrom::Start(header.unk_0c_offset as u64))?;
        reader.read_u32_into::<LE>(&mut metadata.unk_0c)?;
        reader.seek(SeekFrom::Start(header.unk_10_offset as u64))?;
        reader.read_u32_into::<LE>(&mut metadata.unk_10)?;
        reader.seek(SeekFrom::Start(header.unk_14_offset as u64))?;
        reader.read_u32_into::<LE>(&mut metadata.unk_14)?;
        reader.seek(SeekFrom::Start(header.unk_18_offset as u64))?;
        reader.read_u32_into::<LE>(&mut metadata.unk_18)?;
        reader.seek(SeekFrom::Start(header.unk_1c_offset as u64))?;
        reader.read_u32_into::<LE>(&mut metadata.unk_1c)?;
        reader.seek(SeekFrom::Start(header.pickup_sounds_offset as u64))?;
        reader.read_u32_into::<LE>(&mut metadata.pickup_sounds)?;
        reader.seek(SeekFrom::Start(header.collect_sounds_offset as u64))?;
        reader.read_u32_into::<LE>(&mut metadata.collect_sounds)?;

        let mut reader = StringReader::new(reader);
        reader.seek(SeekFrom::Start(header.items_offset as u64))?;
        Item::read_all_from(&mut reader, &mut metadata.items)?;
        reader.seek(SeekFrom::Start(header.actors_offset as u64))?;
        Actor::read_all_from(&mut reader, &mut metadata.actors)?;
        reader.seek(SeekFrom::Start(header.atcs_offset as u64))?;
        Atc::read_all_from(&mut reader, &mut metadata.atcs)?;
        reader.seek(SeekFrom::Start(header.suits_offset as u64))?;
        Suit::read_all_from(&mut reader, &mut metadata.suits)?;
        reader.seek(SeekFrom::Start(header.stages_offset as u64))?;
        Stage::read_all_from(&mut reader, &mut metadata.stages)?;
        reader.seek(SeekFrom::Start(header.letickers_offset as u64))?;
        Leticker::read_all_from(&mut reader, &mut metadata.letickers)?;
        reader.seek(SeekFrom::Start(header.stickers_offset as u64))?;
        Sticker::read_all_from(&mut reader, &mut metadata.stickers)?;
        reader.seek(SeekFrom::Start(header.stats_offset as u64))?;
        Stat::read_all_from(&mut reader, &mut metadata.stats)?;

        // Fill in strings after everything has been read
        for item in &mut *metadata.items {
            item.read_strings(&mut reader)?;
        }
        for actor in &mut *metadata.actors {
            actor.read_strings(&mut reader)?;
        }
        for atc in &mut *metadata.atcs {
            atc.read_strings(&mut reader)?;
        }
        for suit in &mut *metadata.suits {
            suit.read_strings(&mut reader)?;
        }
        for stage in &mut *metadata.stages {
            stage.read_strings(&mut reader)?;
        }
        for leticker in &mut *metadata.letickers {
            leticker.read_strings(&mut reader)?;
        }
        for sticker in &mut *metadata.stickers {
            sticker.read_strings(&mut reader)?;
        }
        for stat in &mut *metadata.stats {
            stat.read_strings(&mut reader)?;
        }
        Ok(metadata)
    }
}

fn write_u32_slice<E: ByteOrder, W: Write>(writer: &mut W, nums: &[u32]) -> io::Result<()> {
    for &num in nums {
        writer.write_u32::<E>(num)?;
    }
    Ok(())
}

impl<W: Write + Seek> WriteTo<W> for Metadata {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        assert_eq!(self.items.len(), NUM_ITEMS);
        assert_eq!(self.actors.len(), NUM_ACTORS);
        assert_eq!(self.atcs.len(), NUM_ATCS);
        assert_eq!(self.suits.len(), NUM_SUITS);
        assert_eq!(self.stages.len(), NUM_REAL_STAGES);
        assert_eq!(self.letickers.len(), NUM_LETICKERS);
        assert_eq!(self.stickers.len(), NUM_STICKERS);
        assert_eq!(self.stats.len(), NUM_STATS);

        assert_eq!(writer.seek(SeekFrom::Current(0))?, 0);
        let mut header = MetadataHeader::default();
        header.write_to(writer)?;

        header.unk_00_offset = writer.seek(SeekFrom::Current(0))? as u32;
        write_u32_slice::<LE, W>(writer, &self.unk_00)?;
        header.unk_04_offset = writer.seek(SeekFrom::Current(0))? as u32;
        write_u32_slice::<LE, W>(writer, &self.unk_04)?;
        header.unk_08_offset = writer.seek(SeekFrom::Current(0))? as u32;
        write_u32_slice::<LE, W>(writer, &self.unk_08)?;
        header.unk_0c_offset = writer.seek(SeekFrom::Current(0))? as u32;
        write_u32_slice::<LE, W>(writer, &self.unk_0c)?;
        header.unk_10_offset = writer.seek(SeekFrom::Current(0))? as u32;
        write_u32_slice::<LE, W>(writer, &self.unk_10)?;
        header.unk_14_offset = writer.seek(SeekFrom::Current(0))? as u32;
        write_u32_slice::<LE, W>(writer, &self.unk_14)?;
        header.unk_18_offset = writer.seek(SeekFrom::Current(0))? as u32;
        write_u32_slice::<LE, W>(writer, &self.unk_18)?;
        header.unk_1c_offset = writer.seek(SeekFrom::Current(0))? as u32;
        write_u32_slice::<LE, W>(writer, &self.unk_1c)?;
        header.pickup_sounds_offset = writer.seek(SeekFrom::Current(0))? as u32;
        write_u32_slice::<LE, W>(writer, &self.pickup_sounds)?;
        header.collect_sounds_offset = writer.seek(SeekFrom::Current(0))? as u32;
        write_u32_slice::<LE, W>(writer, &self.collect_sounds)?;

        let mut writer = StringWriter::new(writer);
        header.items_offset = writer.seek(SeekFrom::Current(0))? as u32;
        Item::write_all_to(&mut writer, &self.items)?;
        header.actors_offset = writer.seek(SeekFrom::Current(0))? as u32;
        Actor::write_all_to(&mut writer, &self.actors)?;
        header.atcs_offset = writer.seek(SeekFrom::Current(0))? as u32;
        Atc::write_all_to(&mut writer, &self.atcs)?;
        header.suits_offset = writer.seek(SeekFrom::Current(0))? as u32;
        Suit::write_all_to(&mut writer, &self.suits)?;
        header.stages_offset = writer.seek(SeekFrom::Current(0))? as u32;
        Stage::write_all_to(&mut writer, &self.stages)?;
        header.letickers_offset = writer.seek(SeekFrom::Current(0))? as u32;
        Leticker::write_all_to(&mut writer, &self.letickers)?;
        header.stickers_offset = writer.seek(SeekFrom::Current(0))? as u32;
        Sticker::write_all_to(&mut writer, &self.stickers)?;
        header.stats_offset = writer.seek(SeekFrom::Current(0))? as u32;
        Stat::write_all_to(&mut writer, &self.stats)?;

        // Write all the strings out at the end. This is different from how the official file is
        // structured, where each list is followed by its strings and names come before
        // descriptions. But this is simpler and still produces valid data.
        writer.write_strings()?;

        let end_offset = writer.seek(SeekFrom::Current(0))?;
        writer.seek(SeekFrom::Start(0))?;
        header.write_to(&mut writer)?;
        writer.seek(SeekFrom::Start(end_offset))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_write_and_read;
    use std::io::Cursor;

    #[test]
    fn test_write_and_read_metadata_header() {
        assert_write_and_read!(MetadataHeader {
            unk_00_offset: 1,
            unk_04_offset: 2,
            unk_08_offset: 3,
            unk_0c_offset: 4,
            unk_10_offset: 5,
            unk_14_offset: 6,
            unk_18_offset: 7,
            unk_1c_offset: 8,
            pickup_sounds_offset: 9,
            collect_sounds_offset: 10,
            items_offset: 11,
            actors_offset: 12,
            atcs_offset: 13,
            suits_offset: 14,
            stages_offset: 15,
            unused_3c: 16,
            unused_40: 17,
            unused_44: 18,
            letickers_offset: 19,
            stickers_offset: 20,
            stats_offset: 21,
        });
    }

    macro_rules! assert_write_and_read_with_strings {
        ($type:ty, $obj:expr) => {
            let mut writer = StringWriter::new(Cursor::new(vec![]));
            let before = $obj;
            before.write_to(&mut writer).unwrap();
            writer.write_strings().unwrap();

            let mut reader = StringReader::new(writer.inner);
            reader.seek(SeekFrom::Start(0)).unwrap();
            let mut after: $type = ReadFrom::read_from(&mut reader).unwrap();
            after.read_strings(&mut reader).unwrap();

            assert_eq!(before, after);
        };
    }

    #[test]
    fn test_write_and_read_item() {
        assert_write_and_read_with_strings!(
            Item,
            Item {
                name: CString::new("name").unwrap(),
                description: CString::new("description").unwrap(),
                flags: ItemFlags::JUNK | ItemFlags::CHIBI_VISION | ItemFlags::INVENTORY,
                pickup_delay: 2,
                price: 3,
                junk_exp: 4,
                junk_money: 5,
                pickup_sound: -6,
                collect_sound: -7,
            }
        );
    }

    #[test]
    fn test_write_and_read_actor() {
        assert_write_and_read_with_strings!(Actor, Actor { name: CString::new("name").unwrap() });
    }

    #[test]
    fn test_write_and_read_atc() {
        assert_write_and_read_with_strings!(
            Atc,
            Atc {
                name: CString::new("name").unwrap(),
                description: CString::new("description").unwrap(),
                price: 1,
            }
        );
    }

    #[test]
    fn test_write_and_read_suit() {
        assert_write_and_read_with_strings!(Suit, Suit { name: CString::new("name").unwrap() });
    }

    #[test]
    fn test_write_and_read_stage() {
        assert_write_and_read_with_strings!(
            Stage,
            Stage {
                name: CString::new("name").unwrap(),
                description: CString::new("description").unwrap(),
            }
        );
    }

    #[test]
    fn test_write_and_read_leticker() {
        assert_write_and_read_with_strings!(
            Leticker,
            Leticker {
                name: CString::new("name").unwrap(),
                description: CString::new("description").unwrap(),
                price: 1,
            }
        );
    }

    #[test]
    fn test_write_and_read_sticker() {
        assert_write_and_read_with_strings!(
            Sticker,
            Sticker {
                name: CString::new("name").unwrap(),
                description: CString::new("description").unwrap(),
                flag_index: 1,
            }
        );
    }

    #[test]
    fn test_write_and_read_stat() {
        assert_write_and_read_with_strings!(
            Stat,
            Stat {
                name: CString::new("name").unwrap(),
                description: CString::new("description").unwrap(),
            }
        );
    }

    #[test]
    fn test_write_and_read_metadata() {
        assert_write_and_read!(Metadata::new());
    }
}
