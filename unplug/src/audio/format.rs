pub mod adpcm;
pub mod dsp;
pub mod pcm;

pub use adpcm::GcAdpcm;
pub use dsp::DspFormat;
pub use pcm::{PcmF32Le, PcmS16Be, PcmS16Le, PcmS24Le, PcmS32Le, PcmS8};

use super::Result;
use crate::common::endian::{ConvertEndian, IsNative, ReadValuesExt, WriteValuesExt};
use byte_slice_cast::*;
use byteorder::ByteOrder;
use std::any::Any;
use std::borrow::Cow;
use std::fmt::Debug;
use std::io::{Read, Write};
use std::mem;
use std::result::Result as StdResult;
use std::slice;

/// Supported audio sample formats.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Format {
    PcmS8,
    PcmS16Le,
    PcmS16Be,
    PcmS24Le,
    PcmS32Le,
    PcmF32Le,
    GcAdpcm,
}

impl Format {
    /// Gets the width of the format's smallest addressable unit in bits.
    pub fn bits(&self) -> usize {
        match *self {
            Self::PcmS8 => 8,
            Self::PcmS16Le | Self::PcmS16Be => 16,
            Self::PcmS24Le => 24,
            Self::PcmS32Le | Self::PcmF32Le => 32,
            Self::GcAdpcm => 4,
        }
    }

    /// Converts an address to a byte offset.
    pub fn address_to_byte(&self, address: usize) -> usize {
        address * self.bits() / 8
    }

    /// Converts an address to a byte offset, rounding up.
    pub fn address_to_byte_up(&self, address: usize) -> usize {
        (address * self.bits() + 7) / 8
    }

    /// Converts a byte offset to an address.
    pub fn byte_to_address(&self, byte: usize) -> usize {
        byte * 8 / self.bits()
    }

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

    /// Aligns `address` down to the beginning of a frame.
    pub fn frame_address(&self, address: usize) -> usize {
        match *self {
            Self::GcAdpcm => address & !0xf,
            _ => address,
        }
    }

    /// Checks whether the format is compatible with data from `other` without conversion. If this
    /// returns `true`, casting sample data between the two formats must be legal.
    pub fn compatible_with(&self, other: Format) -> bool {
        if *self == Format::PcmS16Le || *self == Format::PcmS16Be {
            assert_cast::<PcmS16Le, PcmS16Be>();
            assert_cast::<PcmS16Be, PcmS16Le>();
            other == Format::PcmS16Le || other == Format::PcmS16Be
        } else {
            *self == other
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
    type Data: Clone + Copy + PartialEq + 'static;

    /// The type of format-dependent parameters that can be associated with samples.
    type Params: 'static;
}

pub trait DynamicFormat: FormatTag {
    /// Retrieves the `Format` corresponding to `params`.
    fn format_from_params(params: &Self::Params) -> Format;
}

/// A type tag for an audio sample format which has a static `Format` mapping.
pub trait StaticFormat: FormatTag {
    /// Returns the static `Format`.
    fn format() -> Format;

    /// Gets the width of the format's smallest addressable unit in bits. See
    /// `Format::bits_per_sample()`.
    fn bits(&self) -> usize {
        Self::format().bits()
    }

    /// Converts an address to a byte offset. See `Format::address_to_byte()`.
    fn address_to_byte(address: usize) -> usize {
        Self::format().address_to_byte(address)
    }

    /// Converts an address to a byte offset, rounding up. See `Format::address_to_byte()`.
    fn address_to_byte_up(address: usize) -> usize {
        Self::format().address_to_byte_up(address)
    }

    /// Converts a byte offset to an address. See `Format::byte_to_address()`.
    fn byte_to_address(byte: usize) -> usize {
        Self::format().byte_to_address(byte)
    }

    /// Converts an address to a value index. See `Format::address_to_index()`.
    fn address_to_index(address: usize) -> usize {
        Self::format().address_to_index(address)
    }

    /// Converts a value index to an address. See `Format::index_to_address()`.
    fn index_to_address(index: usize) -> usize {
        Self::format().index_to_address(index)
    }

    /// Aligns `address` down to the beginning of a frame.
    fn frame_address(address: usize) -> usize {
        Self::format().frame_address(address)
    }
}

impl<T: StaticFormat> DynamicFormat for T {
    fn format_from_params(_params: &Self::Params) -> Format {
        Self::format()
    }
}

/// A type tag for an audio sample format which allows blocks of samples to be concatenated.
pub trait ExtendSamples: DynamicFormat {
    /// Appends the sample data described by `src` and `src_params` to the sample data described by
    /// `dest` and `dest_params`.
    fn extend_samples(
        dest: &mut Cow<'_, [Self::Data]>,
        dest_params: &mut Self::Params,
        src: &[Self::Data],
        src_params: &Self::Params,
    ) -> Result<()>;
}

/// A type tag for an audio sample format which can be serialized to/from byte streams.
pub trait ReadWriteBytes: FormatTag {
    /// Deserializes sample data from `reader`.
    fn read_bytes(reader: impl Read) -> Result<Vec<Self::Data>>;

    /// Serializes sample data into `writer`.
    fn write_bytes(writer: impl Write, data: &[Self::Data]) -> Result<()>;
}

/// Indicates that a format represents samples as raw PCM.
/// This auto-implements `ExtendSamples` and `ReadWriteBytes`.
pub trait PcmFormat: StaticFormat + FormatTag<Params = ()> {
    /// The endianness of each sample. Used to auto-implement `ReadWriteBytes`.
    type Endian: ByteOrder + IsNative;

    /// Converts a sample number and channel count to a value index.
    fn sample_to_index(sample: usize, channels: usize) -> usize {
        sample * channels
    }

    /// Converts a byte offset to a sample number.
    fn index_to_sample(index: usize, channels: usize) -> usize {
        index / channels
    }
}

impl<F: PcmFormat> ExtendSamples for F {
    fn extend_samples(
        dest: &mut Cow<'_, [Self::Data]>,
        _dest_params: &mut Self::Params,
        src: &[Self::Data],
        _src_params: &Self::Params,
    ) -> Result<()> {
        dest.to_mut().extend(src);
        Ok(())
    }
}

impl<F: PcmFormat> ReadWriteBytes for F
where
    Self::Data: ToByteSlice + ToMutByteSlice + FromByteSlice,
    for<'a> &'a mut [Self::Data]: ConvertEndian<Self::Data>,
{
    fn read_bytes(mut reader: impl Read) -> Result<Vec<Self::Data>> {
        Ok(reader.read_values_to_end::<F::Endian, _>()?)
    }

    fn write_bytes(mut writer: impl Write, data: &[Self::Data]) -> Result<()> {
        Ok(writer.write_all_values::<F::Endian, _>(data)?)
    }
}

/// A format tag which allows samples to be of any known format.
#[derive(Copy, Clone)]
pub struct AnyFormat;
impl FormatTag for AnyFormat {
    type Data = AnyData;
    type Params = AnyParams;
}
impl DynamicFormat for AnyFormat {
    fn format_from_params(params: &Self::Params) -> Format {
        params.format
    }
}

/// Opaque data type for `AnyFormat` sample data.
#[repr(transparent)]
#[derive(Copy, Clone, PartialEq)]
pub struct AnyData(u8);

/// Parameters for `AnyFormat` samples.
pub struct AnyParams {
    /// The actual sample format.
    pub(super) format: Format,
    /// The actual codec info for the sample data.
    pub(super) inner: Box<dyn Any>,
}

type DataCow<'a, F> = Cow<'a, [<F as FormatTag>::Data]>;

/// Internal trait for a sample format which can be casted to another.
pub trait Cast<F: DynamicFormat>: DynamicFormat {
    fn cast_params(params: Self::Params) -> StdResult<F::Params, Self::Params>;
    fn cast_data(data: DataCow<'_, Self>) -> DataCow<'_, F>;
}

/// Asserts at compile time that `From` can be casted to `To`.
fn assert_cast<From: Cast<To>, To: DynamicFormat>() {}

impl<F: DynamicFormat> Cast<F> for F {
    fn cast_params(params: Self::Params) -> StdResult<F::Params, Self::Params> {
        Ok(params)
    }
    fn cast_data(data: DataCow<'_, Self>) -> DataCow<'_, F> {
        data
    }
}

impl<F: StaticFormat> Cast<AnyFormat> for F
where
    F::Data: ToByteSlice + ToMutByteSlice,
{
    fn cast_params(params: Self::Params) -> StdResult<AnyParams, Self::Params> {
        Ok(AnyParams { format: F::format(), inner: Box::from(params) })
    }

    fn cast_data(data: DataCow<'_, Self>) -> DataCow<'_, AnyFormat> {
        match data {
            // &[Data] -> &[u8] -> &[AnyData]
            Cow::Borrowed(s) => {
                // &[Data] -> &[u8]
                let b = s.as_byte_slice();

                // &[u8] -> &[AnyData]
                // Safety: AnyData and u8 have the same representation
                Cow::Borrowed(unsafe {
                    slice::from_raw_parts(b.as_ptr() as *const AnyData, b.len())
                })
            }

            // Vec<Data> -> Box<[Data]> -> &mut [u8] -> Vec<AnyData>
            Cow::Owned(v) => {
                // Vec<Data> -> Box<[Data]>
                let mut s = v.into_boxed_slice();

                // Box<[Data]> -> &mut [u8]
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
}

impl<F: StaticFormat> Cast<F> for AnyFormat
where
    F::Data: FromByteSlice,
{
    fn cast_params(mut params: Self::Params) -> StdResult<F::Params, Self::Params> {
        if !F::format().compatible_with(params.format) {
            return Err(params);
        }
        match params.inner.downcast() {
            Ok(p) => Ok(*p),
            Err(p) => {
                params.inner = p;
                Err(params)
            }
        }
    }

    fn cast_data(data: DataCow<'_, Self>) -> DataCow<'_, F> {
        match data {
            // &[AnyData] -> &[u8] -> &[Data]
            Cow::Borrowed(s) => {
                // &[AnyData] -> &[u8]
                // Safety: AnyData and u8 have the same representation
                let b = unsafe { slice::from_raw_parts(s.as_ptr() as *const u8, s.len()) };

                // &[u8] -> &[Data]
                Cow::Borrowed(b.as_slice_of().expect("cast failed"))
            }

            // Vec<AnyData> -> Box<[AnyData]> -> &mut [u8] -> &mut [Data] -> Vec<Data>
            Cow::Owned(v) => {
                // Vec<AnyData> -> Box<[AnyData]>
                let mut s = v.into_boxed_slice();

                // Box<[AnyData]> -> &mut [u8]
                // Safety: AnyData and u8 have the same representation
                let b = unsafe { slice::from_raw_parts_mut(s.as_mut_ptr() as *mut u8, s.len()) };

                // &mut [u8] -> &mut [Data]
                let v = b.as_mut_slice_of().expect("cast failed");

                // &mut [Data] -> Vec<Data>
                // Safety: We know the pointer was allocated by a Vec, and as_mut_slice_of() already
                // checked that the pointer is safe
                let v = unsafe { Vec::from_raw_parts(v.as_mut_ptr(), v.len(), v.len()) };
                mem::forget(s); // Allocation is managed by the Vec now
                Cow::Owned(v)
            }
        }
    }
}

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
    fn test_address_to_byte_up() {
        assert_eq!(Format::PcmS8.address_to_byte_up(0), 0);
        assert_eq!(Format::PcmS8.address_to_byte_up(1), 1);
        assert_eq!(Format::PcmS8.address_to_byte_up(2), 2);

        assert_eq!(Format::PcmS16Le.address_to_byte_up(0), 0);
        assert_eq!(Format::PcmS16Le.address_to_byte_up(1), 2);
        assert_eq!(Format::PcmS16Le.address_to_byte_up(2), 4);

        assert_eq!(Format::GcAdpcm.address_to_byte_up(0), 0);
        assert_eq!(Format::GcAdpcm.address_to_byte_up(1), 1);
        assert_eq!(Format::GcAdpcm.address_to_byte_up(2), 1);
        assert_eq!(Format::GcAdpcm.address_to_byte_up(3), 2);
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
    fn test_frame_address() {
        assert_eq!(Format::PcmS16Le.frame_address(123), 123);
        assert_eq!(Format::GcAdpcm.frame_address(0xf), 0);
        assert_eq!(Format::GcAdpcm.frame_address(0x10), 0x10);
        assert_eq!(Format::GcAdpcm.frame_address(0x11), 0x10);
    }

    #[test]
    fn test_compatible_with() {
        assert!(Format::GcAdpcm.compatible_with(Format::GcAdpcm));
        assert!(!Format::GcAdpcm.compatible_with(Format::PcmS16Le));

        assert!(Format::PcmS16Le.compatible_with(Format::PcmS16Le));
        assert!(Format::PcmS16Le.compatible_with(Format::PcmS16Be));
        assert!(Format::PcmS16Be.compatible_with(Format::PcmS16Le));
        assert!(Format::PcmS16Be.compatible_with(Format::PcmS16Be));
    }

    #[test]
    fn test_anydata_borrowed() {
        let values: Vec<i16> = (0..100).collect();

        let borrowed = Cow::from(&values);
        let any = <PcmS16Le as Cast<AnyFormat>>::cast_data(borrowed);
        assert!(matches!(any, Cow::Borrowed(_)));

        let unwrapped = <AnyFormat as Cast<PcmS16Le>>::cast_data(any);
        assert!(matches!(unwrapped, Cow::Borrowed(_)));
        assert_eq!(unwrapped, values);
    }

    #[test]
    fn test_anydata_owned() {
        let values: Vec<i16> = (0..100).collect();

        let owned = Cow::from(values.clone());
        let any = <PcmS16Le as Cast<AnyFormat>>::cast_data(owned);
        assert!(matches!(any, Cow::Owned(_)));

        let unwrapped = <AnyFormat as Cast<PcmS16Le>>::cast_data(any);
        assert!(matches!(unwrapped, Cow::Owned(_)));
        assert_eq!(unwrapped, values);
    }
}
