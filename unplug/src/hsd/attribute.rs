use super::array::{Array, ArrayElement};
use super::{ByteArray, Node, Pointer};
use crate::common::ReadFrom;
use crate::hsd::{Error, ReadPointer, Result};
use byteorder::{ReadBytesExt, BE};
use num_enum::{IntoPrimitive, TryFromPrimitive};

/// An array of vertex attributes.
pub type AttributeArray<'a> = Array<'a, Attribute<'a>>;

#[derive(Debug, Copy, Clone, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u32)]
pub enum AttributeName {
    PositionNormalMatrixIndex = 0,
    Texture0MatrixIndex = 1,
    Texture1MatrixIndex = 2,
    Texture2MatrixIndex = 3,
    Texture3MatrixIndex = 4,
    Texture4MatrixIndex = 5,
    Texture5MatrixIndex = 6,
    Texture6MatrixIndex = 7,
    Texture7MatrixIndex = 8,
    Position = 9,
    Normal = 10,
    Color0 = 11,
    Color1 = 12,
    Texture0 = 13,
    Texture1 = 14,
    Texture2 = 15,
    Texture3 = 16,
    Texture4 = 17,
    Texture5 = 18,
    Texture6 = 19,
    Texture7 = 20,
    PositionMatrixArray = 21,
    NormalMatrixArray = 22,
    TextureMatrixArray = 23,
    LightArray = 24,
    NormalBinormalTangent = 25,
    End = 255,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u32)]
pub enum AttributeType {
    #[default]
    None = 0,
    Direct = 1,
    Index8 = 2,
    Index16 = 3,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u32)]
pub enum PositionCount {
    #[default]
    Xy = 0,
    Xyz = 1,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u32)]
pub enum NormalCount {
    #[default]
    Xyz = 0,
    Nbt = 1,
    Nbt3 = 2,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u32)]
pub enum ColorCount {
    #[default]
    Rgb = 0,
    Rgba = 1,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u32)]
pub enum TextureCount {
    #[default]
    S = 0,
    St = 1,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ComponentCount {
    Position(PositionCount),
    Normal(NormalCount),
    Color(ColorCount),
    Texture(TextureCount),
    Other(u32),
}

impl Default for ComponentCount {
    fn default() -> Self {
        Self::Other(0)
    }
}

impl ComponentCount {
    pub fn try_from_primitive(value: u32, name: AttributeName) -> Result<Self> {
        Ok(match name {
            AttributeName::Position => Self::Position(
                PositionCount::try_from_primitive(value)
                    .map_err(|e| Error::UnsupportedComponentCount(name, e.number))?,
            ),
            AttributeName::Normal => Self::Normal(
                NormalCount::try_from_primitive(value)
                    .map_err(|e| Error::UnsupportedComponentCount(name, e.number))?,
            ),
            AttributeName::Color0 | AttributeName::Color1 => Self::Color(
                ColorCount::try_from_primitive(value)
                    .map_err(|e| Error::UnsupportedComponentCount(name, e.number))?,
            ),
            AttributeName::Texture0
            | AttributeName::Texture1
            | AttributeName::Texture2
            | AttributeName::Texture3
            | AttributeName::Texture4
            | AttributeName::Texture5
            | AttributeName::Texture6
            | AttributeName::Texture7 => Self::Texture(
                TextureCount::try_from_primitive(value)
                    .map_err(|e| Error::UnsupportedComponentCount(name, e.number))?,
            ),
            _ => Self::Other(value),
        })
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u32)]
pub enum IntType {
    UInt8 = 0,
    Int8 = 1,
    UInt16 = 2,
    Int16 = 3,
    Float = 4,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u32)]
pub enum ColorType {
    Rgb565 = 0,
    Rgb8 = 1,
    Rgbx8 = 2,
    Rgba4 = 3,
    Rgba6 = 4,
    Rgba8 = 5,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ComponentType {
    Int(IntType),
    Color(ColorType),
}

impl ComponentType {
    pub fn try_from_primitive(value: u32, name: AttributeName) -> Result<Self> {
        Ok(match name {
            AttributeName::Color0 | AttributeName::Color1 => Self::Color(
                ColorType::try_from_primitive(value)
                    .map_err(|e| Error::UnsupportedComponentType(name, e.number))?,
            ),
            _ => Self::Int(
                IntType::try_from_primitive(value)
                    .map_err(|e| Error::UnsupportedComponentType(name, e.number))?,
            ),
        })
    }
}

#[derive(Debug, Clone)]
pub struct Attribute<'a> {
    pub name: AttributeName,
    pub kind: AttributeType,
    pub component_count: ComponentCount,
    pub component_type: ComponentType,
    pub scale: u8,
    pub stride: u16,
    pub data: Pointer<'a, ByteArray<'a>>,
}

impl<'a> Attribute<'a> {
    /// Get the size (in bytes) of the component's data in a display list.
    pub fn display_list_size(&self) -> usize {
        match self.kind {
            AttributeType::None => 0,
            AttributeType::Direct => unimplemented!("direct vertex data is not supported"),
            AttributeType::Index8 => 1,
            AttributeType::Index16 => 2,
        }
    }
}

impl<'a, R: ReadPointer<'a> + ?Sized> ReadFrom<R> for Attribute<'a> {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let name = AttributeName::try_from_primitive(reader.read_u32::<BE>()?)
            .map_err(|e| Error::UnsupportedAttributeName(e.number))?;
        Ok(Self {
            name,
            kind: AttributeType::try_from_primitive(reader.read_u32::<BE>()?)
                .map_err(|e| Error::UnsupportedAttributeType(e.number))?,
            component_count: ComponentCount::try_from_primitive(reader.read_u32::<BE>()?, name)?,
            component_type: ComponentType::try_from_primitive(reader.read_u32::<BE>()?, name)?,
            scale: (reader.read_u16::<BE>()? >> 8) as u8,
            stride: reader.read_u16::<BE>()?,
            data: ByteArray::read_pointer(reader, ByteArray::UNKNOWN_LENGTH)?,
        })
    }
}

impl<'a> Node<'a> for Attribute<'a> {}

impl<'a> ArrayElement for Attribute<'a> {
    fn is_end_of_array(&self) -> bool {
        self.name == AttributeName::End
    }
}
