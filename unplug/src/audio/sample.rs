use super::format::{AnyContext, AnyFormat, Format, FormatTag, RawFormat, StaticFormat};
use super::{Error, Result};
use std::borrow::Cow;
use std::iter::FusedIterator;
use std::marker::PhantomData;

/// A block of audio sample data read from an audio source.
#[derive(Clone)]
pub struct Samples<'a, F: FormatTag> {
    /// The decoder context for the samples.
    pub context: F::Context,
    /// The address of the first unit to decode. This is in format dependent units; use
    /// `Format::address_to_byte()` and related methods to convert to and from byte offsets.
    pub start_address: usize,
    /// The address of the last unit to decode. This is in format dependent units; use
    /// `Format::address_to_byte()` and related methods to convert to and from byte offsets.
    pub end_address: usize,
    /// The number of channels in the data.
    pub channels: usize,
    /// The raw sample data.
    pub bytes: Cow<'a, [u8]>,
}

impl<'a, F: FormatTag> Samples<'a, F> {
    /// Gets the format of the sample data.
    pub fn format(&self) -> Format {
        F::format(&self.context)
    }

    /// Moves the samples into a reader which returns them.
    pub fn into_reader(self) -> ReadSamplesOnce<'a, F> {
        ReadSamplesOnce::new(self)
    }
}

impl<F: RawFormat> Samples<'_, F> {
    /// Returns an iterator over the slices for each sample.
    pub fn iter(&self) -> SampleIterator<'_> {
        SampleIterator::new(self)
    }
}

impl<'a, F: StaticFormat> Samples<'a, F> {
    /// Converts the samples into samples tagged with `AnyFormat`.
    pub fn into_any(self) -> Samples<'a, AnyFormat> {
        Samples::<AnyFormat> {
            context: AnyContext::new::<F>(self.context),
            start_address: self.start_address,
            end_address: self.end_address,
            channels: self.channels,
            bytes: self.bytes,
        }
    }
}

impl<'a> Samples<'a, AnyFormat> {
    /// Casts the `AnyFormat` sample into a concrete sample type. If the samples do not have the
    /// expected format, this will fail and the samples will be returned back uncasted.
    pub fn cast<F: StaticFormat>(mut self) -> SampleCastResult<'a, F> {
        if self.format() != F::format_static() {
            return Err(self);
        }
        match self.context.inner.downcast() {
            Ok(context) => Ok(Samples::<F> {
                context: *context,
                start_address: self.start_address,
                end_address: self.end_address,
                channels: self.channels,
                bytes: self.bytes,
            }),
            Err(context) => {
                self.context.inner = context;
                Err(self)
            }
        }
    }
}

type SampleCastResult<'a, F> = std::result::Result<Samples<'a, F>, Samples<'a, AnyFormat>>;

/// Trait for an audio source.
pub trait ReadSamples<'a> {
    /// The format that samples are decoded as.
    type Format: FormatTag;

    /// Reads the next block of samples. If there are no more samples, returns `Ok(None)`.
    fn read_samples(&mut self) -> Result<Option<Samples<'a, Self::Format>>>;
}

impl<'a, F: FormatTag> ReadSamples<'a> for Box<dyn ReadSamples<'a, Format = F> + '_> {
    type Format = F;
    fn read_samples(&mut self) -> Result<Option<Samples<'a, Self::Format>>> {
        (**self).read_samples()
    }
}

/// `ReadSamples` implementation which yields a single `Samples` struct.
pub struct ReadSamplesOnce<'a, F: FormatTag> {
    samples: Option<Samples<'a, F>>,
}

impl<'a, F: FormatTag> ReadSamplesOnce<'a, F> {
    pub fn new(samples: Samples<'a, F>) -> Self {
        Self { samples: Some(samples) }
    }
}

impl<'a, F: FormatTag> ReadSamples<'a> for ReadSamplesOnce<'a, F> {
    type Format = F;
    fn read_samples(&mut self) -> Result<Option<Samples<'a, Self::Format>>> {
        Ok(self.samples.take())
    }
}

/// A wrapper which casts audio samples to a particular type as they are read.
/// If a sample block is not of the expected type, this will stop with `Error::UnsupportedFormat`.
pub struct CastSamples<'a, From, To>
where
    From: ReadSamples<'a, Format = AnyFormat>,
    To: StaticFormat,
{
    inner: From,
    _marker: PhantomData<&'a To>,
}

impl<'a, From, To> CastSamples<'a, From, To>
where
    From: ReadSamples<'a, Format = AnyFormat>,
    To: StaticFormat,
{
    /// Creates a new `CastSamples` which casts samples from `inner`.
    pub fn new(inner: From) -> Self {
        Self { inner, _marker: PhantomData }
    }
}

impl<'a, From, To> ReadSamples<'a> for CastSamples<'a, From, To>
where
    From: ReadSamples<'a, Format = AnyFormat>,
    To: StaticFormat,
{
    type Format = To;
    fn read_samples(&mut self) -> Result<Option<Samples<'a, Self::Format>>> {
        match self.inner.read_samples() {
            Ok(Some(samples)) => match samples.cast() {
                Ok(samples) => Ok(Some(samples)),
                Err(samples) => Err(Error::UnsupportedFormat(samples.format())),
            },
            Ok(None) => Ok(None),
            Err(error) => Err(error),
        }
    }
}

/// An iterator over raw audio samples.
pub struct SampleIterator<'a> {
    bytes: &'a [u8],
    step: usize,
}

impl<'a> SampleIterator<'a> {
    /// Creates a `SampleIterator` which iterates over the data in `samples`.
    pub fn new<F: RawFormat>(samples: &'a Samples<'a, F>) -> Self {
        let step = F::sample_to_byte(1, samples.channels);
        assert!(step > 0);
        let start = F::address_to_byte(samples.start_address);
        let end = F::address_to_byte(samples.end_address + 1);
        Self { bytes: &samples.bytes[start..end], step }
    }
}

impl<'a> Iterator for SampleIterator<'a> {
    type Item = &'a [u8];
    fn next(&mut self) -> Option<Self::Item> {
        if self.bytes.len() >= self.step {
            let sample = &self.bytes[..self.step];
            self.bytes = &self.bytes[self.step..];
            Some(sample)
        } else {
            None
        }
    }
}

impl FusedIterator for SampleIterator<'_> {}

/// Joins two raw mono streams into a single stereo stream.
/// The streams must both come from the same source and have the same block sizes.
pub struct JoinChannels<'a, F, R>
where
    F: RawFormat,
    R: ReadSamples<'a, Format = F>,
{
    left: R,
    right: R,
    _marker: PhantomData<&'a F>,
}

impl<'a, F, R> JoinChannels<'a, F, R>
where
    F: RawFormat,
    R: ReadSamples<'a, Format = F>,
{
    pub fn new(left: R, right: R) -> Self {
        Self { left, right, _marker: PhantomData }
    }
}

impl<'a, F, R> ReadSamples<'static> for JoinChannels<'a, F, R>
where
    F: RawFormat,
    R: ReadSamples<'a, Format = F>,
{
    type Format = F;
    fn read_samples(&mut self) -> Result<Option<Samples<'static, Self::Format>>> {
        let left = self.left.read_samples()?;
        let right = self.right.read_samples()?;
        let (left, right) = match (left, right) {
            (Some(l), Some(r)) => (l, r),
            _ => return Ok(None),
        };
        if left.channels != 1 || right.channels != 1 {
            return Err(Error::StreamNotMono);
        }

        // TODO: Optimize?
        let mut merged = Vec::with_capacity(left.bytes.len() + right.bytes.len());
        for (l, r) in left.iter().zip(right.iter()) {
            merged.extend(l);
            merged.extend(r);
        }

        Ok(Some(Samples {
            context: (),
            start_address: 0,
            end_address: F::byte_to_address(merged.len() - 1),
            channels: 2,
            bytes: merged.into(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::format::{PcmS16Be, PcmS16Le};
    use std::convert::TryFrom;

    #[derive(Clone)]
    struct PcmS16LeContext;
    impl StaticFormat for PcmS16LeContext {
        type Context = i32;
        fn format_static() -> Format {
            Format::PcmS16Le
        }
    }

    #[test]
    fn test_any_without_context() {
        let bytes: Vec<u8> = (0..16).collect();
        let original = Samples::<PcmS16Le> {
            context: (),
            start_address: 2,
            end_address: 7,
            channels: 1,
            bytes: Cow::Borrowed(&bytes),
        };

        let any = original.clone().into_any();
        assert_eq!(any.format(), Format::PcmS16Le);
        assert_eq!(any.start_address, original.start_address);
        assert_eq!(any.end_address, original.end_address);
        assert_eq!(any.channels, original.channels);
        assert_eq!(any.bytes, original.bytes);

        // The contexts are the same, but this should fail because the formats differ
        let casted = any.cast::<PcmS16Be>();
        assert!(casted.is_err());
        let any = casted.err().unwrap();

        let casted = any.cast::<PcmS16Le>().ok().unwrap();
        assert_eq!(casted.format(), Format::PcmS16Le);
        assert_eq!(casted.start_address, original.start_address);
        assert_eq!(casted.end_address, original.end_address);
        assert_eq!(casted.channels, original.channels);
        assert_eq!(casted.bytes, original.bytes);
    }

    #[test]
    fn test_any_with_context() {
        let bytes: Vec<u8> = (0..16).collect();
        let original = Samples::<PcmS16LeContext> {
            context: 123,
            start_address: 2,
            end_address: 31,
            channels: 1,
            bytes: Cow::Borrowed(&bytes),
        };

        let any = original.clone().into_any();
        assert_eq!(any.format(), Format::PcmS16Le);
        assert_eq!(any.start_address, original.start_address);
        assert_eq!(any.end_address, original.end_address);
        assert_eq!(any.channels, original.channels);
        assert_eq!(any.bytes, original.bytes);

        // The formats are the same, but this should fail because the contexts differ
        let casted = any.cast::<PcmS16Le>();
        assert!(casted.is_err());
        let any = casted.err().unwrap();

        let casted = any.cast::<PcmS16LeContext>().ok().unwrap();
        assert_eq!(casted.format(), Format::PcmS16Le);
        assert_eq!(casted.context, original.context);
        assert_eq!(casted.start_address, original.start_address);
        assert_eq!(casted.end_address, original.end_address);
        assert_eq!(casted.channels, original.channels);
        assert_eq!(casted.bytes, original.bytes);
        assert!(matches!(casted.bytes, Cow::Borrowed(_)));
    }

    #[test]
    fn test_cast_samples() -> Result<()> {
        let bytes: Vec<u8> = (0..16).collect();
        let original = Samples::<PcmS16Le> {
            context: (),
            start_address: 2,
            end_address: 31,
            channels: 1,
            bytes: Cow::Borrowed(&bytes),
        };

        let any = original.clone().into_any().into_reader();
        let mut caster: CastSamples<'_, _, PcmS16Le> = CastSamples::new(any);
        let casted: Samples<'_, PcmS16Le> = caster.read_samples()?.unwrap();
        assert_eq!(casted.format(), Format::PcmS16Le);
        assert_eq!(casted.start_address, original.start_address);
        assert_eq!(casted.end_address, original.end_address);
        assert_eq!(casted.channels, original.channels);
        assert_eq!(casted.bytes, original.bytes);
        assert!(matches!(casted.bytes, Cow::Borrowed(_)));

        // Casting to PcmS16Be should fail with UnsupportedFormat
        let any = original.clone().into_any().into_reader();
        let mut caster: CastSamples<'_, _, PcmS16Be> = CastSamples::new(any);
        assert!(matches!(caster.read_samples(), Err(Error::UnsupportedFormat(Format::PcmS16Le))));

        Ok(())
    }

    #[test]
    fn test_sample_iterator_mono() {
        let bytes: Vec<u8> = (0..16).collect();
        let samples = Samples::<PcmS16Le> {
            context: (),
            start_address: 1,
            end_address: 6,
            channels: 1,
            bytes: Cow::Borrowed(&bytes),
        };
        let iterated: Vec<_> =
            samples.iter().map(|s| <[u8; 2]>::try_from(s).ok().unwrap()).collect();
        assert_eq!(
            iterated,
            &[[0x2, 0x3], [0x4, 0x5], [0x6, 0x7], [0x8, 0x9], [0xa, 0xb], [0xc, 0xd]]
        );
    }

    #[test]
    fn test_sample_iterator_stereo() {
        let bytes: Vec<u8> = (0..16).collect();
        let samples = Samples::<PcmS16Le> {
            context: (),
            start_address: 1,
            end_address: 6,
            channels: 2,
            bytes: Cow::Borrowed(&bytes),
        };
        let iterated: Vec<_> =
            samples.iter().map(|s| <[u8; 4]>::try_from(s).ok().unwrap()).collect();
        assert_eq!(iterated, &[[0x2, 0x3, 0x4, 0x5], [0x6, 0x7, 0x8, 0x9], [0xa, 0xb, 0xc, 0xd]]);
    }

    #[test]
    fn test_join_channels() -> Result<()> {
        let lbytes: Vec<u8> = (0..32).step_by(2).collect();
        let rbytes: Vec<u8> = (0..32).skip(1).step_by(2).collect();
        let left = Samples::<PcmS16Le> {
            context: (),
            start_address: 1,
            end_address: 6,
            channels: 1,
            bytes: Cow::Borrowed(&lbytes),
        };
        let right = Samples::<PcmS16Le> {
            context: (),
            start_address: 1,
            end_address: 6,
            channels: 1,
            bytes: Cow::Borrowed(&rbytes),
        };

        let mut joiner = JoinChannels::new(left.into_reader(), right.into_reader());
        let joined = joiner.read_samples()?.unwrap();
        assert_eq!(joined.start_address, 0);
        assert_eq!(joined.end_address, 11);
        assert_eq!(joined.channels, 2);
        assert_eq!(
            joined.bytes.as_ref(),
            &[
                0x4, 0x6, 0x5, 0x7, 0x8, 0xa, 0x9, 0xb, 0xc, 0xe, 0xd, 0xf, 0x10, 0x12, 0x11, 0x13,
                0x14, 0x16, 0x15, 0x17, 0x18, 0x1a, 0x19, 0x1b,
            ]
        );

        Ok(())
    }
}
