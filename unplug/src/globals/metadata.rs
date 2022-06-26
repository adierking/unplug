use super::{Error, Result};
use crate::common::{ReadFrom, Text, WriteTo};
use crate::data::Sound;
use bitflags::bitflags;
use byteorder::{ReadBytesExt, WriteBytesExt, BE, LE};
use std::convert::TryInto;
use std::ffi::CString;
use std::io::{self, Read, Seek, SeekFrom, Write};

const NUM_PICKUP_SOUNDS: usize = 4;
const NUM_COLLECT_SOUNDS: usize = 3;
const NUM_ITEMS: usize = 159;
const NUM_ACTORS: usize = 64;
const NUM_ATCS: usize = 9;
const NUM_SUITS: usize = 9;
const NUM_STAGES: usize = 30;
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
    /// Returns an empty `Text` for convenience purposes.
    fn read_string_offset(&mut self) -> io::Result<Text> {
        let offset = self.inner.read_u32::<LE>()?;
        self.offsets.push(offset);
        Ok(Text::new())
    }
}

impl<R: Read + Seek> StringReader<R> {
    /// Reads the next unread string (in order the offsets were read).
    fn read_next_string(&mut self) -> io::Result<Text> {
        assert!(self.index < self.offsets.len(), "no more unread strings");
        let offset = self.offsets[self.index];
        self.index += 1;
        self.inner.seek(SeekFrom::Start(offset as u64))?;
        Ok(CString::read_from(&mut self.inner)?.into())
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
    strings: Vec<(u64, Text)>,
}

impl<W: Write + Seek> StringWriter<W> {
    /// Constructs a new `StringWriter<W>` which wraps `inner`.
    fn new(inner: W) -> Self {
        Self { inner, strings: Vec::with_capacity(NUM_STRINGS) }
    }

    /// Writes a placeholder string offset.
    fn write_string_offset(&mut self, string: &Text) -> io::Result<()> {
        let offset = self.inner.seek(SeekFrom::Current(0))?;
        self.strings.push((offset, string.clone()));
        self.inner.write_u32::<LE>(0)?;
        Ok(())
    }

    /// Writes out the actual strings and fills in the placeholder offsets.
    fn write_strings(&mut self) -> io::Result<()> {
        let mut cur_offset = self.inner.seek(SeekFrom::Current(0))?;
        for (ptr_offset, string) in &self.strings {
            let str_offset = cur_offset;
            self.inner.write_all(string.as_bytes())?;
            self.inner.write_u8(0)?;
            cur_offset += string.as_bytes().len() as u64 + 1;
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
    battery_globals_offset: u32,
    popper_globals_offset: u32,
    copter_globals_offset: u32,
    radar_globals_offset: u32,
    time_limit_offset: u32,
    player_globals_offset: u32,
    default_atcs_offset: u32,
    coin_values_offset: u32,
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

impl<R: Read + ?Sized> ReadFrom<R> for MetadataHeader {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self {
            battery_globals_offset: reader.read_u32::<LE>()?,
            popper_globals_offset: reader.read_u32::<LE>()?,
            copter_globals_offset: reader.read_u32::<LE>()?,
            radar_globals_offset: reader.read_u32::<LE>()?,
            time_limit_offset: reader.read_u32::<LE>()?,
            player_globals_offset: reader.read_u32::<LE>()?,
            default_atcs_offset: reader.read_u32::<LE>()?,
            coin_values_offset: reader.read_u32::<LE>()?,
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

impl<W: Write + ?Sized> WriteTo<W> for MetadataHeader {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_u32::<LE>(self.battery_globals_offset)?;
        writer.write_u32::<LE>(self.popper_globals_offset)?;
        writer.write_u32::<LE>(self.copter_globals_offset)?;
        writer.write_u32::<LE>(self.radar_globals_offset)?;
        writer.write_u32::<LE>(self.time_limit_offset)?;
        writer.write_u32::<LE>(self.player_globals_offset)?;
        writer.write_u32::<LE>(self.default_atcs_offset)?;
        writer.write_u32::<LE>(self.coin_values_offset)?;
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

/// Per-action battery drain values in hundredths of watts per second.
/// This is internally an array, but a struct is more convenient.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BatteryGlobals {
    pub idle: i32,
    pub idle_anim: i32,
    pub walk: i32,
    pub jog: i32,
    pub run: i32,
    pub slide: i32,
    pub equip: i32,
    pub lift: i32,
    pub drop: i32,
    pub leticker: i32,
    pub ledge_grab: i32,
    pub ledge_slide: i32,
    pub ledge_climb: i32,
    pub ledge_drop: i32,
    pub ledge_teeter: i32,
    pub jump: i32,
    pub fall: i32,
    pub ladder_grab: i32,
    pub ladder_ascend: i32,
    pub ladder_descend: i32,
    pub ladder_top: i32,
    pub ladder_bottom: i32,
    pub rope_grab: i32,
    pub rope_ascend: i32,
    pub rope_descend: i32,
    pub rope_top: i32,
    pub rope_bottom: i32,
    pub push: i32,
    pub copter_hover: i32,
    pub copter_descend: i32,
    pub popper_shoot: i32,
    pub popper_shoot_charged: i32,
    pub radar_scan: i32,
    pub radar_follow: i32,
    pub brush: i32,
    pub spoon: i32,
    pub mug: i32,
    pub squirter_suck: i32,
    pub squirter_spray: i32,
}

impl BatteryGlobals {
    /// Constructs an empty `BatteryGlobals`.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<R: Read + ?Sized> ReadFrom<R> for BatteryGlobals {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self {
            idle: reader.read_i32::<LE>()?,
            idle_anim: reader.read_i32::<LE>()?,
            walk: reader.read_i32::<LE>()?,
            jog: reader.read_i32::<LE>()?,
            run: reader.read_i32::<LE>()?,
            slide: reader.read_i32::<LE>()?,
            equip: reader.read_i32::<LE>()?,
            lift: reader.read_i32::<LE>()?,
            drop: reader.read_i32::<LE>()?,
            leticker: reader.read_i32::<LE>()?,
            ledge_grab: reader.read_i32::<LE>()?,
            ledge_slide: reader.read_i32::<LE>()?,
            ledge_climb: reader.read_i32::<LE>()?,
            ledge_drop: reader.read_i32::<LE>()?,
            ledge_teeter: reader.read_i32::<LE>()?,
            jump: reader.read_i32::<LE>()?,
            fall: reader.read_i32::<LE>()?,
            ladder_grab: reader.read_i32::<LE>()?,
            ladder_ascend: reader.read_i32::<LE>()?,
            ladder_descend: reader.read_i32::<LE>()?,
            ladder_top: reader.read_i32::<LE>()?,
            ladder_bottom: reader.read_i32::<LE>()?,
            rope_grab: reader.read_i32::<LE>()?,
            rope_ascend: reader.read_i32::<LE>()?,
            rope_descend: reader.read_i32::<LE>()?,
            rope_top: reader.read_i32::<LE>()?,
            rope_bottom: reader.read_i32::<LE>()?,
            push: reader.read_i32::<LE>()?,
            copter_hover: reader.read_i32::<LE>()?,
            copter_descend: reader.read_i32::<LE>()?,
            popper_shoot: reader.read_i32::<LE>()?,
            popper_shoot_charged: reader.read_i32::<LE>()?,
            radar_scan: reader.read_i32::<LE>()?,
            radar_follow: reader.read_i32::<LE>()?,
            brush: reader.read_i32::<LE>()?,
            spoon: reader.read_i32::<LE>()?,
            mug: reader.read_i32::<LE>()?,
            squirter_suck: reader.read_i32::<LE>()?,
            squirter_spray: reader.read_i32::<LE>()?,
        })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for BatteryGlobals {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_i32::<LE>(self.idle)?;
        writer.write_i32::<LE>(self.idle_anim)?;
        writer.write_i32::<LE>(self.walk)?;
        writer.write_i32::<LE>(self.jog)?;
        writer.write_i32::<LE>(self.run)?;
        writer.write_i32::<LE>(self.slide)?;
        writer.write_i32::<LE>(self.equip)?;
        writer.write_i32::<LE>(self.lift)?;
        writer.write_i32::<LE>(self.drop)?;
        writer.write_i32::<LE>(self.leticker)?;
        writer.write_i32::<LE>(self.ledge_grab)?;
        writer.write_i32::<LE>(self.ledge_slide)?;
        writer.write_i32::<LE>(self.ledge_climb)?;
        writer.write_i32::<LE>(self.ledge_drop)?;
        writer.write_i32::<LE>(self.ledge_teeter)?;
        writer.write_i32::<LE>(self.jump)?;
        writer.write_i32::<LE>(self.fall)?;
        writer.write_i32::<LE>(self.ladder_grab)?;
        writer.write_i32::<LE>(self.ladder_ascend)?;
        writer.write_i32::<LE>(self.ladder_descend)?;
        writer.write_i32::<LE>(self.ladder_top)?;
        writer.write_i32::<LE>(self.ladder_bottom)?;
        writer.write_i32::<LE>(self.rope_grab)?;
        writer.write_i32::<LE>(self.rope_ascend)?;
        writer.write_i32::<LE>(self.rope_descend)?;
        writer.write_i32::<LE>(self.rope_top)?;
        writer.write_i32::<LE>(self.rope_bottom)?;
        writer.write_i32::<LE>(self.push)?;
        writer.write_i32::<LE>(self.copter_hover)?;
        writer.write_i32::<LE>(self.copter_descend)?;
        writer.write_i32::<LE>(self.popper_shoot)?;
        writer.write_i32::<LE>(self.popper_shoot_charged)?;
        writer.write_i32::<LE>(self.radar_scan)?;
        writer.write_i32::<LE>(self.radar_follow)?;
        writer.write_i32::<LE>(self.brush)?;
        writer.write_i32::<LE>(self.spoon)?;
        writer.write_i32::<LE>(self.mug)?;
        writer.write_i32::<LE>(self.squirter_suck)?;
        writer.write_i32::<LE>(self.squirter_spray)?;
        Ok(())
    }
}

/// Values which control the behavior of the popper (blaster) attachment.
/// This is internally an array, but a struct is more convenient.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PopperGlobals {
    /// The projectile range without the Range Chip in tenths of units.
    pub range_default: i32,
    /// The projectile range with the Range Chip in tenths of units.
    pub range_upgraded: i32,
    /// The speed of each projectile in tenths of units.
    pub projectile_speed: i32,
    /// The maximum number of projectiles that can exist at a time. Hard-capped at 10.
    pub max_projectiles: i32,
}

impl PopperGlobals {
    /// Constructs an empty `PopperGlobals`.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<R: Read + ?Sized> ReadFrom<R> for PopperGlobals {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self {
            range_default: reader.read_i32::<LE>()?,
            range_upgraded: reader.read_i32::<LE>()?,
            projectile_speed: reader.read_i32::<LE>()?,
            max_projectiles: reader.read_i32::<LE>()?,
        })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for PopperGlobals {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_i32::<LE>(self.range_default)?;
        writer.write_i32::<LE>(self.range_upgraded)?;
        writer.write_i32::<LE>(self.projectile_speed)?;
        writer.write_i32::<LE>(self.max_projectiles)?;
        Ok(())
    }
}

/// Values which control the behavior of the copter attachment.
/// This is internally an array, but a struct is more convenient.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CopterGlobals {
    /// The amount of time the player hovers when activating the copter in hundredths of seconds.
    pub hover_duration: i32,
    /// The player's gravity when falling with the copter in hundredths of units.
    pub gravity: i32,
    /// The player's terminal velocity when falling with the copter in hundredths of units.
    pub terminal_velocity: i32,
}

impl CopterGlobals {
    /// Constructs an empty `CopterGlobals`.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<R: Read + ?Sized> ReadFrom<R> for CopterGlobals {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self {
            hover_duration: reader.read_i32::<LE>()?,
            gravity: reader.read_i32::<LE>()?,
            terminal_velocity: reader.read_i32::<LE>()?,
        })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for CopterGlobals {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_i32::<LE>(self.hover_duration)?;
        writer.write_i32::<LE>(self.gravity)?;
        writer.write_i32::<LE>(self.terminal_velocity)?;
        Ok(())
    }
}

/// Values which control the behavior of the radar attachment.
/// This is internally an array, but a struct is more convenient.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RadarGlobals {
    /// The range where the radar beam is red in hundredths of units.
    pub red_range: i32,
    /// The range where the radar beam is yellow in hundredths of units.
    pub yellow_range: i32,
}

impl RadarGlobals {
    /// Constructs an empty `RadarGlobals`.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<R: Read + ?Sized> ReadFrom<R> for RadarGlobals {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self { red_range: reader.read_i32::<LE>()?, yellow_range: reader.read_i32::<LE>()? })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for RadarGlobals {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_i32::<LE>(self.red_range)?;
        writer.write_i32::<LE>(self.yellow_range)?;
        Ok(())
    }
}

/// This gets turned into a tick count value when a level starts but never seems to be used
/// otherwise. Possibly related to the unused "time up" feature referenced in several places?
///
/// This is internally an array, but a struct is more convenient.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TimeLimit {
    pub hours: i32,
    pub minutes: i32,
    pub seconds: i32,
}

impl TimeLimit {
    /// Constructs an empty `TimeLimit`.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<R: Read + ?Sized> ReadFrom<R> for TimeLimit {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self {
            hours: reader.read_i32::<LE>()?,
            minutes: reader.read_i32::<LE>()?,
            seconds: reader.read_i32::<LE>()?,
        })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for TimeLimit {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_i32::<LE>(self.hours)?;
        writer.write_i32::<LE>(self.minutes)?;
        writer.write_i32::<LE>(self.seconds)?;
        Ok(())
    }
}

/// Values which control the behavior of the player character.
/// This is internally an array, but a struct is more convenient.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PlayerGlobals {
    /// The amount of time it takes to climb onto an object in hundredths of seconds if the analog
    /// stick's magnitude is 1.0.
    pub climb_duration: i32,
    /// If this is greater than 0, it overrides the analog stick magnitude to produce a constant
    /// climb rate. A value of 100 is equivalent to a magnitude of 1.0.
    pub climb_rate: i32,
    /// The percentage (1-100) that the climb meter caps out at if the player is only gently tilting
    /// the analog stick (magnitude <= 0.7). If this is 0, the player can always climb objects
    /// regardless of the analog stick magnitude.
    pub gentle_climb_percent: i32,
    /// The amount of time that the player can hold the A button in hundredths of seconds to
    /// automatically pick up the plug.
    pub auto_plug_pickup_time: i32,
}

impl PlayerGlobals {
    /// Constructs an empty `PlayerGlobals`.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<R: Read + ?Sized> ReadFrom<R> for PlayerGlobals {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self {
            climb_duration: reader.read_i32::<LE>()?,
            climb_rate: reader.read_i32::<LE>()?,
            gentle_climb_percent: reader.read_i32::<LE>()?,
            auto_plug_pickup_time: reader.read_i32::<LE>()?,
        })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for PlayerGlobals {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_i32::<LE>(self.climb_duration)?;
        writer.write_i32::<LE>(self.climb_rate)?;
        writer.write_i32::<LE>(self.gentle_climb_percent)?;
        writer.write_i32::<LE>(self.auto_plug_pickup_time)?;
        Ok(())
    }
}

/// Determines which attachments are unlocked by default in a new game.
/// This is internally an array, but a struct is more convenient.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DefaultAtcs {
    /// `true` if the copter should be unlocked by default.
    pub copter: bool,
    /// `true` if the popper (blaster) should be unlocked by default.
    pub popper: bool,
    /// `true` if the radar should be unlocked by default.
    pub radar: bool,
}

impl DefaultAtcs {
    /// Constructs an empty `DefaultAtcs`.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<R: Read + ?Sized> ReadFrom<R> for DefaultAtcs {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self {
            copter: reader.read_u32::<LE>()? != 0,
            popper: reader.read_u32::<LE>()? != 0,
            radar: reader.read_u32::<LE>()? != 0,
        })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for DefaultAtcs {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_u32::<LE>(self.copter as u32)?;
        writer.write_u32::<LE>(self.popper as u32)?;
        writer.write_u32::<LE>(self.radar as u32)?;
        Ok(())
    }
}

/// Stores the value of each type of coin object.
///
/// This is internally an array, but the indices are calculated based on the object data values
/// hardcoded into the executable, so we don't lose anything by representing this as a struct.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CoinValues {
    pub coin_g: u32,      // 0
    pub coin_s: u32,      // 1
    pub coin_c: u32,      // 2
    pub junk_a: u32,      // 100
    pub junk_b: u32,      // 101
    pub junk_c: u32,      // 102
    pub junk_unko: u32,   // 103
    pub energyb: u32,     // 200
    pub happy_heart: u32, // 300
}

impl CoinValues {
    /// Constructs an empty `CoinValues`.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<R: Read + ?Sized> ReadFrom<R> for CoinValues {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self {
            coin_g: reader.read_u32::<LE>()?,
            coin_s: reader.read_u32::<LE>()?,
            coin_c: reader.read_u32::<LE>()?,
            junk_a: reader.read_u32::<LE>()?,
            junk_b: reader.read_u32::<LE>()?,
            junk_c: reader.read_u32::<LE>()?,
            junk_unko: reader.read_u32::<LE>()?,
            energyb: reader.read_u32::<LE>()?,
            happy_heart: reader.read_u32::<LE>()?,
        })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for CoinValues {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_u32::<LE>(self.coin_g)?;
        writer.write_u32::<LE>(self.coin_s)?;
        writer.write_u32::<LE>(self.coin_c)?;
        writer.write_u32::<LE>(self.junk_a)?;
        writer.write_u32::<LE>(self.junk_b)?;
        writer.write_u32::<LE>(self.junk_c)?;
        writer.write_u32::<LE>(self.junk_unko)?;
        writer.write_u32::<LE>(self.energyb)?;
        writer.write_u32::<LE>(self.happy_heart)?;
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
    pub name: Text,
    /// The item's description (shown in the inventory and the shop).
    pub description: Text,
    /// The item's flags.
    pub flags: ItemFlags,
    /// The amount of time it takes to pick up the item in hundredths of seconds.
    pub pickup_delay: i16,
    /// The item's shop price.
    pub price: i16,
    /// The number of happy points rewarded if the player throws the item away.
    pub junk_exp: i16,
    /// The amount of money rewarded if the player throws the item away.
    pub junk_money: i16,
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
            pickup_delay: reader.read_i16::<BE>()?,
            price: reader.read_i16::<BE>()?,
            junk_exp: reader.read_i16::<BE>()?,
            junk_money: reader.read_i16::<BE>()?,
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
        writer.write_i16::<BE>(self.pickup_delay)?;
        writer.write_i16::<BE>(self.price)?;
        writer.write_i16::<BE>(self.junk_exp)?;
        writer.write_i16::<BE>(self.junk_money)?;
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
    pub name: Text,
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
    pub name: Text,
    /// The attachment's description (unused?).
    pub description: Text,
    /// The attachment's shop price.
    pub price: i16,
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
            price: reader.read_i16::<BE>()?,
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
        writer.write_i16::<BE>(self.price)?;
        writer.write_u16::<BE>(0)?; // Padding
        Ok(())
    }
}

/// Metadata which describes a suit item.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Suit {
    /// The suit's display name.
    pub name: Text,
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
    pub name: Text,
    /// The stage's description (shown on the level select menu).
    pub description: Text,
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
    pub name: Text,
    /// The utilibot's description (shown in the shop).
    pub description: Text,
    /// The utilibot's shop price.
    pub price: i16,
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
            price: reader.read_i16::<BE>()?,
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
        writer.write_i16::<BE>(self.price)?;
        writer.write_u16::<BE>(0)?; // Padding
        Ok(())
    }
}

/// Metadata which describes a sticker.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Sticker {
    /// The sticker's display name.
    pub name: Text,
    /// The sticker's description (shown in the sticker menu).
    pub description: Text,
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
    pub name: Text,
    /// The stat's description (unused?).
    pub description: Text,
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

fn read_sounds(mut reader: impl Read, sounds: &mut [Sound]) -> Result<()> {
    for sound in sounds {
        *sound = reader.read_u32::<LE>()?.try_into()?;
    }
    Ok(())
}

fn write_sounds(mut writer: impl Write, sounds: &[Sound]) -> Result<()> {
    for sound in sounds {
        writer.write_u32::<LE>(sound.value())?;
    }
    Ok(())
}

/// The metadata stored in globals.bin.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Metadata {
    pub battery_globals: BatteryGlobals,
    pub popper_globals: PopperGlobals,
    pub copter_globals: CopterGlobals,
    pub radar_globals: RadarGlobals,
    pub time_limit: TimeLimit,
    pub player_globals: PlayerGlobals,
    pub default_atcs: DefaultAtcs,
    pub coin_values: CoinValues,
    pub pickup_sounds: [Sound; NUM_PICKUP_SOUNDS],
    pub collect_sounds: [Sound; NUM_COLLECT_SOUNDS],
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
            battery_globals: BatteryGlobals::new(),
            popper_globals: PopperGlobals::new(),
            copter_globals: CopterGlobals::new(),
            radar_globals: RadarGlobals::new(),
            time_limit: TimeLimit::new(),
            player_globals: PlayerGlobals::new(),
            default_atcs: DefaultAtcs::new(),
            coin_values: CoinValues::new(),
            pickup_sounds: [Sound::default(); NUM_PICKUP_SOUNDS],
            collect_sounds: [Sound::default(); NUM_COLLECT_SOUNDS],
            items: vec![Item::new(); NUM_ITEMS].into_boxed_slice(),
            actors: vec![Actor::new(); NUM_ACTORS].into_boxed_slice(),
            atcs: vec![Atc::new(); NUM_ATCS].into_boxed_slice(),
            suits: vec![Suit::new(); NUM_SUITS].into_boxed_slice(),
            stages: vec![Stage::new(); NUM_STAGES].into_boxed_slice(),
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

impl<R: Read + Seek + ?Sized> ReadFrom<R> for Metadata {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        assert_eq!(reader.seek(SeekFrom::Current(0))?, 0);
        let header = MetadataHeader::read_from(reader)?;
        let mut metadata = Self::new();

        reader.seek(SeekFrom::Start(header.battery_globals_offset as u64))?;
        metadata.battery_globals = BatteryGlobals::read_from(reader)?;
        reader.seek(SeekFrom::Start(header.popper_globals_offset as u64))?;
        metadata.popper_globals = PopperGlobals::read_from(reader)?;
        reader.seek(SeekFrom::Start(header.copter_globals_offset as u64))?;
        metadata.copter_globals = CopterGlobals::read_from(reader)?;
        reader.seek(SeekFrom::Start(header.radar_globals_offset as u64))?;
        metadata.radar_globals = RadarGlobals::read_from(reader)?;
        reader.seek(SeekFrom::Start(header.time_limit_offset as u64))?;
        metadata.time_limit = TimeLimit::read_from(reader)?;
        reader.seek(SeekFrom::Start(header.player_globals_offset as u64))?;
        metadata.player_globals = PlayerGlobals::read_from(reader)?;
        reader.seek(SeekFrom::Start(header.default_atcs_offset as u64))?;
        metadata.default_atcs = DefaultAtcs::read_from(reader)?;
        reader.seek(SeekFrom::Start(header.coin_values_offset as u64))?;
        metadata.coin_values = CoinValues::read_from(reader)?;
        reader.seek(SeekFrom::Start(header.pickup_sounds_offset as u64))?;
        read_sounds(&mut *reader, &mut metadata.pickup_sounds)?;
        reader.seek(SeekFrom::Start(header.collect_sounds_offset as u64))?;
        read_sounds(&mut *reader, &mut metadata.collect_sounds)?;

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

impl<W: Write + Seek + ?Sized> WriteTo<W> for Metadata {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        assert_eq!(self.items.len(), NUM_ITEMS);
        assert_eq!(self.actors.len(), NUM_ACTORS);
        assert_eq!(self.atcs.len(), NUM_ATCS);
        assert_eq!(self.suits.len(), NUM_SUITS);
        assert_eq!(self.stages.len(), NUM_STAGES);
        assert_eq!(self.letickers.len(), NUM_LETICKERS);
        assert_eq!(self.stickers.len(), NUM_STICKERS);
        assert_eq!(self.stats.len(), NUM_STATS);

        assert_eq!(writer.seek(SeekFrom::Current(0))?, 0);
        let mut header = MetadataHeader::default();
        header.write_to(writer)?;

        header.battery_globals_offset = writer.seek(SeekFrom::Current(0))? as u32;
        self.battery_globals.write_to(writer)?;
        header.popper_globals_offset = writer.seek(SeekFrom::Current(0))? as u32;
        self.popper_globals.write_to(writer)?;
        header.copter_globals_offset = writer.seek(SeekFrom::Current(0))? as u32;
        self.copter_globals.write_to(writer)?;
        header.radar_globals_offset = writer.seek(SeekFrom::Current(0))? as u32;
        self.radar_globals.write_to(writer)?;
        header.time_limit_offset = writer.seek(SeekFrom::Current(0))? as u32;
        self.time_limit.write_to(writer)?;
        header.player_globals_offset = writer.seek(SeekFrom::Current(0))? as u32;
        self.player_globals.write_to(writer)?;
        header.default_atcs_offset = writer.seek(SeekFrom::Current(0))? as u32;
        self.default_atcs.write_to(writer)?;
        header.coin_values_offset = writer.seek(SeekFrom::Current(0))? as u32;
        self.coin_values.write_to(writer)?;
        header.pickup_sounds_offset = writer.seek(SeekFrom::Current(0))? as u32;
        write_sounds(&mut *writer, &self.pickup_sounds)?;
        header.collect_sounds_offset = writer.seek(SeekFrom::Current(0))? as u32;
        write_sounds(&mut *writer, &self.collect_sounds)?;

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
            battery_globals_offset: 1,
            popper_globals_offset: 2,
            copter_globals_offset: 3,
            radar_globals_offset: 4,
            time_limit_offset: 5,
            player_globals_offset: 6,
            default_atcs_offset: 7,
            coin_values_offset: 8,
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

    #[test]
    fn test_write_and_read_battery_globals() {
        assert_write_and_read!(BatteryGlobals {
            idle: 1,
            idle_anim: 2,
            walk: 3,
            jog: 4,
            run: 5,
            slide: 6,
            equip: 7,
            lift: 8,
            drop: 9,
            leticker: 10,
            ledge_grab: 11,
            ledge_slide: 12,
            ledge_climb: 13,
            ledge_drop: 14,
            ledge_teeter: 15,
            jump: 16,
            fall: 17,
            ladder_grab: 18,
            ladder_ascend: 19,
            ladder_descend: 20,
            ladder_top: 21,
            ladder_bottom: 22,
            rope_grab: 23,
            rope_ascend: 24,
            rope_descend: 25,
            rope_top: 26,
            rope_bottom: 27,
            push: 28,
            copter_hover: 29,
            copter_descend: 30,
            popper_shoot: 31,
            popper_shoot_charged: 32,
            radar_scan: 33,
            radar_follow: 34,
            brush: 35,
            spoon: 36,
            mug: 37,
            squirter_suck: 38,
            squirter_spray: 39,
        });
    }

    #[test]
    fn test_write_and_read_popper_globals() {
        assert_write_and_read!(PopperGlobals {
            range_default: 1,
            range_upgraded: 2,
            projectile_speed: 3,
            max_projectiles: 4,
        });
    }

    #[test]
    fn test_write_and_read_copter_globals() {
        assert_write_and_read!(CopterGlobals {
            hover_duration: 1,
            gravity: 2,
            terminal_velocity: 3,
        });
    }

    #[test]
    fn test_write_and_read_radar_globals() {
        assert_write_and_read!(RadarGlobals { red_range: 1, yellow_range: 2 });
    }

    #[test]
    fn test_write_and_read_player_globals() {
        assert_write_and_read!(PlayerGlobals {
            climb_duration: 1,
            climb_rate: 2,
            gentle_climb_percent: 3,
            auto_plug_pickup_time: 4,
        });
    }

    #[test]
    fn test_write_and_read_default_atcs() {
        assert_write_and_read!(DefaultAtcs { copter: true, popper: false, radar: true });
    }

    #[test]
    fn test_write_and_read_time_limit() {
        assert_write_and_read!(TimeLimit { hours: 1, minutes: 2, seconds: 3 });
    }

    #[test]
    fn test_write_and_read_coin_values() {
        assert_write_and_read!(CoinValues {
            coin_g: 1,
            coin_s: 2,
            coin_c: 3,
            junk_a: 4,
            junk_b: 5,
            junk_c: 6,
            junk_unko: 7,
            energyb: 8,
            happy_heart: 9,
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
                name: Text::encode("name").unwrap(),
                description: Text::encode("description").unwrap(),
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
        assert_write_and_read_with_strings!(Actor, Actor { name: Text::encode("name").unwrap() });
    }

    #[test]
    fn test_write_and_read_atc() {
        assert_write_and_read_with_strings!(
            Atc,
            Atc {
                name: Text::encode("name").unwrap(),
                description: Text::encode("description").unwrap(),
                price: 1,
            }
        );
    }

    #[test]
    fn test_write_and_read_suit() {
        assert_write_and_read_with_strings!(Suit, Suit { name: Text::encode("name").unwrap() });
    }

    #[test]
    fn test_write_and_read_stage() {
        assert_write_and_read_with_strings!(
            Stage,
            Stage {
                name: Text::encode("name").unwrap(),
                description: Text::encode("description").unwrap(),
            }
        );
    }

    #[test]
    fn test_write_and_read_leticker() {
        assert_write_and_read_with_strings!(
            Leticker,
            Leticker {
                name: Text::encode("name").unwrap(),
                description: Text::encode("description").unwrap(),
                price: 1,
            }
        );
    }

    #[test]
    fn test_write_and_read_sticker() {
        assert_write_and_read_with_strings!(
            Sticker,
            Sticker {
                name: Text::encode("name").unwrap(),
                description: Text::encode("description").unwrap(),
                flag_index: 1,
            }
        );
    }

    #[test]
    fn test_write_and_read_stat() {
        assert_write_and_read_with_strings!(
            Stat,
            Stat {
                name: Text::encode("name").unwrap(),
                description: Text::encode("description").unwrap(),
            }
        );
    }

    #[test]
    fn test_write_and_read_metadata() {
        assert_write_and_read!(Metadata::new());
    }
}
