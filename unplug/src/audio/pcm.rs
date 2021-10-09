use super::format::*;
use super::sample::{AnySamples, ReadSamples, Samples};
use super::{Error, Result};
use float_cmp::approx_eq;
use std::marker::PhantomData;

/// Returns the number of bits needed to represent all values between `min` and `max`.
const fn num_bits(min: u64, max: u64) -> u32 {
    64 - max.wrapping_sub(min).leading_zeros()
}

/// Scales `val` from the range `[from_min, from_max]` into `[to_min, to_max]`. The ranges must
/// cover all the bits of their respective data types.
const fn scale(val: u64, from_min: u64, from_max: u64, to_min: u64, to_max: u64) -> u64 {
    if from_min == to_min && from_max == to_max {
        val
    } else if val == from_min {
        to_min
    } else if val == from_max {
        to_max
    } else {
        let unsigned = val.wrapping_sub(from_min);
        let from_bits = num_bits(from_min, from_max);
        let to_bits = num_bits(to_min, to_max);
        let scaled = if to_bits >= from_bits {
            unsigned.overflowing_shl(to_bits - from_bits).0
        } else {
            // Banker's rounding
            let shift = from_bits - to_bits;
            let half = 1 << (from_bits - to_bits - 1);
            if unsigned & (1 << shift) == 0 {
                unsigned.wrapping_add(half - 1).overflowing_shr(shift).0
            } else {
                unsigned.wrapping_add(half).overflowing_shr(shift).0
            }
        };
        scaled.wrapping_add(to_min)
    }
}

/// Scales `val` from the range `[min, max]` into `[-1.0, 1.0]`.
fn scale_to_f64(val: u32, min: u32, max: u32) -> f64 {
    if val == min {
        -1.0
    } else if val == max {
        1.0
    } else {
        let unsigned = val.wrapping_sub(min);
        let range = max.wrapping_sub(min) + 1;
        f64::from(unsigned) / f64::from(range / 2) - 1.0
    }
}

/// Scales `val` from the range `[-1.0, 1.0]` into `[min, max]`.
fn scale_from_f64(val: f64, min: u32, max: u32) -> u32 {
    let clamped = val.clamp(-1.0, 1.0);
    if approx_eq!(f64, clamped, -1.0, ulps = 2) {
        min
    } else if approx_eq!(f64, clamped, 1.0, ulps = 2) {
        max
    } else {
        let range = max.wrapping_sub(min) + 1;
        let scaled = ((clamped + 1.0) * f64::from(range / 2)).round() as u32;
        scaled.wrapping_add(min)
    }
}

mod private {
    pub trait Sealed {}
    impl Sealed for i8 {}
    impl Sealed for u8 {}
    impl Sealed for i16 {}
    impl Sealed for u16 {}
    impl Sealed for i32 {}
    impl Sealed for u32 {}
    impl Sealed for i64 {}
    impl Sealed for u64 {}
    impl Sealed for f32 {}
    impl Sealed for f64 {}
}

/// Internal trait used to scale PCM samples.
pub trait Scalable: Sized + private::Sealed {
    const IS_FLOAT: bool;
    const MIN: u64;
    const MAX: u64;

    fn to_u64(self) -> u64;
    fn from_u64(val: u64) -> Self;

    fn to_f64(self) -> f64;
    fn from_f64(val: f64) -> Self;

    fn scale<T: Scalable>(self) -> T {
        if Self::IS_FLOAT || T::IS_FLOAT {
            T::from_f64(self.to_f64())
        } else {
            T::from_u64(scale(self.to_u64(), Self::MIN, Self::MAX, T::MIN, T::MAX))
        }
    }
}

macro_rules! scalable {
    ($int:ty) => {
        #[allow(trivial_numeric_casts)]
        impl Scalable for $int {
            const IS_FLOAT: bool = false;
            const MIN: u64 = <$int>::MIN as u64;
            const MAX: u64 = <$int>::MAX as u64;

            fn to_u64(self) -> u64 {
                self as u64
            }

            fn from_u64(val: u64) -> Self {
                val as Self
            }

            fn to_f64(self) -> f64 {
                let from_min = <Self as Scalable>::MIN;
                let from_max = <Self as Scalable>::MAX;
                if from_max > (u32::MAX as u64) {
                    // Scale to u32 to avoid floating-point precision loss
                    scale_to_f64(self.scale(), u32::MIN, u32::MAX)
                } else {
                    scale_to_f64(self as u32, from_min as u32, from_max as u32)
                }
            }

            fn from_f64(val: f64) -> Self {
                let to_min = <Self as Scalable>::MIN;
                let to_max = <Self as Scalable>::MAX;
                if to_max > (u32::MAX as u64) {
                    // Scale to u32 to avoid floating-point precision loss
                    scale_from_f64(val, u32::MIN, u32::MAX).scale()
                } else {
                    scale_from_f64(val, to_min as u32, to_max as u32) as Self
                }
            }
        }
    };
}
scalable!(i8);
scalable!(u8);
scalable!(i16);
scalable!(u16);
scalable!(i32);
scalable!(u32);
scalable!(i64);
scalable!(u64);

impl Scalable for f32 {
    const IS_FLOAT: bool = true;
    const MIN: u64 = i64::MIN as u64;
    const MAX: u64 = i64::MAX as u64;

    fn to_u64(self) -> u64 {
        u64::from_f64(self as f64)
    }

    fn from_u64(val: u64) -> Self {
        u64::to_f64(val) as Self
    }

    fn to_f64(self) -> f64 {
        self as f64
    }

    fn from_f64(val: f64) -> Self {
        val as Self
    }
}

impl Scalable for f64 {
    const IS_FLOAT: bool = true;
    const MIN: u64 = i64::MIN as u64;
    const MAX: u64 = i64::MAX as u64;

    fn to_u64(self) -> u64 {
        u64::from_f64(self)
    }

    fn from_u64(val: u64) -> Self {
        u64::to_f64(val)
    }

    fn to_f64(self) -> f64 {
        self
    }

    fn from_f64(val: f64) -> Self {
        val
    }
}

/// Internal trait for samples that are either PCM or `AnyFormat`.
pub trait AnyPcm: ToFromAny {}
impl<T: PcmFormat + ToFromAny> AnyPcm for T {}
impl AnyPcm for AnyFormat {}

/// Wraps a stream of PCM samples and converts them to another PCM format as they are read. If the
/// samples are already in the target format, they will be passed through.
#[allow(single_use_lifetimes)]
pub struct ConvertPcm<'r, 's: 'r, To>
where
    To: PcmFormat + ToFromAny,
    To::Data: Scalable,
{
    inner: Box<dyn ReadSamples<'s, Format = AnyFormat> + 'r>,
    _marker: PhantomData<To>,
}

impl<'r, 's: 'r, To> ConvertPcm<'r, 's, To>
where
    To: PcmFormat + ToFromAny,
    To::Data: Scalable,
{
    /// Creates a new converter which reads samples from `inner`.
    pub fn new<From: AnyPcm + 'r>(inner: impl ReadSamples<'s, Format = From> + 'r) -> Self {
        Self { inner: Box::new(AnySamples::new(inner)), _marker: PhantomData }
    }

    /// Converts samples from the `From` format to the `To` format.
    fn convert<From: PcmFormat>(samples: Samples<'s, From>) -> Samples<'s, To>
    where
        From::Data: Scalable,
    {
        let mut converted = Vec::with_capacity(samples.len);
        for &sample in &samples.data[..samples.len] {
            converted.push(sample.scale());
        }
        Samples::from_pcm(converted, samples.channels)
    }
}

impl<'r, 's: 'r, To> ReadSamples<'s> for ConvertPcm<'r, 's, To>
where
    To: PcmFormat + ToFromAny,
    To::Data: Scalable,
{
    type Format = To;
    fn read_samples(&mut self) -> Result<Option<Samples<'s, Self::Format>>> {
        let samples = match self.inner.read_samples()? {
            Some(s) => s,
            None => return Ok(None),
        };
        match samples.try_cast::<To>() {
            Ok(casted) => Ok(Some(casted)),
            Err(samples) => Ok(Some(match samples.format() {
                Format::PcmS8 => Self::convert(samples.cast::<PcmS8>()),
                Format::PcmS16Le => Self::convert(samples.cast::<PcmS16Le>()),
                Format::PcmS16Be => Self::convert(samples.cast::<PcmS16Be>()),
                Format::PcmS32Le => Self::convert(samples.cast::<PcmS32Le>()),
                Format::PcmF32Le => Self::convert(samples.cast::<PcmF32Le>()),
                f @ Format::GcAdpcm => return Err(Error::UnsupportedFormat(f)),
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::format::{PcmS16Le, PcmS32Le, ReadWriteBytes};
    use crate::test::{
        open_test_wav, TEST_WAV_F32, TEST_WAV_F32_DATA_OFFSET, TEST_WAV_S32,
        TEST_WAV_S32_DATA_OFFSET,
    };
    use float_cmp::assert_approx_eq;
    use std::borrow::Cow;

    #[test]
    fn test_num_bits() {
        assert_eq!(num_bits(i8::MIN as u64, i8::MAX as u64), 8);
        assert_eq!(num_bits(u8::MIN as u64, u8::MAX as u64), 8);
        assert_eq!(num_bits(i64::MIN as u64, i64::MAX as u64), 64);
        assert_eq!(num_bits(u64::MIN, u64::MAX), 64);
    }

    #[test]
    fn test_scale_up() {
        assert_eq!(i16::MIN.scale::<u64>(), u64::MIN);
        assert_eq!(i16::MAX.scale::<u64>(), u64::MAX);

        assert_eq!((-1i16).scale::<u64>(), 0x7fff000000000000);
        assert_eq!(0i16.scale::<u64>(), 0x8000000000000000);
        assert_eq!(1i16.scale::<u64>(), 0x8001000000000000);
    }

    #[test]
    fn test_scale_down() {
        assert_eq!(u64::MIN.scale::<i16>(), i16::MIN);
        assert_eq!(u64::MAX.scale::<i16>(), i16::MAX);

        assert_eq!(0x7fff000000000000u64.scale::<i16>(), -1);
        assert_eq!(0x8000000000000000u64.scale::<i16>(), 0);
        assert_eq!(0x8001000000000000u64.scale::<i16>(), 1);

        assert_eq!(0x7fff7fffffffffffu64.scale::<i16>(), -1);
        assert_eq!(0x7fff800000000000u64.scale::<i16>(), 0);
        assert_eq!(0x7fff800000000001u64.scale::<i16>(), 0);

        assert_eq!(0x80007fffffffffffu64.scale::<i16>(), 0);
        assert_eq!(0x8000800000000000u64.scale::<i16>(), 0);
        assert_eq!(0x8000800000000001u64.scale::<i16>(), 1);

        assert_eq!(0x80017fffffffffffu64.scale::<i16>(), 1);
        assert_eq!(0x8001800000000000u64.scale::<i16>(), 2);
        assert_eq!(0x8001800000000001u64.scale::<i16>(), 2);
    }

    #[test]
    fn test_scale_to_float() {
        assert_approx_eq!(f64, i8::MIN.scale::<f64>(), -1.0, ulps = 2);
        assert_approx_eq!(f64, 0i8.scale::<f64>(), 0.0, ulps = 2);
        assert_approx_eq!(f64, i8::MAX.scale::<f64>(), 1.0, ulps = 2);

        assert_approx_eq!(f64, (-1i8).scale::<f64>(), -1.0 / 128.0, ulps = 2);
        assert_approx_eq!(f64, (1i8).scale::<f64>(), 1.0 / 128.0, ulps = 2);

        assert_approx_eq!(f64, (-32i8).scale::<f64>(), -0.25, ulps = 2);
        assert_approx_eq!(f64, 32i8.scale::<f64>(), 0.25, ulps = 2);

        assert_approx_eq!(f64, (-64i8).scale::<f64>(), -0.5, ulps = 2);
        assert_approx_eq!(f64, 64i8.scale::<f64>(), 0.5, ulps = 2);
    }

    #[test]
    fn test_scale_from_float() {
        assert_eq!((-1.0).scale::<i8>(), i8::MIN);
        assert_eq!((0.0).scale::<i8>(), 0);
        assert_eq!((1.0).scale::<i8>(), i8::MAX);

        assert_eq!((-1.0 / 128.0).scale::<i8>(), -1);
        assert_eq!((1.0 / 128.0).scale::<i8>(), 1);

        assert_eq!((-0.25).scale::<i8>(), -32);
        assert_eq!((0.25).scale::<i8>(), 32);

        assert_eq!((-0.5).scale::<i8>(), -64);
        assert_eq!((0.5).scale::<i8>(), 64);
    }

    #[test]
    fn test_nop_conversion() -> Result<()> {
        let data = open_test_wav();
        let samples_s16 = Samples::<PcmS16Le>::from_pcm(&data, 2);
        let mut converter = ConvertPcm::<PcmS16Le>::new(samples_s16.into_reader());
        let converted = converter.coalesce_samples()?;
        assert!(matches!(converted.data, Cow::Borrowed(_)));
        assert!(converted.data == data);
        Ok(())
    }

    #[test]
    fn test_pcms16le_to_pcms32le() -> Result<()> {
        let samples_s16 = Samples::<PcmS16Le>::from_pcm(open_test_wav(), 2);
        let mut converter = ConvertPcm::<PcmS32Le>::new(samples_s16.into_reader());
        let samples_s32 = converter.coalesce_samples()?;
        let expected = PcmS32Le::read_bytes(&TEST_WAV_S32[TEST_WAV_S32_DATA_OFFSET..])?;
        assert!(samples_s32.data == expected);
        Ok(())
    }

    #[test]
    fn test_pcms32le_to_pcms16le() -> Result<()> {
        let data = PcmS32Le::read_bytes(&TEST_WAV_S32[TEST_WAV_S32_DATA_OFFSET..])?;
        let samples_s32 = Samples::<PcmS32Le>::from_pcm(data, 2);
        let mut converter = ConvertPcm::<PcmS16Le>::new(samples_s32.into_reader());
        let samples_s16 = converter.coalesce_samples()?;
        assert!(samples_s16.data == open_test_wav());
        Ok(())
    }

    #[test]
    fn test_pcms16le_to_pcmf32le() -> Result<()> {
        let samples_s16 = Samples::<PcmS16Le>::from_pcm(open_test_wav(), 2);
        let mut converter = ConvertPcm::<PcmF32Le>::new(samples_s16.into_reader());
        let samples_f32 = converter.coalesce_samples()?;
        let expected = PcmF32Le::read_bytes(&TEST_WAV_F32[TEST_WAV_F32_DATA_OFFSET..])?;
        assert!(samples_f32.data == expected);
        Ok(())
    }

    #[test]
    fn test_pcmf32le_to_pcms16le() -> Result<()> {
        let data = PcmF32Le::read_bytes(&TEST_WAV_F32[TEST_WAV_F32_DATA_OFFSET..])?;
        let samples_f32 = Samples::<PcmF32Le>::from_pcm(data, 2);
        let mut converter = ConvertPcm::<PcmS16Le>::new(samples_f32.into_reader());
        let samples_s16 = converter.coalesce_samples()?;
        assert!(samples_s16.data == open_test_wav());
        Ok(())
    }
}
