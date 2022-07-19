use super::{Error, Result};
use crate::common::{ReadOptionFrom, WriteOptionTo, WriteTo};
use crate::data::{Object, ObjectFlags, RawObjectPlacement};
use crate::event::BlockId;
use byteorder::{ReadBytesExt, WriteBytesExt, BE};
use std::convert::TryInto;
use std::io::{Read, Write};
use std::num::NonZeroI32;

/// Defines the placement of an object in a stage.
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
    /// A per-level and per-object-type flag index which is used to control whether the object
    /// should spawn. Typically the purpose of this is to make it so that items don't respawn after
    /// you pick them up.
    pub spawn_flag: Option<NonZeroI32>,
    /// An object-dependent variant index. For example, soda cans use this to select which texture
    /// to display.
    pub variant: i32,
    /// Flags which control the object's behavior.
    pub flags: ObjectFlags,
    /// The script to run when the object is interacted with.
    pub script: Option<BlockId>,
}

impl ObjectPlacement {
    /// Creates an object placement for `id` with properties initialized to reasonable defaults.
    pub fn new(id: Object) -> Self {
        Self::new_impl(id, None)
    }

    /// Creates an object placement for `id` with properties initialized to reasonable defaults and
    /// `script` assigned.
    pub fn with_script(id: Object, script: BlockId) -> Self {
        Self::new_impl(id, Some(script))
    }

    fn new_impl(id: Object, script: Option<BlockId>) -> Self {
        Self {
            id,
            x: 0,
            y: 0,
            z: 0,
            rotate_x: 0,
            rotate_y: 0,
            rotate_z: 0,
            scale_x: 100,
            scale_y: 100,
            scale_z: 100,
            data: 0,
            spawn_flag: None,
            variant: 0,
            flags: ObjectFlags::empty(),
            script,
        }
    }
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

impl From<RawObjectPlacement> for ObjectPlacement {
    fn from(raw: RawObjectPlacement) -> Self {
        Self {
            id: raw.id,
            x: raw.x,
            y: raw.y,
            z: raw.z,
            rotate_x: raw.rotate_x,
            rotate_y: raw.rotate_y,
            rotate_z: raw.rotate_z,
            scale_x: raw.scale_x,
            scale_y: raw.scale_y,
            scale_z: raw.scale_z,
            data: raw.data,
            spawn_flag: NonZeroI32::new(raw.spawn_flag),
            variant: raw.variant,
            flags: raw.flags,
            script: None,
        }
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
