use super::{Error, Result};
use crate::common::ReadFrom;
use byteorder::{ReadBytesExt, BE};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::any::Any;
use std::convert::TryFrom;
use std::fmt::Debug;
use std::io::Read;

/// Supported audio sample formats.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Format {
    PcmS8,
    PcmS16Le,
    PcmS16Be,
    GcAdpcm,
}

impl Format {
    /// The width of the format's smallest addressable unit in bits.
    pub fn bits_per_unit(&self) -> usize {
        match *self {
            Self::PcmS8 => 8,
            Self::PcmS16Le | Self::PcmS16Be => 16,
            Self::GcAdpcm => 4,
        }
    }

    /// Converts an address to a byte offset.
    pub fn address_to_byte(&self, address: usize) -> usize {
        address * self.bits_per_unit() / 8
    }

    /// Converts a byte offset to an address.
    pub fn byte_to_address(&self, byte: usize) -> usize {
        byte * 8 / self.bits_per_unit()
    }

    /// Calculates the number of bytes necessary to fit the given number of units.
    pub fn size_of(&self, units: usize) -> usize {
        (units * self.bits_per_unit() + 7) / 8
    }
}

impl Default for Format {
    fn default() -> Self {
        Self::PcmS16Le
    }
}

impl From<GcFormat> for Format {
    fn from(gc: GcFormat) -> Self {
        match gc {
            GcFormat::Adpcm => Self::GcAdpcm,
            GcFormat::Pcm16 => Self::PcmS16Be,
            GcFormat::Pcm8 => Self::PcmS8,
        }
    }
}

/// GameCube audio sample formats.
#[derive(Debug, Copy, Clone, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u16)]
pub enum GcFormat {
    /// GameCube ADPCM
    Adpcm = 0,
    /// 16-bit big endian PCM
    Pcm16 = 10,
    /// 8-bit PCM
    Pcm8 = 25,
}

impl Default for GcFormat {
    fn default() -> Self {
        Self::Adpcm
    }
}

impl<R: Read> ReadFrom<R> for GcFormat {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let id = reader.read_u16::<BE>()?;
        match Self::try_from(id) {
            Ok(format) => Ok(format),
            Err(_) => Err(Error::UnrecognizedSampleFormat(id)),
        }
    }
}

/// A type tag for an audio sample format.
pub trait FormatTag {
    /// The type of a format-dependent context that can be associated with samples.
    type Context: 'static;

    /// Returns a dynamic format based on the supplied context.
    fn format(context: &Self::Context) -> Format;
}

/// A type tag for an audio sample format which has a static `Format` mapping.
/// This auto-implements `FormatTag`.
pub trait StaticFormat {
    /// The type of a format-dependent context that can be associated with samples.
    type Context: 'static;

    /// Returns the static format.
    fn format_static() -> Format;

    /// Converts an address to a byte offset. See `Format::address_to_byte()`.
    fn address_to_byte(address: usize) -> usize {
        Self::format_static().address_to_byte(address)
    }

    /// Converts a byte offset to an address. See `Format::byte_to_address()`.
    fn byte_to_address(byte: usize) -> usize {
        Self::format_static().byte_to_address(byte)
    }

    /// Calculates the number of bytes necessary to fit the given number of units. See
    /// `Format::size_of()`.
    fn size_of(units: usize) -> usize {
        Self::format_static().size_of(units)
    }
}

impl<T: StaticFormat> FormatTag for T {
    type Context = T::Context;
    fn format(_context: &Self::Context) -> Format {
        Self::format_static()
    }
}

/// Indicates that a format consists solely of raw fixed-width samples which require no context to
/// decode - i.e. addresses and samples are the same unit.
pub trait RawFormat: StaticFormat<Context = ()> {
    /// Converts a sample number and channel count to a byte offset.
    fn sample_to_byte(sample: usize, channels: usize) -> usize {
        Self::address_to_byte(sample) * channels
    }

    /// Converts a byte offset to a sample number.
    fn byte_to_sample(byte: usize, channels: usize) -> usize {
        Self::byte_to_address(byte) / channels
    }
}

/// A format tag which allows samples to be of any known format.
#[derive(Copy, Clone)]
pub struct AnyFormat;
impl FormatTag for AnyFormat {
    type Context = AnyContext;
    fn format(context: &Self::Context) -> Format {
        context.format
    }
}

/// Context for `AnyFormat`.
pub struct AnyContext {
    /// The actual sample format.
    pub(super) format: Format,
    /// The actual context for the sample data.
    pub(super) inner: Box<dyn Any>,
}

impl AnyContext {
    /// Wraps a context in an `AnyContext`.
    pub fn new<T: FormatTag>(inner: T::Context) -> Self {
        Self { format: T::format(&inner), inner: Box::new(inner) }
    }
}

/// Macro for declaring a raw format.
macro_rules! raw_format {
    ($name:ident) => {
        #[derive(Copy, Clone)]
        pub struct $name;
        impl StaticFormat for $name {
            type Context = ();
            fn format_static() -> Format {
                Format::$name
            }
        }
        impl RawFormat for $name {}
    };
}

raw_format!(PcmS8);
raw_format!(PcmS16Le);
raw_format!(PcmS16Be);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address_to_byte() {
        assert_eq!(Format::PcmS8.address_to_byte(0), 0);
        assert_eq!(Format::PcmS8.address_to_byte(1), 1);
        assert_eq!(Format::PcmS8.address_to_byte(2), 2);

        assert_eq!(Format::PcmS16Le.address_to_byte(0), 0);
        assert_eq!(Format::PcmS16Le.address_to_byte(1), 2);
        assert_eq!(Format::PcmS16Le.address_to_byte(2), 4);

        assert_eq!(Format::GcAdpcm.address_to_byte(0), 0);
        assert_eq!(Format::GcAdpcm.address_to_byte(1), 0);
        assert_eq!(Format::GcAdpcm.address_to_byte(2), 1);
    }

    #[test]
    fn test_byte_to_address() {
        assert_eq!(Format::PcmS8.byte_to_address(0), 0);
        assert_eq!(Format::PcmS8.byte_to_address(1), 1);
        assert_eq!(Format::PcmS8.byte_to_address(2), 2);

        assert_eq!(Format::PcmS16Le.byte_to_address(0), 0);
        assert_eq!(Format::PcmS16Le.byte_to_address(1), 0);
        assert_eq!(Format::PcmS16Le.byte_to_address(2), 1);

        assert_eq!(Format::GcAdpcm.byte_to_address(0), 0);
        assert_eq!(Format::GcAdpcm.byte_to_address(1), 2);
        assert_eq!(Format::GcAdpcm.byte_to_address(2), 4);
    }

    #[test]
    fn test_size_of() {
        assert_eq!(Format::PcmS8.size_of(0), 0);
        assert_eq!(Format::PcmS8.size_of(1), 1);
        assert_eq!(Format::PcmS8.size_of(2), 2);
        assert_eq!(Format::PcmS8.size_of(3), 3);

        assert_eq!(Format::PcmS16Le.size_of(0), 0);
        assert_eq!(Format::PcmS16Le.size_of(1), 2);
        assert_eq!(Format::PcmS16Le.size_of(2), 4);
        assert_eq!(Format::PcmS16Le.size_of(3), 6);

        assert_eq!(Format::GcAdpcm.size_of(0), 0);
        assert_eq!(Format::GcAdpcm.size_of(1), 1);
        assert_eq!(Format::GcAdpcm.size_of(2), 1);
        assert_eq!(Format::GcAdpcm.size_of(3), 2);
    }
}
