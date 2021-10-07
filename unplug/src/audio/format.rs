use super::Result;
use crate::common::endian::{ReadValuesExt, WriteValuesExt};
use byte_slice_cast::*;
use byteorder::{NativeEndian as NE, BE, LE};
use std::any::Any;
use std::borrow::Cow;
use std::fmt::Debug;
use std::io::{Read, Write};
use std::mem;
use std::slice;

/// Supported audio sample formats.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Format {
    PcmS8,
    PcmS16Le,
    PcmS16Be,
    GcAdpcm,
}

impl Format {
    /// Converts an address to a value index.
    pub fn address_to_index(&self, address: usize) -> usize {
        match *self {
            Self::GcAdpcm => address / 2,
            _ => address,
        }
    }

    /// Converts a value index to an address.
    pub fn index_to_address(&self, index: usize) -> usize {
        match *self {
            Self::GcAdpcm => index * 2,
            _ => index,
        }
    }

    /// Calculates the number of values necessary to fit the given number of units.
    pub fn size_of(&self, units: usize) -> usize {
        match *self {
            Self::GcAdpcm => (units + 1) / 2,
            _ => units,
        }
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
    /// The type of the data that this format stores.
    type Data: Clone + 'static;

    /// The type of format-dependent parameters that can be associated with samples.
    type Params: 'static;

    /// Returns a dynamic format based on the supplied parameters.
    fn format(params: &Self::Params) -> Format;
}

/// A type tag for an audio sample format which has a static `Format` mapping.
/// This auto-implements `FormatTag`.
pub trait StaticFormat {
    /// The type of the data that this format stores.
    type Data: Default
        + Clone
        + Copy
        + PartialEq
        + ToByteSlice
        + ToMutByteSlice
        + FromByteSlice
        + 'static;

    /// The type of format-dependent parameters that can be associated with samples.
    type Params: 'static;

    /// Returns the static format.
    fn format_static() -> Format;

    /// Serializes sample data into `writer`.
    fn write_bytes(writer: impl Write, data: &[Self::Data]) -> Result<()>;

    /// Deserializes sample data from `reader`.
    fn read_bytes(reader: impl Read) -> Result<Vec<Self::Data>>;

    /// Converts an address to a value index. See `Format::address_to_index()`.
    fn address_to_index(address: usize) -> usize {
        Self::format_static().address_to_index(address)
    }

    /// Converts a value index to an address. See `Format::index_to_address()`.
    fn index_to_address(index: usize) -> usize {
        Self::format_static().index_to_address(index)
    }

    /// Calculates the number of values necessary to fit the given number of units. See
    /// `Format::size_of()`.
    fn size_of(units: usize) -> usize {
        Self::format_static().size_of(units)
    }

    /// Aligns `address` down to the beginning of a frame.
    fn frame_address(address: usize) -> usize {
        Self::format_static().frame_address(address)
    }

    /// Appends the sample data described by `src` and `src_params` to the sample data described by
    /// `dest` and `dest_params`.
    fn append(
        dest: &mut Cow<'_, [Self::Data]>,
        dest_params: &mut Self::Params,
        src: &[Self::Data],
        src_params: &Self::Params,
    ) -> Result<()>;
}

impl<T: StaticFormat> FormatTag for T {
    type Data = T::Data;
    type Params = T::Params;
    fn format(_info: &Self::Params) -> Format {
        Self::format_static()
    }
}

/// Indicates that a format consists solely of raw fixed-width samples which require no context to
/// decode - i.e. addresses and samples are the same unit.
pub trait RawFormat: StaticFormat<Params = ()> {
    /// Converts a sample number and channel count to a value index.
    fn sample_to_index(sample: usize, channels: usize) -> usize {
        sample * channels
    }

    /// Converts a byte offset to a sample number.
    fn index_to_sample(index: usize, channels: usize) -> usize {
        index / channels
    }
}

/// A format tag which allows samples to be of any known format.
#[derive(Copy, Clone)]
pub struct AnyFormat;
impl FormatTag for AnyFormat {
    type Data = AnyData;
    type Params = AnyParams;
    fn format(params: &Self::Params) -> Format {
        params.format
    }
}

/// Opaque data type for `AnyFormat` sample data.
#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct AnyData(u8);
impl AnyData {
    /// Casts data values to `AnyData`.
    pub(super) fn from_data<T>(data: Cow<'_, [T]>) -> Cow<'_, [AnyData]>
    where
        T: Clone + ToByteSlice + ToMutByteSlice,
    {
        match data {
            // &[T] -> &[u8] -> &[AnyData]
            Cow::Borrowed(s) => {
                // &[T] -> &[u8]
                let b = s.as_byte_slice();

                // &[u8] -> &[AnyData]
                // Safety: AnyData and u8 have the same representation
                Cow::Borrowed(unsafe {
                    slice::from_raw_parts(b.as_ptr() as *const AnyData, b.len())
                })
            }

            // Vec<T> -> Box<[T]> -> &mut [u8] -> Vec<AnyData>
            Cow::Owned(v) => {
                // Vec<T> -> Box<[T]>
                let mut s = v.into_boxed_slice();

                // Box<[T]> -> &mut [u8]
                let b = s.as_mut_byte_slice();

                // &mut [u8] -> Vec<AnyData>
                // Safety: AnyData and u8 have the same representation, and we know the pointer was
                // allocated by a Vec
                let v = unsafe {
                    Vec::from_raw_parts(b.as_mut_ptr() as *mut AnyData, b.len(), b.len())
                };
                mem::forget(s); // Allocation is managed by the Vec now
                Cow::Owned(v)
            }
        }
    }

    /// Casts data values from `AnyData`.
    pub(super) fn to_data<T>(data: Cow<'_, [AnyData]>) -> Cow<'_, [T]>
    where
        T: Clone + FromByteSlice,
    {
        match data {
            // &[AnyData] -> &[u8] -> &[T]
            Cow::Borrowed(s) => {
                // &[AnyData] -> &[u8]
                // Safety: AnyData and u8 have the same representation
                let b = unsafe { slice::from_raw_parts(s.as_ptr() as *const u8, s.len()) };

                // &[u8] -> &[T]
                Cow::Borrowed(b.as_slice_of().expect("cast failed"))
            }

            // Vec<AnyData> -> Box<[AnyData]> -> &mut [u8] -> &mut [T] -> Vec<T>
            Cow::Owned(v) => {
                // Vec<AnyData> -> Box<[AnyData]>
                let mut s = v.into_boxed_slice();

                // Box<[AnyData]> -> &mut [u8]
                // Safety: AnyData and u8 have the same representation
                let b = unsafe { slice::from_raw_parts_mut(s.as_mut_ptr() as *mut u8, s.len()) };

                // &mut [u8] -> &mut [T]
                let v = b.as_mut_slice_of().expect("cast failed");

                // &mut [T] -> Vec<T>
                // Safety: We know the pointer was allocated by a Vec, and as_mut_slice_of() already
                // checked that the pointer is safe
                let v = unsafe { Vec::from_raw_parts(v.as_mut_ptr(), v.len(), v.len()) };
                mem::forget(s); // Allocation is managed by the Vec now
                Cow::Owned(v)
            }
        }
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
    pub fn new<T: StaticFormat>(inner: T::Params) -> Self {
        Self { format: T::format(&inner), inner: Box::from(inner) }
    }
}

/// Macro for declaring a raw format.
macro_rules! raw_format {
    ($name:ident, $data:ty, $endian:ty) => {
        #[derive(Copy, Clone)]
        pub struct $name;
        impl StaticFormat for $name {
            type Data = $data;
            type Params = ();

            fn format_static() -> Format {
                Format::$name
            }

            fn write_bytes(mut writer: impl Write, data: &[Self::Data]) -> Result<()> {
                Ok(writer.write_all_values::<$endian, _>(data)?)
            }

            fn read_bytes(mut reader: impl Read) -> Result<Vec<Self::Data>> {
                Ok(reader.read_values_to_end::<$endian, _>()?)
            }

            fn append(
                dest: &mut Cow<'_, [Self::Data]>,
                _dest_params: &mut Self::Params,
                src: &[Self::Data],
                _src_params: &Self::Params,
            ) -> Result<()> {
                dest.to_mut().extend(src);
                Ok(())
            }
        }
        impl RawFormat for $name {}
    };
}

raw_format!(PcmS8, i8, NE);
raw_format!(PcmS16Le, i16, LE);
raw_format!(PcmS16Be, i16, BE);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address_to_index() {
        assert_eq!(Format::PcmS8.address_to_index(0), 0);
        assert_eq!(Format::PcmS8.address_to_index(1), 1);
        assert_eq!(Format::PcmS8.address_to_index(2), 2);

        assert_eq!(Format::PcmS16Le.address_to_index(0), 0);
        assert_eq!(Format::PcmS16Le.address_to_index(1), 1);
        assert_eq!(Format::PcmS16Le.address_to_index(2), 2);

        assert_eq!(Format::GcAdpcm.address_to_index(0), 0);
        assert_eq!(Format::GcAdpcm.address_to_index(1), 0);
        assert_eq!(Format::GcAdpcm.address_to_index(2), 1);
    }

    #[test]
    fn test_index_to_address() {
        assert_eq!(Format::PcmS8.index_to_address(0), 0);
        assert_eq!(Format::PcmS8.index_to_address(1), 1);
        assert_eq!(Format::PcmS8.index_to_address(2), 2);

        assert_eq!(Format::PcmS16Le.index_to_address(0), 0);
        assert_eq!(Format::PcmS16Le.index_to_address(1), 1);
        assert_eq!(Format::PcmS16Le.index_to_address(2), 2);

        assert_eq!(Format::GcAdpcm.index_to_address(0), 0);
        assert_eq!(Format::GcAdpcm.index_to_address(1), 2);
        assert_eq!(Format::GcAdpcm.index_to_address(2), 4);
    }

    #[test]
    fn test_size_of() {
        assert_eq!(Format::PcmS8.size_of(0), 0);
        assert_eq!(Format::PcmS8.size_of(1), 1);
        assert_eq!(Format::PcmS8.size_of(2), 2);
        assert_eq!(Format::PcmS8.size_of(3), 3);

        assert_eq!(Format::PcmS16Le.size_of(0), 0);
        assert_eq!(Format::PcmS16Le.size_of(1), 1);
        assert_eq!(Format::PcmS16Le.size_of(2), 2);
        assert_eq!(Format::PcmS16Le.size_of(3), 3);

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

    #[test]
    fn test_anydata_borrowed() {
        let values: Vec<i32> = (0..100).collect();

        let borrowed = Cow::from(&values);
        let any = AnyData::from_data(borrowed);
        assert!(matches!(any, Cow::Borrowed(_)));

        let unwrapped = AnyData::to_data::<i32>(any);
        assert!(matches!(unwrapped, Cow::Borrowed(_)));
        assert_eq!(unwrapped, values);
    }

    #[test]
    fn test_anydata_owned() {
        let values: Vec<i32> = (0..100).collect();

        let owned = Cow::from(values.clone());
        let any = AnyData::from_data(owned);
        assert!(matches!(any, Cow::Owned(_)));

        let unwrapped = AnyData::to_data::<i32>(any);
        assert!(matches!(unwrapped, Cow::Owned(_)));
        assert_eq!(unwrapped, values);
    }
}
