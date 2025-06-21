use super::dobj::DObj;
use super::{Error, Node, Pointer, ReadPointer, Result};
use crate::common::ReadFrom;
use bitflags::bitflags;
use byteorder::{ReadBytesExt, BE};

bitflags! {
    // From HSDLib
    #[derive(Default)]
    pub struct Flags: u32 {
        const SKELETON = 1 << 0;
        const SKELETON_ROOT = 1 << 1;
        const ENVELOPE_MODEL = 1 << 2;
        const CLASSICAL_SCALING = 1 << 3;
        const HIDDEN = 1 << 4;
        const PTCL = 1 << 5;
        const MTX_DIRTY = 1 << 6;
        const LIGHTING = 1 << 7;
        const TEXGEN = 1 << 8;
        const BILLBOARD = 1 << 9;
        const VBILLBOARD = 2 << 9;
        const HBILLBOARD = 3 << 9;
        const RBILLBOARD = 4 << 9;
        const INSTANCE = 1 << 12;
        const PBILLBOARD = 1 << 13;
        const SPLINE = 1 << 14;
        const FLIP_IK = 1 << 15;
        const SPECULAR = 1 << 16;
        const USE_QUATERNION = 1 << 17;
        const OPA = 1 << 18;
        const XLU = 1 << 19;
        const TEXEDGE = 1 << 20;
        const JOINT1 = 1 << 21;
        const JOINT2 = 2 << 21;
        const EFFECTOR = 3 << 21;
        const USER_DEFINED_MTX = 1 << 23;
        const MTX_INDEPEND_PARENT = 1 << 24;
        const MTX_INDEPEND_SRT = 1 << 25;
        const ROOT_OPA = 1 << 28;
        const ROOT_XLU = 1 << 29;
        const ROOT_TEXEDGE = 1 << 30;
    }
}

#[derive(Debug, Clone)]
pub enum Data<'a> {
    DObj(Pointer<'a, DObj<'a>>),
    Spline(Pointer<'a, ()>),
    ParticleJoint(Pointer<'a, ()>),
}

impl Default for Data<'_> {
    fn default() -> Self {
        Self::DObj(Pointer::new())
    }
}

#[derive(Debug, Default, Clone)]
pub struct JObj<'a> {
    pub name: Pointer<'a, ()>,
    pub flags: Flags,
    pub child: Pointer<'a, JObj<'a>>,
    pub next: Pointer<'a, JObj<'a>>,
    pub data: Data<'a>,
    pub rx: f32,
    pub ry: f32,
    pub rz: f32,
    pub sx: f32,
    pub sy: f32,
    pub sz: f32,
    pub tx: f32,
    pub ty: f32,
    pub tz: f32,
    pub inv_world: Pointer<'a, ()>, // HSD_Matrix4x3
    pub robj: Pointer<'a, ()>,      // HSD_ROBJ
}

impl<'a, R: ReadPointer<'a> + ?Sized> ReadFrom<R> for JObj<'a> {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let mut result = Self {
            name: reader.read_pointer()?,
            flags: Flags::from_bits_truncate(reader.read_u32::<BE>()?),
            child: reader.read_pointer()?,
            next: reader.read_pointer()?,
            ..Default::default()
        };
        result.data = if result.flags.contains(Flags::SPLINE) {
            Data::Spline(reader.read_pointer()?)
        } else if result.flags.contains(Flags::PTCL) {
            Data::ParticleJoint(reader.read_pointer()?)
        } else {
            Data::DObj(reader.read_pointer()?)
        };
        Ok(Self {
            rx: reader.read_f32::<BE>()?,
            ry: reader.read_f32::<BE>()?,
            rz: reader.read_f32::<BE>()?,
            sx: reader.read_f32::<BE>()?,
            sy: reader.read_f32::<BE>()?,
            sz: reader.read_f32::<BE>()?,
            tx: reader.read_f32::<BE>()?,
            ty: reader.read_f32::<BE>()?,
            tz: reader.read_f32::<BE>()?,
            inv_world: reader.read_pointer()?,
            robj: reader.read_pointer()?,
            ..result
        })
    }
}

impl<'a> Node<'a> for JObj<'a> {}
