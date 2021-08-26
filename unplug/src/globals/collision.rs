use super::{Error, Result};
use crate::common::{NonNoneList, ReadFrom, ReadOptionFrom, WriteOptionTo, WriteTo};
use crate::data::object::{Object, NUM_OBJECTS};
use byteorder::{ReadBytesExt, WriteBytesExt, BE, LE};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::convert::TryInto;
use std::io::{Read, Seek, SeekFrom, Write};
use std::ops::{Index, IndexMut};

#[derive(Debug, Copy, Clone, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(i16)]
pub enum Shape {
    Cylinder = 0,
    Sphere = 1,
    Box = 2,
    Unk3 = 3,
    Unk4 = 4,
    Quad = 5,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(i16)]
pub enum Material {
    Default = 0,
    Wood = 1,
    Ceramic = 2,
    Metal = 3,
    Fabric = 4,
    Rubber = 5,
    Plastic = 6,
    Glass = 7,
    Water = 8,
    Clay = 9,
    Grass = 10,
    Stone = 11,
    Piano = 12,
    Unk13 = 13,
    Unk254 = 254,
    Unk255 = 255,
    Unk768 = 768,
    Unk769 = 769,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collider {
    pub shape: Shape,
    pub material: Material,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub rotate_y: i32,
    pub scale_x: i32,
    pub scale_y: i32,
    pub scale_z: i32,
}

impl<R: Read> ReadOptionFrom<R> for Collider {
    type Error = Error;
    fn read_option_from(reader: &mut R) -> Result<Option<Self>> {
        let shape = reader.read_i16::<BE>()?;
        if shape == -1 {
            return Ok(None);
        }
        let material = reader.read_i16::<BE>()?;
        Ok(Some(Self {
            shape: shape.try_into().map_err(|_| Error::UnrecognizedShape(shape))?,
            material: material.try_into().map_err(|_| Error::UnrecognizedMaterial(material))?,
            x: reader.read_i32::<BE>()?,
            y: reader.read_i32::<BE>()?,
            z: reader.read_i32::<BE>()?,
            rotate_y: reader.read_i32::<BE>()?,
            scale_x: reader.read_i32::<BE>()?,
            scale_y: reader.read_i32::<BE>()?,
            scale_z: reader.read_i32::<BE>()?,
        }))
    }
}

impl<W: Write> WriteTo<W> for Collider {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_i16::<BE>(self.shape.into())?;
        writer.write_i16::<BE>(self.material.into())?;
        writer.write_i32::<BE>(self.x)?;
        writer.write_i32::<BE>(self.y)?;
        writer.write_i32::<BE>(self.z)?;
        writer.write_i32::<BE>(self.rotate_y)?;
        writer.write_i32::<BE>(self.scale_x)?;
        writer.write_i32::<BE>(self.scale_y)?;
        writer.write_i32::<BE>(self.scale_z)?;
        Ok(())
    }
}

impl<W: Write> WriteOptionTo<W> for Collider {
    type Error = Error;
    fn write_option_to(opt: Option<&Self>, writer: &mut W) -> Result<()> {
        match opt {
            Some(x) => x.write_to(writer),
            None => Ok(writer.write_i32::<BE>(-1)?),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectColliders {
    pub objects: Box<[Vec<Collider>]>,
}

impl Index<Object> for ObjectColliders {
    type Output = Vec<Collider>;

    fn index(&self, id: Object) -> &Self::Output {
        &self.objects[i32::from(id) as usize]
    }
}

impl IndexMut<Object> for ObjectColliders {
    fn index_mut(&mut self, id: Object) -> &mut Self::Output {
        &mut self.objects[i32::from(id) as usize]
    }
}

impl<R: Read + Seek> ReadFrom<R> for ObjectColliders {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let mut offsets = Vec::with_capacity(NUM_OBJECTS);
        for _ in 0..NUM_OBJECTS {
            offsets.push(reader.read_u32::<LE>()?);
        }
        let mut objects: Vec<Vec<Collider>> = Vec::with_capacity(NUM_OBJECTS);
        for &offset in &offsets {
            if offset != 0 {
                reader.seek(SeekFrom::Start(offset as u64))?;
                let colliders = NonNoneList::<Collider>::read_from(reader)?;
                objects.push(colliders.into_vec());
            } else {
                objects.push(vec![]);
            }
        }
        Ok(Self { objects: objects.into_boxed_slice() })
    }
}

impl<W: Write + Seek> WriteTo<W> for ObjectColliders {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        assert_eq!(self.objects.len(), NUM_OBJECTS);

        // Write an empty offset table because it has to come first
        let table_offset = writer.seek(SeekFrom::Current(0))?;
        let mut offsets = vec![0u32; NUM_OBJECTS];
        for &offset in &offsets {
            writer.write_u32::<LE>(offset)?;
        }

        // Write out each object's collider list
        for (i, colliders) in self.objects.iter().enumerate() {
            if colliders.is_empty() {
                continue;
            }
            offsets[i] = writer.seek(SeekFrom::Current(0))? as u32;
            NonNoneList(colliders.into()).write_to(writer)?;
        }

        // Go back and fill in the offset table
        let end_offset = writer.seek(SeekFrom::Current(0))?;
        writer.seek(SeekFrom::Start(table_offset))?;
        for &offset in &offsets {
            writer.write_u32::<LE>(offset)?;
        }
        writer.seek(SeekFrom::Start(end_offset))?;
        Ok(())
    }
}
