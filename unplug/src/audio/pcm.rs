use super::format::*;
use super::sample::{CastSamples, ReadSamples, Samples};
use super::{Error, Result};
use float_cmp::approx_eq;
use std::marker::PhantomData;

mod private {
    pub struct TypeInfo {
        pub is_float: bool,
        pub min: u64,
        pub max: u64,
        pub bits: u32,
    }

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
use private::*;

/// Scales `val` between two data types.
const fn scale(val: u64, from: TypeInfo, to: TypeInfo) -> u64 {
    if from.bits == to.bits && from.min == to.min {
        val
    } else {
        let unsigned = val.wrapping_sub(from.min);
        let scaled = if to.bits >= from.bits {
            // Scale up
            unsigned.overflowing_shl(to.bits - from.bits).0
        } else if val == from.max {
            // Scaling down with the code below will cause overflow if we don't special-case this
            return to.max;
        } else {
            // Scale down with banker's rounding
            let shift = from.bits - to.bits;
            let half = 1 << (from.bits - to.bits - 1);
            if unsigned & (1 << shift) == 0 {
                unsigned.wrapping_add(half - 1).overflowing_shr(shift).0
            } else {
                unsigned.wrapping_add(half).overflowing_shr(shift).0
            }
        };
        scaled.wrapping_add(to.min)
    }
}

/// Scales `val` from the range `[min, max]` into `[-1.0, 1.0]`.
fn scale_to_f64(val: u32, min: u32, max: u32) -> f64 {
    let unsigned = val.wrapping_sub(min);
    let range = max.wrapping_sub(min) + 1;
    f64::from(unsigned) / f64::from(range / 2) - 1.0
}

/// Scales `val` from the range `[-1.0, 1.0]` into `[min, max]`.
fn scale_from_f64(val: f64, min: u32, max: u32) -> u32 {
    if !val.is_finite() {
        return 0;
    }
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

/// Internal trait used to scale PCM samples.
pub trait Scalable: Sized + Sealed {
    const INFO: TypeInfo;

    fn to_u64(self) -> u64;
    fn from_u64(val: u64) -> Self;

    fn to_f64(self) -> f64;
    fn from_f64(val: f64) -> Self;

    fn scale<T: Scalable>(self) -> T {
        if Self::INFO.is_float || T::INFO.is_float {
            T::from_f64(self.to_f64())
        } else {
            T::from_u64(scale(self.to_u64(), Self::INFO, T::INFO))
        }
    }
}

macro_rules! scalable {
    ($int:ty) => {
        #[allow(trivial_numeric_casts)]
        impl Scalable for $int {
            const INFO: TypeInfo = TypeInfo {
                is_float: false,
                min: <$int>::MIN as u64,
                max: <$int>::MAX as u64,
                bits: <$int>::BITS,
            };

            fn to_u64(self) -> u64 {
                self as u64
            }

            fn from_u64(val: u64) -> Self {
                val as Self
            }

            fn to_f64(self) -> f64 {
                if Self::INFO.max > (u32::MAX as u64) {
                    // Scale to u32 to avoid floating-point precision loss
                    scale_to_f64(self.scale(), u32::MIN, u32::MAX)
                } else {
                    scale_to_f64(self as u32, Self::INFO.min as u32, Self::INFO.max as u32)
                }
            }

            fn from_f64(val: f64) -> Self {
                if Self::INFO.max > (u32::MAX as u64) {
                    // Scale to u32 to avoid floating-point precision loss
                    scale_from_f64(val, u32::MIN, u32::MAX).scale()
                } else {
                    scale_from_f64(val, Self::INFO.min as u32, Self::INFO.max as u32) as Self
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
    const INFO: TypeInfo =
        TypeInfo { is_float: true, min: i64::MIN as u64, max: i64::MAX as u64, bits: 32 };

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
    const INFO: TypeInfo =
        TypeInfo { is_float: true, min: i64::MIN as u64, max: i64::MAX as u64, bits: 64 };

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
pub trait AnyPcm: DynamicFormat + Cast<AnyFormat> {}
impl<T: PcmFormat + Cast<AnyFormat>> AnyPcm for T {}
impl AnyPcm for AnyFormat {}

/// Wraps a stream of PCM samples and converts them to another PCM format as they are read. If the
/// samples are already in the target format, they will be passed through.
pub struct ConvertPcm<'r, 's: 'r, To>
where
    To: PcmFormat + Cast<AnyFormat>,
    To::Data: Scalable,
    AnyFormat: Cast<To>,
{
    inner: Box<dyn ReadSamples<'s, Format = AnyFormat> + 'r>,
    _marker: PhantomData<To>,
}

impl<'r, 's: 'r, To> ConvertPcm<'r, 's, To>
where
    To: PcmFormat + Cast<AnyFormat>,
    To::Data: Scalable,
    AnyFormat: Cast<To>,
{
    /// Creates a new converter which reads samples from `inner`.
    pub fn new<From: AnyPcm + 'r>(inner: impl ReadSamples<'s, Format = From> + 'r) -> Self {
        Self { inner: Box::new(CastSamples::new(inner)), _marker: PhantomData }
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
    To: PcmFormat + Cast<AnyFormat>,
    To::Data: Scalable,
    AnyFormat: Cast<To>,
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
    fn test_scale_up() {
        assert_eq!(i16::MIN.scale::<u64>(), u64::MIN);
        assert_eq!(i16::MAX.scale::<u64>(), 0xffff000000000000);

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
        assert_approx_eq!(f64, i8::MAX.scale::<f64>(), 127.0 / 128.0, ulps = 2);

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
        assert_eq!((127.0 / 128.0).scale::<i8>(), i8::MAX);

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

    #[test]
    fn test_pcms16le_to_pcms16be() -> Result<()> {
        let data = open_test_wav();
        let samples_le = Samples::<PcmS16Le>::from_pcm(&data, 2);
        let mut converter = ConvertPcm::<PcmS16Be>::new(samples_le.into_reader());
        let converted = converter.coalesce_samples()?;
        // The conversion should have been zero-cost
        assert!(matches!(converted.data, Cow::Borrowed(_)));
        assert!(converted.data == data);
        Ok(())
    }
}
