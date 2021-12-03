use super::adpcm;
use super::pcm::{ConvertPcm, Scalable};
use super::{
    AnyFormat, Cast, DynamicFormat, Format, GcAdpcm, PcmF32Le, PcmFormat, PcmS16Be, PcmS16Le,
    PcmS24Le, PcmS32Le, PcmS8, StaticFormat,
};
use crate::audio::ReadSamples;

mod private {
    pub trait Sealed {}
    impl Sealed for super::PcmS8 {}
    impl Sealed for super::PcmS16Le {}
    impl Sealed for super::PcmS16Be {}
    impl Sealed for super::PcmS24Le {}
    impl Sealed for super::PcmS32Le {}
    impl Sealed for super::PcmF32Le {}
    impl Sealed for super::GcAdpcm {}
    impl Sealed for super::AnyFormat {}
}
use private::*;

/// Convenience type for a boxed `ReadSamples` trait object.
type DynReader<'r, 's, F> = Box<dyn ReadSamples<'s, Format = F> + 'r>;

/// Trait for a format that can be converted to another format.
pub trait Convert<To: DynamicFormat>: DynamicFormat + Sealed {
    /// Returns a reader which reads and converts samples from `reader`.
    fn convert<'r, 's: 'r>(reader: DynReader<'r, 's, Self>) -> DynReader<'r, 's, To>;
}

// PCM -> PCM
impl<From, To> Convert<To> for From
where
    From: PcmFormat + Cast<AnyFormat> + Sealed,
    To: PcmFormat,
    To::Data: Scalable,
    AnyFormat: Cast<To>,
{
    fn convert<'r, 's: 'r>(reader: DynReader<'r, 's, Self>) -> DynReader<'r, 's, To> {
        Box::from(ConvertPcm::new(reader))
    }
}

// PCM -> ADPCM
impl<From> Convert<GcAdpcm> for From
where
    From: PcmFormat + Cast<AnyFormat> + Sealed,
{
    fn convert<'r, 's: 'r>(reader: DynReader<'r, 's, Self>) -> DynReader<'r, 's, GcAdpcm> {
        // TODO: Better error handling?
        let pcm16 = ConvertPcm::new(reader);
        let (left, right) = adpcm::EncoderBuilder::simple(pcm16).expect("ADPCM encoding failed");
        assert!(right.is_none());
        Box::from(left)
    }
}

// ADPCM -> PCM
impl<To> Convert<To> for GcAdpcm
where
    To: PcmFormat,
    To::Data: Scalable,
    AnyFormat: Cast<To>,
{
    fn convert<'r, 's: 'r>(reader: DynReader<'r, 's, Self>) -> DynReader<'r, 's, To> {
        Box::from(ConvertPcm::new(adpcm::Decoder::new(reader)))
    }
}

// ADPCM -> ADPCM
impl Convert<GcAdpcm> for GcAdpcm {
    fn convert<'r, 's: 'r>(reader: DynReader<'r, 's, Self>) -> DynReader<'r, 's, GcAdpcm> {
        reader
    }
}

// Any -> StaticFormat
impl<To> Convert<To> for AnyFormat
where
    To: StaticFormat,
    PcmS8: Convert<To>,
    PcmS16Le: Convert<To>,
    PcmS16Be: Convert<To>,
    PcmS24Le: Convert<To>,
    PcmS32Le: Convert<To>,
    PcmF32Le: Convert<To>,
    GcAdpcm: Convert<To>,
{
    fn convert<'r, 's: 'r>(reader: DynReader<'r, 's, Self>) -> DynReader<'r, 's, To> {
        match reader.format() {
            Format::PcmS8 => PcmS8::convert(Box::from(reader.cast())),
            Format::PcmS16Le => PcmS16Le::convert(Box::from(reader.cast())),
            Format::PcmS16Be => PcmS16Be::convert(Box::from(reader.cast())),
            Format::PcmS24Le => PcmS24Le::convert(Box::from(reader.cast())),
            Format::PcmS32Le => PcmS32Le::convert(Box::from(reader.cast())),
            Format::PcmF32Le => PcmF32Le::convert(Box::from(reader.cast())),
            Format::GcAdpcm => GcAdpcm::convert(Box::from(reader.cast())),
        }
    }
}

// DynamicFormat -> Any
impl<From> Convert<AnyFormat> for From
where
    From: DynamicFormat + Sealed + Cast<AnyFormat>,
{
    fn convert<'r, 's: 'r>(reader: DynReader<'r, 's, Self>) -> DynReader<'r, 's, AnyFormat> {
        Box::from(reader.cast())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::format::ReadWriteBytes;
    use crate::audio::{Result, Samples};
    use crate::test::{
        open_test_wav, TEST_WAV_LEFT_DSP, TEST_WAV_RIGHT_DSP, TEST_WAV_S32,
        TEST_WAV_S32_DATA_OFFSET,
    };

    fn wrap<'r, 's: 'r, R, From, To>(reader: R) -> DynReader<'r, 's, To>
    where
        R: ReadSamples<'s, Format = From> + 'r,
        From: Convert<To>,
        To: StaticFormat,
    {
        From::convert(Box::from(reader))
    }

    #[test]
    fn test_convert_pcm_to_pcm() -> Result<()> {
        let samples_s16 = Samples::<PcmS16Le>::from_pcm(open_test_wav(), 2, 44100);
        let mut converter = wrap::<_, _, PcmS32Le>(samples_s16.into_reader("test"));
        let samples_s32 = converter.read_all_samples()?;
        let expected = PcmS32Le::read_bytes(&TEST_WAV_S32[TEST_WAV_S32_DATA_OFFSET..])?;
        assert!(samples_s32.data == expected);
        Ok(())
    }

    #[test]
    fn test_convert_pcm_to_adpcm() -> Result<()> {
        let samples = Samples::<PcmS16Le>::from_pcm(open_test_wav(), 2, 44100);
        let split = samples.into_reader("test").split_channels();
        let mut left_encoder = wrap::<_, _, GcAdpcm>(split.left());
        let mut right_encoder = wrap::<_, _, GcAdpcm>(split.right());
        let left = left_encoder.read_samples()?.unwrap();
        let right = right_encoder.read_samples()?.unwrap();
        assert!(left.data == TEST_WAV_LEFT_DSP);
        assert!(right.data == TEST_WAV_RIGHT_DSP);
        Ok(())
    }

    #[test]
    fn test_convert_any_to_adpcm() -> Result<()> {
        let samples = Samples::<PcmS16Le>::from_pcm(open_test_wav(), 2, 44100);
        let split = samples.into_reader("test").split_channels();
        let mut left_encoder = wrap::<_, AnyFormat, GcAdpcm>(split.left().cast());
        let mut right_encoder = wrap::<_, AnyFormat, GcAdpcm>(split.right().cast());
        let left = left_encoder.read_samples()?.unwrap();
        let right = right_encoder.read_samples()?.unwrap();
        assert!(left.data == TEST_WAV_LEFT_DSP);
        assert!(right.data == TEST_WAV_RIGHT_DSP);
        Ok(())
    }
}
