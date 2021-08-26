use super::{Error, Result};
use crate::common::{ReadOptionFrom, WriteOptionTo, WriteTo};
use crate::data::Object;
use crate::event::BlockId;
use bitflags::bitflags;
use byteorder::{ReadBytesExt, WriteBytesExt, BE};
use std::convert::TryInto;
use std::io::{Read, Write};
use std::num::NonZeroI32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectPlacement {
    pub id: Object,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub rotate_x: i32,
    pub rotate_y: i32,
    pub rotate_z: i32,
    pub scale_x: i32,
    pub scale_y: i32,
    pub scale_z: i32,
    pub unk_40: i32,
    pub flag_index: Option<NonZeroI32>,
    pub unk_48: i32,
    pub flags: ObjectFlags,
    pub script: Option<BlockId>,
}

impl<R: Read> ReadOptionFrom<R> for ObjectPlacement {
    type Error = Error;
    fn read_option_from(reader: &mut R) -> Result<Option<Self>> {
        let id = reader.read_i32::<BE>()?;
        if id < 0 {
            return Ok(None);
        }
        Ok(Some(Self {
            id: id.try_into().map_err(|_| Error::UnrecognizedObject(id))?,
            x: reader.read_i32::<BE>()?,
            y: reader.read_i32::<BE>()?,
            z: reader.read_i32::<BE>()?,
            rotate_x: reader.read_i32::<BE>()?,
            rotate_y: reader.read_i32::<BE>()?,
            rotate_z: reader.read_i32::<BE>()?,
            scale_x: reader.read_i32::<BE>()?,
            scale_y: reader.read_i32::<BE>()?,
            scale_z: reader.read_i32::<BE>()?,
            unk_40: reader.read_i32::<BE>()?,
            flag_index: NonZeroI32::new(reader.read_i32::<BE>()?),
            unk_48: reader.read_i32::<BE>()?,
            flags: ObjectFlags::from_bits_truncate(reader.read_u32::<BE>()?),
            script: None,
        }))
    }
}

impl<W: Write> WriteTo<W> for ObjectPlacement {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_i32::<BE>(self.id.into())?;
        writer.write_i32::<BE>(self.x)?;
        writer.write_i32::<BE>(self.y)?;
        writer.write_i32::<BE>(self.z)?;
        writer.write_i32::<BE>(self.rotate_x)?;
        writer.write_i32::<BE>(self.rotate_y)?;
        writer.write_i32::<BE>(self.rotate_z)?;
        writer.write_i32::<BE>(self.scale_x)?;
        writer.write_i32::<BE>(self.scale_y)?;
        writer.write_i32::<BE>(self.scale_z)?;
        writer.write_i32::<BE>(self.unk_40)?;
        writer.write_i32::<BE>(self.flag_index.map(|i| i.get()).unwrap_or(0))?;
        writer.write_i32::<BE>(self.unk_48)?;
        writer.write_u32::<BE>(self.flags.bits())?;
        Ok(())
    }
}

impl<W: Write> WriteOptionTo<W> for ObjectPlacement {
    type Error = Error;
    fn write_option_to(opt: Option<&Self>, writer: &mut W) -> Result<()> {
        match opt {
            Some(obj) => obj.write_to(writer),
            None => Ok(writer.write_i32::<BE>(-1)?),
        }
    }
}

bitflags! {
    pub struct ObjectFlags: u32 {
        const UNK_0 = 1 << 0;
        const UNK_1 = 1 << 1;
        const UNK_2 = 1 << 2;
        const UNK_3 = 1 << 3;
        const UNK_4 = 1 << 4;
        const UNK_5 = 1 << 5;
        const UNK_6 = 1 << 6;
        const UNK_7 = 1 << 7;
        const UNK_8 = 1 << 8;
        const UNK_9 = 1 << 9;
        const UNK_10 = 1 << 10;
        const UNK_11 = 1 << 11;
        const UNK_12 = 1 << 12;
        const UNK_13 = 1 << 13;
        const UNK_14 = 1 << 14;
        const UNK_15 = 1 << 15;
        /// The object can be picked up and carried.
        const CARRY = 1 << 16;
        /// The player can climb on the object.
        const CLIMB = 1 << 17;
        const UNK_18 = 1 << 18;
        const UNK_19 = 1 << 19;
        /// The player can climb up the object as a rope.
        const ROPE = 1 << 20;
        /// The object is a ledge that warns the player when they're about to fall.
        const LEDGE = 1 << 21;
        const UNK_22 = 1 << 22;
        const UNK_23 = 1 << 23;
        const UNK_24 = 1 << 24;
        const UNK_25 = 1 << 25;
        /// The object responds to attachments.
        const ATC = 1 << 26;
        const UNK_27 = 1 << 27;
        const UNK_28 = 1 << 28;
        const UNK_29 = 1 << 29;
        const UNK_30 = 1 << 30;
        /// The object is killed.
        const DEAD = 1 << 31;
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Actor {
    pub obj: i32,
    pub id: i32,
}

impl<R: Read> ReadOptionFrom<R> for Actor {
    type Error = Error;
    fn read_option_from(reader: &mut R) -> Result<Option<Self>> {
        let obj = reader.read_i32::<BE>()?;
        if obj < 0 {
            return Ok(None);
        }
        Ok(Some(Self { obj, id: reader.read_i32::<BE>()? }))
    }
}

impl<W: Write> WriteTo<W> for Actor {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_i32::<BE>(self.obj)?;
        writer.write_i32::<BE>(self.id)?;
        Ok(())
    }
}

impl<W: Write> WriteOptionTo<W> for Actor {
    type Error = Error;
    fn write_option_to(opt: Option<&Self>, writer: &mut W) -> Result<()> {
        match opt {
            Some(actor) => actor.write_to(writer),
            None => Ok(writer.write_i32::<BE>(-1)?),
        }
    }
}
