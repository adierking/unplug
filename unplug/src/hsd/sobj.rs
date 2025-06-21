use super::jobj::JObj;
use super::{Error, Node, Pointer, PointerArray, ReadPointer, Result};
use crate::common::ReadFrom;

#[derive(Debug, Default, Clone)]
pub struct SObj<'a> {
    pub jobj_descs: Pointer<'a, PointerArray<'a, JObjDesc<'a>>>,
    pub camera: Pointer<'a, ()>, // HSD_Camera
    pub lights: Pointer<'a, ()>, // HSD_Lights
    pub fog: Pointer<'a, ()>,    // HSD_FogAdjDesc
}

impl<'a, R: ReadPointer<'a> + ?Sized> ReadFrom<R> for SObj<'a> {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self {
            jobj_descs: reader.read_pointer()?,
            camera: reader.read_pointer()?,
            lights: reader.read_pointer()?,
            fog: reader.read_pointer()?,
        })
    }
}

#[derive(Debug, Default, Clone)]
pub struct JObjDesc<'a> {
    pub root_joint: Pointer<'a, JObj<'a>>,
    pub joint_anims: Pointer<'a, PointerArray<'a, ()>>, // HSD_AnimJoint
    pub material_anims: Pointer<'a, PointerArray<'a, ()>>, // HSD_MatAnimJoint
    pub shape_anims: Pointer<'a, PointerArray<'a, ()>>, // HSD_ShapeAnimJoint
}

impl<'a, R: ReadPointer<'a> + ?Sized> ReadFrom<R> for JObjDesc<'a> {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self {
            root_joint: reader.read_pointer()?,
            joint_anims: reader.read_pointer()?,
            material_anims: reader.read_pointer()?,
            shape_anims: reader.read_pointer()?,
        })
    }
}

impl<'a> Node<'a> for JObjDesc<'a> {}
impl<'a> Node<'a> for SObj<'a> {}
