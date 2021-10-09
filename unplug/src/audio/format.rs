use super::Result;
use crate::common::endian::{ConvertEndian, IsNative, ReadValuesExt, WriteValuesExt};
use byte_slice_cast::*;
use byteorder::{ByteOrder, NativeEndian as NE, BE, LE};
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

    /// Converts an address to a value index. See `Format::address_to_index()`.
    fn address_to_index(address: usize) -> usize {
        Self::format().address_to_index(address)
    }

    /// Converts a value index to an address. See `Format::index_to_address()`.
    fn index_to_address(index: usize) -> usize {
        Self::format().index_to_address(index)
    }

    /// Calculates the number of values necessary to fit the given number of units. See
    /// `Format::size_of()`.
    fn size_of(units: usize) -> usize {
        Self::format().size_of(units)
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

#[allow(single_use_lifetimes)]
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

/// Implementation detail for formats that can be converted to/from `AnyFormat`.
pub trait ToFromAny: FormatTag {
    fn wrap_params(params: Self::Params) -> AnyParams;
    fn unwrap_params(params: AnyParams) -> std::result::Result<Self::Params, AnyParams>;
    fn into_any(data: Cow<'_, [Self::Data]>) -> Cow<'_, [AnyData]>;
    fn from_any(data: Cow<'_, [AnyData]>) -> Cow<'_, [Self::Data]>;
}

impl ToFromAny for AnyFormat {
    fn wrap_params(params: Self::Params) -> AnyParams {
        params
    }
    fn unwrap_params(params: AnyParams) -> std::result::Result<Self::Params, AnyParams> {
        Ok(params)
    }
    fn into_any(data: Cow<'_, [Self::Data]>) -> Cow<'_, [AnyData]> {
        data
    }
    fn from_any(data: Cow<'_, [AnyData]>) -> Cow<'_, [Self::Data]> {
        data
    }
}

impl<T: StaticFormat> ToFromAny for T
where
    T::Data: ToByteSlice + ToMutByteSlice + FromByteSlice,
{
    fn wrap_params(params: Self::Params) -> AnyParams {
        AnyParams { format: T::format(), inner: Box::from(params) }
    }

    fn unwrap_params(mut params: AnyParams) -> std::result::Result<Self::Params, AnyParams> {
        match params.inner.downcast() {
            Ok(p) => Ok(*p),
            Err(p) => {
                params.inner = p;
                Err(params)
            }
        }
    }

    fn into_any(data: Cow<'_, [Self::Data]>) -> Cow<'_, [AnyData]> {
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

    fn from_any(data: Cow<'_, [AnyData]>) -> Cow<'_, [Self::Data]> {
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

/// Declares a PCM format.
macro_rules! pcm_format {
    ($name:ident, $data:ty, $endian:ty) => {
        #[derive(Copy, Clone)]
        pub struct $name;
        impl FormatTag for $name {
            type Data = $data;
            type Params = ();
        }
        impl StaticFormat for $name {
            fn format() -> Format {
                Format::$name
            }
        }
        impl PcmFormat for $name {
            type Endian = $endian;
        }
    };
}

pcm_format!(PcmS8, i8, NE);
pcm_format!(PcmS16Le, i16, LE);
pcm_format!(PcmS16Be, i16, BE);

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
        let values: Vec<i16> = (0..100).collect();

        let borrowed = Cow::from(&values);
        let any = PcmS16Le::into_any(borrowed);
        assert!(matches!(any, Cow::Borrowed(_)));

        let unwrapped = PcmS16Le::from_any(any);
        assert!(matches!(unwrapped, Cow::Borrowed(_)));
        assert_eq!(unwrapped, values);
    }

    #[test]
    fn test_anydata_owned() {
        let values: Vec<i16> = (0..100).collect();

        let owned = Cow::from(values.clone());
        let any = PcmS16Le::into_any(owned);
        assert!(matches!(any, Cow::Owned(_)));

        let unwrapped = PcmS16Le::from_any(any);
        assert!(matches!(unwrapped, Cow::Owned(_)));
        assert_eq!(unwrapped, values);
    }
}
