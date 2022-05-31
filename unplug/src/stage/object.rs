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
    /// The object being placed.
    pub id: Object,
    /// X coordinate in hundredths
    pub x: i32,
    /// Y coordinate in hundredths
    pub y: i32,
    /// Z coordinate in hundredths
    pub z: i32,
    /// X rotation in degrees
    pub rotate_x: i32,
    /// Y rotation in degrees
    pub rotate_y: i32,
    /// Z rotation in degrees
    pub rotate_z: i32,
    /// X scale in hundredths
    pub scale_x: i32,
    /// Y scale in hundredths
    pub scale_y: i32,
    /// Z scale in hundredths
    pub scale_z: i32,
    /// Auxiliary data value whose meaning depends on context:
    /// - Staircase objects use this to indicate the stair height
    /// - Sometimes used with npc_spider_1 and npc_spider_2, unknown how
    /// - Used with house_r_chibi_h_hasi (bridge utilibot), unknown how
    pub data: i32,
    /// A per-level flag index which is used to control whether the object should spawn. Typically
    /// the purpose of this is to make it so that items don't respawn after you pick them up.
    pub spawn_flag: Option<NonZeroI32>,
    /// An object-dependent variant index. For example, soda cans use this to select which texture
    /// to display.
    pub variant: i32,
    /// Flags which control the object's behavior.
    pub flags: ObjectFlags,
    /// The script to run when the object is interacted with.
    pub script: Option<BlockId>,
}

impl<R: Read + ?Sized> ReadOptionFrom<R> for ObjectPlacement {
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
            data: reader.read_i32::<BE>()?,
            spawn_flag: NonZeroI32::new(reader.read_i32::<BE>()?),
            variant: reader.read_i32::<BE>()?,
            flags: ObjectFlags::from_bits_truncate(reader.read_u32::<BE>()?),
            script: None,
        }))
    }
}

impl<W: Write + ?Sized> WriteTo<W> for ObjectPlacement {
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
        writer.write_i32::<BE>(self.data)?;
        writer.write_i32::<BE>(self.spawn_flag.map(|i| i.get()).unwrap_or(0))?;
        writer.write_i32::<BE>(self.variant)?;
        writer.write_u32::<BE>(self.flags.bits())?;
        Ok(())
    }
}

impl<W: Write + ?Sized> WriteOptionTo<W> for ObjectPlacement {
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
        /// Spawn the object when the stage loads.
        const SPAWN = 1 << 0;
        const UNK_1 = 1 << 1;
        const UNK_2 = 1 << 2;
        /// The radar will point to the object if it is nearby.
        const RADAR = 1 << 3;
        const UNK_4 = 1 << 4;
        const UNK_5 = 1 << 5;
        const UNK_6 = 1 << 6;
        /// The object is an item.
        const ITEM = 1 << 7;
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
        /// The player can clamber up to surfaces on the object.
        const CLAMBER = 1 << 18;
        /// The player can climb up the object as a ladder.
        const LADDER = 1 << 19;
        /// The player can climb up the object as a rope.
        const ROPE = 1 << 20;
        /// The object is a staircase (i.e. it has internal ledges).
        const STAIRS = 1 << 21;
        /// Gravity applies to the object.
        const GRAVITY = 1 << 22;
        /// The player can grab the object and push/pull it.
        const GRAB = 1 << 23;
        /// The object can be interacted with by walking up to it and pressing A.
        const INTERACT = 1 << 24;
        /// The object responds to being touched by the player.
        const TOUCH = 1 << 25;
        /// The object responds to attachments.
        const ATC = 1 << 26;
        /// The object responds to projectiles.
        const PROJECTILE = 1 << 27;
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

impl<R: Read + ?Sized> ReadOptionFrom<R> for Actor {
    type Error = Error;
    fn read_option_from(reader: &mut R) -> Result<Option<Self>> {
        let obj = reader.read_i32::<BE>()?;
        if obj < 0 {
            return Ok(None);
        }
        Ok(Some(Self { obj, id: reader.read_i32::<BE>()? }))
    }
}

impl<W: Write + ?Sized> WriteTo<W> for Actor {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_i32::<BE>(self.obj)?;
        writer.write_i32::<BE>(self.id)?;
        Ok(())
    }
}

impl<W: Write + ?Sized> WriteOptionTo<W> for Actor {
    type Error = Error;
    fn write_option_to(opt: Option<&Self>, writer: &mut W) -> Result<()> {
        match opt {
            Some(actor) => actor.write_to(writer),
            None => Ok(writer.write_i32::<BE>(-1)?),
        }
    }
}
