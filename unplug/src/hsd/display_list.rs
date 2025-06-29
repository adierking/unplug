use super::{Error, Result};
use crate::common::I24;
use byteorder::{ByteOrder, BE};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use tracing::{debug, trace};

const PRIMITIVE_MASK: u8 = 0x78;
const PRIMITIVE_SHIFT: u32 = 3;
const VAT_MASK: u8 = 0x07;

#[derive(Debug, Copy, Clone, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u8)]
pub enum PrimitiveType {
    Quads = 0x0,
    Quads2 = 0x1,
    Triangles = 0x2,
    TriangleStrip = 0x3,
    TriangleFan = 0x4,
    Lines = 0x5,
    LineStrip = 0x6,
    Points = 0x7,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
    Nop = 0x00,
    LoadBp = 0x61,
    LoadCp = 0x08,
    LoadXf = 0x10,
    LoadIndexA = 0x20,
    LoadIndexB = 0x28,
    LoadIndexC = 0x30,
    LoadIndexD = 0x38,
    CallDisplayList = 0x40,
    UnknownMetrics = 0x44,
    InvalidateVertexCache = 0x48,
    PrimitiveStart = 0x80,
    PrimitiveEnd = 0xbf,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Primitive<'a> {
    pub kind: PrimitiveType,
    pub vat: u8,
    pub data: &'a [u8],
}

// .dat files only seem to contain primitive instructions, but for completeness' sake, this is
// based off of Dolphin.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Instruction<'a> {
    Nop,
    LoadCp { cmd: u8, value: u32 },
    LoadBp { cmd: u8, value: I24 },
    LoadXf { address: u16, data: &'a [u8] },
    LoadIndexA(u32),
    LoadIndexB(u32),
    LoadIndexC(u32),
    LoadIndexD(u32),
    CallDisplayList { address: u32, size: u32 },
    UnknownMetrics,
    InvalidateVertexCache,
    Primitive(Primitive<'a>),
}

impl<'a> Instruction<'a> {
    pub fn parse(bytes: &'a [u8], vertex_size: usize) -> Result<Self> {
        if bytes.is_empty() {
            return Err(Error::NotEnoughData);
        }
        let op = bytes[0];
        match op {
            _ if op == (Opcode::Nop as u8) => Ok(Self::Nop),
            _ if op == (Opcode::UnknownMetrics as u8) => Ok(Self::UnknownMetrics),
            _ if op == (Opcode::InvalidateVertexCache as u8) => Ok(Self::InvalidateVertexCache),

            _ if op == (Opcode::LoadBp as u8) => {
                if bytes.len() >= 5 {
                    let cmd = bytes[1];
                    let value = I24::new(BE::read_i24(&bytes[2..5]));
                    Ok(Self::LoadBp { cmd, value })
                } else {
                    Err(Error::NotEnoughData)
                }
            }

            _ if op == (Opcode::LoadCp as u8) => {
                if bytes.len() >= 6 {
                    let cmd = bytes[1];
                    let value = BE::read_u32(&bytes[2..6]);
                    Ok(Self::LoadCp { cmd, value })
                } else {
                    Err(Error::NotEnoughData)
                }
            }

            _ if op == (Opcode::LoadXf as u8) => {
                if bytes.len() >= 5 {
                    let value = BE::read_u32(&bytes[1..5]);
                    let address = (value & 0xffff) as u16;
                    let size = ((((value >> 16) & 0xf) + 1) * 4) as usize;
                    if bytes.len() >= 5 + size {
                        Ok(Self::LoadXf { address, data: &bytes[5..(5 + size)] })
                    } else {
                        Err(Error::NotEnoughData)
                    }
                } else {
                    Err(Error::NotEnoughData)
                }
            }

            _ if op >= (Opcode::LoadIndexA as u8) && op <= (Opcode::LoadIndexD as u8) => {
                if bytes.len() >= 5 {
                    let value = BE::read_u32(&bytes[1..5]);
                    match op {
                        _ if op == Opcode::LoadIndexA as u8 => Ok(Self::LoadIndexA(value)),
                        _ if op == Opcode::LoadIndexB as u8 => Ok(Self::LoadIndexB(value)),
                        _ if op == Opcode::LoadIndexC as u8 => Ok(Self::LoadIndexC(value)),
                        _ if op == Opcode::LoadIndexD as u8 => Ok(Self::LoadIndexD(value)),
                        _ => unreachable!(),
                    }
                } else {
                    Err(Error::NotEnoughData)
                }
            }

            _ if op == (Opcode::CallDisplayList as u8) => {
                if bytes.len() >= 9 {
                    let address = BE::read_u32(&bytes[1..5]);
                    let size = BE::read_u32(&bytes[5..9]);
                    Ok(Self::CallDisplayList { address, size })
                } else {
                    Err(Error::NotEnoughData)
                }
            }

            // This is the main one we care about.
            _ if op >= (Opcode::PrimitiveStart as u8) && op <= (Opcode::PrimitiveEnd as u8) => {
                if bytes.len() >= 3 {
                    let primitive_idx = (op & PRIMITIVE_MASK) >> PRIMITIVE_SHIFT;
                    let vat = op & VAT_MASK;
                    if vat != 0 {
                        debug!("Nonzero VAT: 0x{vat:x}");
                    }
                    let primitive = PrimitiveType::try_from_primitive(primitive_idx)
                        .map_err(|e| Error::UnsupportedPrimitiveType(e.number))?;
                    let count = BE::read_u16(&bytes[1..3]);
                    let size = count as usize * vertex_size;
                    if bytes.len() >= 3 + size {
                        Ok(Self::Primitive(Primitive {
                            kind: primitive,
                            vat,
                            data: &bytes[3..(3 + size)],
                        }))
                    } else {
                        Err(Error::NotEnoughData)
                    }
                } else {
                    Err(Error::NotEnoughData)
                }
            }

            _ => Err(Error::UnsupportedOpcode(op)),
        }
    }

    pub fn size(&self) -> usize {
        match self {
            Self::Nop | Self::UnknownMetrics | Self::InvalidateVertexCache => 1,
            Self::LoadCp { .. } => 6,
            Self::LoadBp { .. } => 5,
            Self::LoadXf { data, .. } => 5 + data.len(),
            Self::LoadIndexA(_)
            | Self::LoadIndexB(_)
            | Self::LoadIndexC(_)
            | Self::LoadIndexD(_) => 5,
            Self::CallDisplayList { .. } => 9,
            Self::Primitive(primitive) => 3 + primitive.data.len(),
        }
    }

    pub fn primitive(&self) -> Option<&Primitive<'a>> {
        match self {
            Self::Primitive(primitive) => Some(primitive),
            _ => None,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct DisplayList<'a> {
    instructions: Vec<Instruction<'a>>,
}

impl<'a> DisplayList<'a> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_instructions(instructions: impl IntoIterator<Item = Instruction<'a>>) -> Self {
        Self { instructions: instructions.into_iter().collect() }
    }

    pub fn parse(bytes: &'a [u8], vertex_size: usize) -> Result<Self> {
        trace!("Parse display list len={} vertex_size={vertex_size}: {bytes:02x?}", bytes.len());
        let mut instructions = Vec::new();
        let mut cur = bytes;
        while !cur.is_empty() {
            let instruction = Instruction::parse(cur, vertex_size)?;
            cur = &cur[instruction.size()..];
            if instruction != Instruction::Nop {
                instructions.push(instruction);
            }
        }
        Ok(Self { instructions })
    }

    pub fn instructions(&self) -> &[Instruction<'a>] {
        &self.instructions
    }

    pub fn primitives(&self) -> impl Iterator<Item = &Primitive<'a>> {
        self.instructions().iter().filter_map(|i| i.primitive())
    }

    pub fn iter(&self) -> impl Iterator<Item = &Instruction<'a>> {
        self.instructions.iter()
    }
}
