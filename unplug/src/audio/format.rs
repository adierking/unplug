use super::Result;
use std::any::Any;
use std::borrow::Cow;
use std::fmt::Debug;

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

    /// Aligns `address` down to the beginning of a frame.
    pub fn frame_address(&self, address: usize) -> usize {
        match *self {
            Self::PcmS8 | Self::PcmS16Le | Self::PcmS16Be => address,
            Self::GcAdpcm => address & !0xf,
        }
    }
}

impl Default for Format {
    fn default() -> Self {
        Self::PcmS16Le
    }
}

/// A type tag for an audio sample format.
pub trait FormatTag {
    /// The type of format-dependent parameters that can be associated with samples.
    type Params: 'static;

    /// Returns a dynamic format based on the supplied parameters.
    fn format(params: &Self::Params) -> Format;
}

/// A type tag for an audio sample format which has a static `Format` mapping.
/// This auto-implements `FormatTag`.
pub trait StaticFormat {
    /// The type of format-dependent parameters that can be associated with samples.
    type Params: 'static;

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

    /// Aligns `address` down to the beginning of a frame.
    fn frame_address(&self, address: usize) -> usize {
        Self::format_static().frame_address(address)
    }

    /// Appends the sample data described by `src` and `src_params` to the sample data described by
    /// `dest` and `dest_params`.
    fn append(
        dest: &mut Cow<'_, [u8]>,
        dest_params: &mut Self::Params,
        src: &[u8],
        src_params: &Self::Params,
    ) -> Result<()>;
}

impl<T: StaticFormat> FormatTag for T {
    type Params = T::Params;
    fn format(_info: &Self::Params) -> Format {
        Self::format_static()
    }
}

/// Indicates that a format consists solely of raw fixed-width samples which require no context to
/// decode - i.e. addresses and samples are the same unit.
pub trait RawFormat: StaticFormat<Params = ()> {
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
    type Params = AnyParams;
    fn format(params: &Self::Params) -> Format {
        params.format
    }
}

/// Parameters for `AnyFormat` samples.
pub struct AnyParams {
    /// The actual sample format.
    pub(super) format: Format,
    /// The actual codec info for the sample data.
    pub(super) inner: Box<dyn Any>,
}

impl AnyParams {
    /// Wraps codec info in an `AnyInfo`.
    pub fn new<T: FormatTag>(inner: T::Params) -> Self {
        Self { format: T::format(&inner), inner: Box::new(inner) }
    }
}

/// Macro for declaring a raw format.
macro_rules! raw_format {
    ($name:ident) => {
        #[derive(Copy, Clone)]
        pub struct $name;
        impl StaticFormat for $name {
            type Params = ();
            fn format_static() -> Format {
                Format::$name
            }

            fn append(
                dest: &mut Cow<'_, [u8]>,
                _dest_params: &mut Self::Params,
                src: &[u8],
                _src_params: &Self::Params,
            ) -> Result<()> {
                dest.to_mut().extend(src);
                Ok(())
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

    #[test]
    fn test_frame_address() {
        assert_eq!(Format::PcmS16Le.frame_address(123), 123);
        assert_eq!(Format::GcAdpcm.frame_address(0xf), 0);
        assert_eq!(Format::GcAdpcm.frame_address(0x10), 0x10);
        assert_eq!(Format::GcAdpcm.frame_address(0x11), 0x10);
    }
}
