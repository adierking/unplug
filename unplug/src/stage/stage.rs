use super::{Actor, Error, ObjectPlacement, Result};
use crate::common::{
    NonNoneList, ReadFrom, ReadOptionFrom, ReadSeek, WriteOptionTo, WriteSeek, WriteTo,
};
use crate::event::pointer::BlockId;
use crate::event::script::{Script, ScriptReader, ScriptWriter};
use crate::globals::Libs;
use byteorder::{ByteOrder, ReadBytesExt, WriteBytesExt, BE, LE};
use std::io::{Read, SeekFrom, Write};
use std::iter::FusedIterator;
use std::num::NonZeroU32;

const HEADER_SIZE: u32 = 52;

// These should *always* be at the same offsets. Scripts even hardcode references to them.
const EXPECTED_SETTINGS_OFFSET: u32 = HEADER_SIZE;
const EXPECTED_OBJECTS_OFFSET: u32 = EXPECTED_SETTINGS_OFFSET + SETTINGS_SIZE;

/// The number of global events in a stage file.
const NUM_EVENTS: u32 = 6;

#[derive(Debug, Clone, Default)]
struct Header {
    settings_offset: u32,
    objects_offset: u32,
    events_offset: u32,
    on_prologue: Option<NonZeroU32>,
    on_startup: Option<NonZeroU32>,
    on_dead: Option<NonZeroU32>,
    on_pose: Option<NonZeroU32>,
    on_time_cycle: Option<NonZeroU32>,
    on_time_up: Option<NonZeroU32>,
    actors_offset: u32,
    unk_28_offset: Option<NonZeroU32>,
    unk_2c_offset: Option<NonZeroU32>,
    unk_30_offset: Option<NonZeroU32>,
}

impl<R: Read + ?Sized> ReadFrom<R> for Header {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let settings_offset = reader.read_u32::<LE>()?;
        let objects_offset = reader.read_u32::<LE>()?;
        if settings_offset != EXPECTED_SETTINGS_OFFSET || objects_offset != EXPECTED_OBJECTS_OFFSET
        {
            return Err(Error::InvalidHeader);
        }
        Ok(Self {
            settings_offset,
            objects_offset,
            events_offset: reader.read_u32::<LE>()?,
            on_prologue: NonZeroU32::new(reader.read_u32::<LE>()?),
            on_startup: NonZeroU32::new(reader.read_u32::<LE>()?),
            on_dead: NonZeroU32::new(reader.read_u32::<LE>()?),
            on_pose: NonZeroU32::new(reader.read_u32::<LE>()?),
            on_time_cycle: NonZeroU32::new(reader.read_u32::<LE>()?),
            on_time_up: NonZeroU32::new(reader.read_u32::<LE>()?),
            actors_offset: reader.read_u32::<LE>()?,
            unk_28_offset: NonZeroU32::new(reader.read_u32::<LE>()?),
            unk_2c_offset: NonZeroU32::new(reader.read_u32::<LE>()?),
            unk_30_offset: NonZeroU32::new(reader.read_u32::<LE>()?),
        })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for Header {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_u32::<LE>(self.settings_offset)?;
        writer.write_u32::<LE>(self.objects_offset)?;
        writer.write_u32::<LE>(self.events_offset)?;
        writer.write_u32::<LE>(self.on_prologue.map(|o| o.get()).unwrap_or(0))?;
        writer.write_u32::<LE>(self.on_startup.map(|o| o.get()).unwrap_or(0))?;
        writer.write_u32::<LE>(self.on_dead.map(|o| o.get()).unwrap_or(0))?;
        writer.write_u32::<LE>(self.on_pose.map(|o| o.get()).unwrap_or(0))?;
        writer.write_u32::<LE>(self.on_time_cycle.map(|o| o.get()).unwrap_or(0))?;
        writer.write_u32::<LE>(self.on_time_up.map(|o| o.get()).unwrap_or(0))?;
        writer.write_u32::<LE>(self.actors_offset)?;
        writer.write_u32::<LE>(self.unk_28_offset.map(|o| o.get()).unwrap_or(0))?;
        writer.write_u32::<LE>(self.unk_2c_offset.map(|o| o.get()).unwrap_or(0))?;
        writer.write_u32::<LE>(self.unk_30_offset.map(|o| o.get()).unwrap_or(0))?;
        Ok(())
    }
}

const SETTINGS_SIZE: u32 = 20;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Settings {
    pub unk_00: i32,
    pub unk_04: u8,
    pub unk_05: u8,
    pub unk_06: i16,
    pub unk_08: u8,
    pub unk_09: u8,
    pub item_flags_base: i16,
    pub coin_flags_base: i16,
    pub dust_flags_base: i16,
    pub unk_10: i16,
    pub unk_12: i16,
}

impl<R: Read + ?Sized> ReadFrom<R> for Settings {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self {
            unk_00: reader.read_i32::<BE>()?,
            unk_04: reader.read_u8()?,
            unk_05: reader.read_u8()?,
            unk_06: reader.read_i16::<BE>()?,
            unk_08: reader.read_u8()?,
            unk_09: reader.read_u8()?,
            item_flags_base: reader.read_i16::<BE>()?,
            coin_flags_base: reader.read_i16::<BE>()?,
            dust_flags_base: reader.read_i16::<BE>()?,
            unk_10: reader.read_i16::<BE>()?,
            unk_12: reader.read_i16::<BE>()?,
        })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for Settings {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_i32::<BE>(self.unk_00)?;
        writer.write_u8(self.unk_04)?;
        writer.write_u8(self.unk_05)?;
        writer.write_i16::<BE>(self.unk_06)?;
        writer.write_u8(self.unk_08)?;
        writer.write_u8(self.unk_09)?;
        writer.write_i16::<BE>(self.item_flags_base)?;
        writer.write_i16::<BE>(self.coin_flags_base)?;
        writer.write_i16::<BE>(self.dust_flags_base)?;
        writer.write_i16::<BE>(self.unk_10)?;
        writer.write_i16::<BE>(self.unk_12)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct EventTable {
    entry_points: Vec<u32>,
}

impl EventTable {
    fn read_from<R: Read + ?Sized>(reader: &mut R, count: usize) -> Result<Self> {
        let mut entry_points = vec![0u32; count];
        reader.read_u32_into::<LE>(&mut entry_points)?;
        Ok(Self { entry_points })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for EventTable {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        let mut bytes = vec![0u8; self.entry_points.len() * 4];
        LE::write_u32_into(&self.entry_points, &mut bytes);
        writer.write_all(&bytes)?;
        writer.write_i32::<LE>(-1)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unk28 {
    unk_00: i32,
    unk_04: i32,
    unk_08: i32,
    unk_0c: i32,
    unk_10: i32,
    unk_14: i32,
    unk_18: i32,
    unk_1c: i32,
    unk_20: i32,
    unk_24: i32,
    unk_28: i32,
    unk_2c: i16,
    unk_2e: i16,
    unk_30: i32,
}

impl<R: Read + ?Sized> ReadOptionFrom<R> for Unk28 {
    type Error = Error;
    fn read_option_from(reader: &mut R) -> Result<Option<Self>> {
        let unk_00 = reader.read_i32::<BE>()?;
        if unk_00 == -1 {
            return Ok(None);
        }
        Ok(Some(Self {
            unk_00,
            unk_04: reader.read_i32::<BE>()?,
            unk_08: reader.read_i32::<BE>()?,
            unk_0c: reader.read_i32::<BE>()?,
            unk_10: reader.read_i32::<BE>()?,
            unk_14: reader.read_i32::<BE>()?,
            unk_18: reader.read_i32::<BE>()?,
            unk_1c: reader.read_i32::<BE>()?,
            unk_20: reader.read_i32::<BE>()?,
            unk_24: reader.read_i32::<BE>()?,
            unk_28: reader.read_i32::<BE>()?,
            unk_2c: reader.read_i16::<BE>()?,
            unk_2e: reader.read_i16::<BE>()?,
            unk_30: reader.read_i32::<BE>()?,
        }))
    }
}

impl<W: Write + ?Sized> WriteTo<W> for Unk28 {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_i32::<BE>(self.unk_00)?;
        writer.write_i32::<BE>(self.unk_04)?;
        writer.write_i32::<BE>(self.unk_08)?;
        writer.write_i32::<BE>(self.unk_0c)?;
        writer.write_i32::<BE>(self.unk_10)?;
        writer.write_i32::<BE>(self.unk_14)?;
        writer.write_i32::<BE>(self.unk_18)?;
        writer.write_i32::<BE>(self.unk_1c)?;
        writer.write_i32::<BE>(self.unk_20)?;
        writer.write_i32::<BE>(self.unk_24)?;
        writer.write_i32::<BE>(self.unk_28)?;
        writer.write_i16::<BE>(self.unk_2c)?;
        writer.write_i16::<BE>(self.unk_2e)?;
        writer.write_i32::<BE>(self.unk_30)?;
        Ok(())
    }
}

impl<W: Write + ?Sized> WriteOptionTo<W> for Unk28 {
    type Error = Error;
    fn write_option_to(opt: Option<&Self>, writer: &mut W) -> Result<()> {
        match opt {
            Some(x) => x.write_to(writer),
            None => Ok(writer.write_i32::<BE>(-1)?),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unk2C {
    unk_00: i32,
    unk_04: i32,
    unk_08: i32,
    unk_0c: i32,
    unk_10: i32,
    unk_14: i32,
    unk_18: i32,
    unk_1c: i32,
}

impl<R: Read + ?Sized> ReadOptionFrom<R> for Unk2C {
    type Error = Error;
    fn read_option_from(reader: &mut R) -> Result<Option<Self>> {
        let unk_00 = reader.read_i32::<BE>()?;
        if unk_00 == -1 {
            return Ok(None);
        }
        Ok(Some(Self {
            unk_00,
            unk_04: reader.read_i32::<BE>()?,
            unk_08: reader.read_i32::<BE>()?,
            unk_0c: reader.read_i32::<BE>()?,
            unk_10: reader.read_i32::<BE>()?,
            unk_14: reader.read_i32::<BE>()?,
            unk_18: reader.read_i32::<BE>()?,
            unk_1c: reader.read_i32::<BE>()?,
        }))
    }
}

impl<W: Write + ?Sized> WriteTo<W> for Unk2C {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_i32::<BE>(self.unk_00)?;
        writer.write_i32::<BE>(self.unk_04)?;
        writer.write_i32::<BE>(self.unk_08)?;
        writer.write_i32::<BE>(self.unk_0c)?;
        writer.write_i32::<BE>(self.unk_10)?;
        writer.write_i32::<BE>(self.unk_14)?;
        writer.write_i32::<BE>(self.unk_18)?;
        writer.write_i32::<BE>(self.unk_1c)?;
        Ok(())
    }
}

impl<W: Write + ?Sized> WriteOptionTo<W> for Unk2C {
    type Error = Error;
    fn write_option_to(opt: Option<&Self>, writer: &mut W) -> Result<()> {
        match opt {
            Some(x) => x.write_to(writer),
            None => Ok(writer.write_i32::<BE>(-1)?),
        }
    }
}

/// A scripted event in a stage.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Event {
    /// An event that runs when the stage begins loading.
    Prologue,
    /// An event that runs when the stage is finished loading and about to start.
    Startup,
    /// An event that runs when the player runs out of battery power.
    Dead,
    /// An event that runs when the player presses the pose button.
    Pose,
    /// An event that runs when the time of day cycles between day and night.
    TimeCycle,
    /// See `on_time_up` in `Stage`.
    TimeUp,
    /// An event that runs when an object is interacted with.
    Interact(i32),
}

#[derive(Default, Clone)]
pub struct Stage {
    pub objects: Vec<ObjectPlacement>,
    pub actors: Vec<Actor>,
    pub script: Script,

    /// An event that runs when the stage begins loading.
    pub on_prologue: Option<BlockId>,
    /// An event that runs when the stage is finished loading and about to start.
    pub on_startup: Option<BlockId>,
    /// An event that runs when the player runs out of battery power.
    pub on_dead: Option<BlockId>,
    /// An event that runs when the player presses the pose button.
    pub on_pose: Option<BlockId>,
    /// An event that runs when the time of day cycles between day and night.
    pub on_time_cycle: Option<BlockId>,
    /// This event's meaning is mostly unknown. `ahk.bin` is the only stage to set this event and it
    /// displays a Japanese message which translates to "time up," hence this field's name. It is
    /// unknown whether there is actually a way to trigger this event or if it is a remnant of a
    /// deprecated feature. Despite its name, it does *not* seem related to the in-game timer that
    /// controls the day/night cycle.
    pub on_time_up: Option<BlockId>,

    pub settings: Settings,
    pub unk_28: Vec<Unk28>,
    pub unk_2c: Vec<Unk2C>,
    pub unk_30: Vec<Unk28>,
}

impl Stage {
    /// Creates an empty stage.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a reference to the object at `index`, or an error if the index is invalid.
    pub fn object(&self, index: i32) -> Result<&ObjectPlacement> {
        let uindex = usize::try_from(index).map_err(|_| Error::InvalidObjectIndex(index))?;
        self.objects.get(uindex).ok_or(Error::InvalidObjectIndex(index))
    }

    /// Returns a mutable reference to the object at `index`, or an error if the index is invalid.
    pub fn object_mut(&mut self, index: i32) -> Result<&mut ObjectPlacement> {
        let uindex = usize::try_from(index).map_err(|_| Error::InvalidObjectIndex(index))?;
        self.objects.get_mut(uindex).ok_or(Error::InvalidObjectIndex(index))
    }

    /// Returns an iterator over the events in the stage and their corresponding block IDs.
    pub fn events(&self) -> EventIterator<'_> {
        EventIterator::new(self)
    }

    /// Returns the block assigned to the entry point for `event`, if any.
    ///
    /// This can fail if the event references an invalid object index.
    pub fn event(&self, event: Event) -> Result<Option<BlockId>> {
        Ok(match event {
            Event::Prologue => self.on_prologue,
            Event::Startup => self.on_startup,
            Event::Dead => self.on_dead,
            Event::Pose => self.on_pose,
            Event::TimeCycle => self.on_time_cycle,
            Event::TimeUp => self.on_time_up,
            Event::Interact(index) => self.object(index)?.script,
        })
    }

    /// Assigns `block` to the entry point for `event`.
    ///
    /// This can fail if the event references an invalid object index.
    pub fn set_event(&mut self, event: Event, block: Option<BlockId>) -> Result<()> {
        match event {
            Event::Prologue => self.on_prologue = block,
            Event::Startup => self.on_startup = block,
            Event::Dead => self.on_dead = block,
            Event::Pose => self.on_pose = block,
            Event::TimeCycle => self.on_time_cycle = block,
            Event::TimeUp => self.on_time_up = block,
            Event::Interact(index) => self.object_mut(index)?.script = block,
        }
        Ok(())
    }

    /// Resets every event entry point to `None`.
    pub fn clear_events(&mut self) {
        self.on_prologue = None;
        self.on_startup = None;
        self.on_dead = None;
        self.on_pose = None;
        self.on_time_cycle = None;
        self.on_time_up = None;
        for object in &mut self.objects {
            object.script = None;
        }
    }

    /// Clones the stage without cloning any of the script data. The new stage will have an empty
    /// script with no entry points set.
    #[must_use]
    pub fn clone_without_script(&self) -> Self {
        let mut cloned = Self {
            objects: self.objects.clone(),
            actors: self.actors.clone(),
            script: Script::new(),
            on_prologue: None,
            on_startup: None,
            on_dead: None,
            on_pose: None,
            on_time_cycle: None,
            on_time_up: None,
            settings: self.settings.clone(),
            unk_28: self.unk_28.clone(),
            unk_2c: self.unk_2c.clone(),
            unk_30: self.unk_30.clone(),
        };
        for object in &mut cloned.objects {
            object.script = None;
        }
        cloned
    }

    pub fn read_from<R: ReadSeek + ?Sized>(mut reader: &mut R, libs: &Libs) -> Result<Self> {
        let header = Header::read_from(reader)?;

        reader.seek(SeekFrom::Start(header.settings_offset as u64))?;
        let settings = Settings::read_from(reader)?;

        reader.seek(SeekFrom::Start(header.objects_offset as u64))?;
        let mut objects = NonNoneList::<ObjectPlacement>::read_from(reader)?.into_vec();

        reader.seek(SeekFrom::Start(header.events_offset as u64))?;
        let events = EventTable::read_from(reader, objects.len())?.entry_points;

        reader.seek(SeekFrom::Start(header.actors_offset as u64))?;
        let actors = NonNoneList::<Actor>::read_from(reader)?.into_vec();

        let unk_28 = match header.unk_28_offset {
            Some(offset) => {
                reader.seek(SeekFrom::Start(offset.get() as u64))?;
                NonNoneList::<Unk28>::read_from(reader)?.into_vec()
            }
            None => vec![],
        };
        let unk_2c = match header.unk_2c_offset {
            Some(offset) => {
                reader.seek(SeekFrom::Start(offset.get() as u64))?;
                NonNoneList::<Unk2C>::read_from(reader)?.into_vec()
            }
            None => vec![],
        };
        let unk_30 = match header.unk_30_offset {
            Some(offset) => {
                reader.seek(SeekFrom::Start(offset.get() as u64))?;
                NonNoneList::<Unk28>::read_from(reader)?.into_vec()
            }
            None => vec![],
        };

        let mut script = ScriptReader::with_libs(&mut reader, &libs.script, &libs.entry_points);
        let on_prologue = header.on_prologue.map(|o| script.read_event(o.get())).transpose()?;
        let on_startup = header.on_startup.map(|o| script.read_event(o.get())).transpose()?;
        let on_dead = header.on_dead.map(|o| script.read_event(o.get())).transpose()?;
        let on_pose = header.on_pose.map(|o| script.read_event(o.get())).transpose()?;
        let on_time_cycle = header.on_time_cycle.map(|o| script.read_event(o.get())).transpose()?;
        let on_time_up = header.on_time_up.map(|o| script.read_event(o.get())).transpose()?;
        for (obj, &event) in objects.iter_mut().zip(&events) {
            if event != 0 {
                obj.script = Some(script.read_event(event)?);
            }
        }
        Ok(Self {
            objects,
            actors,
            script: script.finish()?,
            on_prologue,
            on_startup,
            on_dead,
            on_pose,
            on_time_cycle,
            on_time_up,
            settings,
            unk_28,
            unk_2c,
            unk_30,
        })
    }
}

impl<W: WriteSeek + ?Sized> WriteTo<W> for Stage {
    type Error = Error;
    fn write_to(&self, mut writer: &mut W) -> Result<()> {
        assert_eq!(writer.stream_position()?, 0);

        let mut header = Header::default();
        header.write_to(writer)?;

        header.settings_offset = writer.stream_position()? as u32;
        assert!(header.settings_offset == EXPECTED_SETTINGS_OFFSET);
        self.settings.write_to(writer)?;

        header.objects_offset = writer.stream_position()? as u32;
        assert!(header.objects_offset == EXPECTED_OBJECTS_OFFSET);
        NonNoneList(self.objects.as_slice().into()).write_to(writer)?;

        header.events_offset = writer.stream_position()? as u32;
        let mut events = EventTable { entry_points: vec![0; self.objects.len()] };
        events.write_to(writer)?;

        header.actors_offset = writer.stream_position()? as u32;
        NonNoneList(self.actors.as_slice().into()).write_to(writer)?;

        if !self.unk_28.is_empty() {
            header.unk_28_offset = NonZeroU32::new(writer.stream_position()? as u32);
            NonNoneList(self.unk_28.as_slice().into()).write_to(writer)?;
        }
        if !self.unk_2c.is_empty() {
            header.unk_2c_offset = NonZeroU32::new(writer.stream_position()? as u32);
            NonNoneList(self.unk_2c.as_slice().into()).write_to(writer)?;
        }
        if !self.unk_30.is_empty() {
            header.unk_30_offset = NonZeroU32::new(writer.stream_position()? as u32);
            NonNoneList(self.unk_30.as_slice().into()).write_to(writer)?;
        }

        let mut script = ScriptWriter::new(&self.script);
        self.events().try_for_each(|(_, b)| script.add_block(b))?;
        let offsets = script.write_to(&mut writer)?;
        let end_offset = writer.stream_position()?;

        let resolve_event = |b| NonZeroU32::new(offsets.get(b));
        header.on_prologue = self.on_prologue.and_then(resolve_event);
        header.on_startup = self.on_startup.and_then(resolve_event);
        header.on_dead = self.on_dead.and_then(resolve_event);
        header.on_pose = self.on_pose.and_then(resolve_event);
        header.on_time_cycle = self.on_time_cycle.and_then(resolve_event);
        header.on_time_up = self.on_time_up.and_then(resolve_event);
        for (obj, offset) in self.objects.iter().zip(&mut events.entry_points) {
            if let Some(block) = obj.script {
                *offset = offsets.get(block);
            }
        }

        writer.rewind()?;
        header.write_to(writer)?;
        writer.seek(SeekFrom::Start(header.events_offset as u64))?;
        events.write_to(writer)?;
        writer.seek(SeekFrom::Start(end_offset))?;
        Ok(())
    }
}

/// Iterator returned by `Stage::events()` which iterates over the events in a stage and their
/// corresponding block IDs.
pub struct EventIterator<'a> {
    stage: &'a Stage,
    /// Virtual index of the next event to check in `next()`
    front: u32,
    /// Virtual index + 1 of the next event to check in `next_back()`
    back: u32,
}

impl<'a> EventIterator<'a> {
    fn new(stage: &'a Stage) -> Self {
        let num_objects = u32::try_from(stage.objects.len()).unwrap();
        Self { stage, front: 0, back: NUM_EVENTS + num_objects }
    }

    fn get(&self, index: u32) -> Option<(Event, BlockId)> {
        match index {
            0 => self.stage.on_prologue.map(|x| (Event::Prologue, x)),
            1 => self.stage.on_startup.map(|x| (Event::Startup, x)),
            2 => self.stage.on_dead.map(|x| (Event::Dead, x)),
            3 => self.stage.on_pose.map(|x| (Event::Pose, x)),
            4 => self.stage.on_time_cycle.map(|x| (Event::TimeCycle, x)),
            5 => self.stage.on_time_up.map(|x| (Event::TimeUp, x)),
            NUM_EVENTS.. => {
                let id = (index - NUM_EVENTS) as i32;
                let object = self.stage.object(id).unwrap();
                object.script.map(|x| (Event::Interact(id), x))
            }
        }
    }
}

impl Iterator for EventIterator<'_> {
    type Item = (Event, BlockId);

    fn next(&mut self) -> Option<Self::Item> {
        while self.front < self.back {
            let pair = self.get(self.front);
            self.front += 1;
            if pair.is_some() {
                return pair;
            }
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let max_len = self.back - self.front;
        (0, Some(max_len as usize))
    }
}

impl DoubleEndedIterator for EventIterator<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        while self.back > self.front {
            self.back -= 1;
            let pair = self.get(self.back);
            if pair.is_some() {
                return pair;
            }
        }
        None
    }
}

impl FusedIterator for EventIterator<'_> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Object;
    use lazy_static::lazy_static;

    lazy_static! {
        static ref TEST_STAGE: Stage = {
            let mut stage = Stage::new();
            stage.on_prologue = Some(BlockId::new(1));
            stage.on_dead = Some(BlockId::new(2));
            stage.on_time_up = Some(BlockId::new(3));
            stage.objects.push(ObjectPlacement::with_script(Object::NpcFrog, BlockId::new(4)));
            stage.objects.push(ObjectPlacement::new(Object::NpcFrog));
            stage.objects.push(ObjectPlacement::with_script(Object::NpcFrog, BlockId::new(5)));
            stage
        };
    }

    #[test]
    fn test_events() {
        let iter = TEST_STAGE.events();
        assert_eq!(iter.size_hint(), (0, Some(9)));
        let blocks = iter.collect::<Vec<_>>();
        assert_eq!(
            blocks,
            &[
                (Event::Prologue, BlockId::new(1)),
                (Event::Dead, BlockId::new(2)),
                (Event::TimeUp, BlockId::new(3)),
                (Event::Interact(0), BlockId::new(4)),
                (Event::Interact(2), BlockId::new(5)),
            ]
        );
    }

    #[test]
    fn test_events_rev() {
        let blocks = TEST_STAGE.events().rev().collect::<Vec<_>>();
        assert_eq!(
            blocks,
            &[
                (Event::Interact(2), BlockId::new(5)),
                (Event::Interact(0), BlockId::new(4)),
                (Event::TimeUp, BlockId::new(3)),
                (Event::Dead, BlockId::new(2)),
                (Event::Prologue, BlockId::new(1)),
            ]
        );
    }

    #[test]
    fn test_set_and_get_events() {
        let mut stage = Stage::new();

        stage.set_event(Event::Prologue, Some(BlockId::new(1))).unwrap();
        stage.set_event(Event::Startup, Some(BlockId::new(2))).unwrap();
        stage.set_event(Event::Dead, Some(BlockId::new(3))).unwrap();
        stage.set_event(Event::Pose, Some(BlockId::new(4))).unwrap();
        stage.set_event(Event::TimeCycle, Some(BlockId::new(5))).unwrap();
        stage.set_event(Event::TimeUp, Some(BlockId::new(6))).unwrap();

        assert_eq!(stage.event(Event::Prologue).unwrap(), Some(BlockId::new(1)));
        assert_eq!(stage.event(Event::Startup).unwrap(), Some(BlockId::new(2)));
        assert_eq!(stage.event(Event::Dead).unwrap(), Some(BlockId::new(3)));
        assert_eq!(stage.event(Event::Pose).unwrap(), Some(BlockId::new(4)));
        assert_eq!(stage.event(Event::TimeCycle).unwrap(), Some(BlockId::new(5)));
        assert_eq!(stage.event(Event::TimeUp).unwrap(), Some(BlockId::new(6)));

        stage.objects.push(ObjectPlacement::new(Object::NpcFrog));
        stage.objects.push(ObjectPlacement::new(Object::NpcFrog));
        stage.set_event(Event::Interact(0), Some(BlockId::new(7))).unwrap();
        stage.set_event(Event::Interact(1), Some(BlockId::new(8))).unwrap();
        assert_eq!(stage.event(Event::Interact(0)).unwrap(), Some(BlockId::new(7)));
        assert_eq!(stage.event(Event::Interact(1)).unwrap(), Some(BlockId::new(8)));

        assert!(matches!(stage.event(Event::Interact(2)), Err(Error::InvalidObjectIndex(2))));
        assert!(matches!(stage.event(Event::Interact(-1)), Err(Error::InvalidObjectIndex(-1))));
        assert!(matches!(
            stage.set_event(Event::Interact(2), None),
            Err(Error::InvalidObjectIndex(2))
        ));
        assert!(matches!(
            stage.set_event(Event::Interact(-1), None),
            Err(Error::InvalidObjectIndex(-1))
        ));
    }

    #[test]
    fn test_clear_events() {
        let mut stage = Stage::new();
        stage.on_prologue = Some(BlockId::new(1));
        stage.on_startup = Some(BlockId::new(2));
        stage.on_dead = Some(BlockId::new(3));
        stage.on_pose = Some(BlockId::new(4));
        stage.on_time_cycle = Some(BlockId::new(5));
        stage.on_time_up = Some(BlockId::new(6));
        stage.objects.push(ObjectPlacement::with_script(Object::NpcFrog, BlockId::new(4)));
        stage.objects.push(ObjectPlacement::with_script(Object::NpcFrog, BlockId::new(5)));
        stage.clear_events();
        assert!(stage.on_prologue.is_none());
        assert!(stage.on_startup.is_none());
        assert!(stage.on_dead.is_none());
        assert!(stage.on_pose.is_none());
        assert!(stage.on_time_cycle.is_none());
        assert!(stage.on_time_up.is_none());
        for object in &stage.objects {
            assert!(object.script.is_none());
        }
    }
}
