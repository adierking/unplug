use super::format::{AnyFormat, AnyParams, Format, FormatTag, RawFormat, StaticFormat};
use super::{Error, Result};
use std::borrow::Cow;
use std::collections::VecDeque;
use std::iter::FusedIterator;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

/// A block of audio sample data read from an audio source.
#[derive(Clone)]
pub struct Samples<'a, F: FormatTag> {
    /// The decoder parameters for the samples.
    pub params: F::Params,
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
        F::format(&self.params)
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
            params: AnyParams::new::<F>(self.params),
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
        match self.params.inner.downcast() {
            Ok(params) => Ok(Samples::<F> {
                params: *params,
                start_address: self.start_address,
                end_address: self.end_address,
                channels: self.channels,
                bytes: self.bytes,
            }),
            Err(params) => {
                self.params.inner = params;
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
            params: (),
            start_address: 0,
            end_address: F::byte_to_address(merged.len() - 1),
            channels: 2,
            bytes: merged.into(),
        }))
    }
}

/// Splits a stereo stream into two mono streams.
pub struct SplitChannels<'a, F>
where
    F: RawFormat + 'static,
{
    state: Arc<Mutex<SplitChannelsState<'a, F>>>,
}

impl<'a, F> SplitChannels<'a, F>
where
    F: RawFormat + 'static,
{
    /// Creates a new `SplitChannels` which reads samples from `reader`.
    pub fn new(reader: impl ReadSamples<'a, Format = F> + 'a) -> Self {
        Self { state: SplitChannelsState::new(Box::from(reader)) }
    }

    /// Returns a thread-safe reader over the samples in the left channel.
    pub fn left(&self) -> SplitChannelsReader<'a, F> {
        SplitChannelsReader { state: Arc::clone(&self.state), is_right: false }
    }

    /// Returns a thread-safe reader over the samples in the right channel.
    pub fn right(&self) -> SplitChannelsReader<'a, F> {
        SplitChannelsReader { state: Arc::clone(&self.state), is_right: true }
    }
}

/// State for `SplitChannels` which is shared across readers.
struct SplitChannelsState<'a, F>
where
    F: RawFormat + 'static,
{
    /// The inner reader to read new samples from.
    reader: Box<dyn ReadSamples<'a, Format = F> + 'a>,
    /// Samples which have not yet been processed by the left reader.
    left: VecDeque<Rc<Samples<'a, F>>>,
    /// Samples which have not yet been processed by the right reader.
    right: VecDeque<Rc<Samples<'a, F>>>,
}

impl<'a, F> SplitChannelsState<'a, F>
where
    F: RawFormat + 'static,
{
    /// Creates a new `SplitChannelsState` wrapping `reader`.
    fn new(reader: Box<dyn ReadSamples<'a, Format = F> + 'a>) -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self { reader, left: VecDeque::new(), right: VecDeque::new() }))
    }

    /// Reads more samples from the inner reader and updates the queues. Returns `Ok(true)` if
    /// samples were read and `Ok(false)` if no more samples are available.
    fn read_next(&mut self) -> Result<bool> {
        let samples = match self.reader.read_samples()? {
            Some(s) => Rc::new(s),
            None => return Ok(false),
        };
        self.left.push_back(Rc::clone(&samples));
        self.right.push_back(samples);
        Ok(true)
    }
}

/// `ReadSamples` implementation for a single channel returned by a `SplitChannels`.
pub struct SplitChannelsReader<'a, F>
where
    F: RawFormat + 'static,
{
    state: Arc<Mutex<SplitChannelsState<'a, F>>>,
    is_right: bool,
}

impl<'a, F> ReadSamples<'a> for SplitChannelsReader<'a, F>
where
    F: RawFormat + 'static,
{
    type Format = F;
    fn read_samples(&mut self) -> Result<Option<Samples<'a, Self::Format>>> {
        // We assume here that the channels can be read from in any order and that we are only
        // expected to read as much as we need. Each channel must hold onto samples it hasn't
        // returned yet, and we also shouldn't read every sample from the inner reader all at once.
        // The basic idea here is that we share samples across both channels using queues with
        // refcounted sample data. When we try to read from an empty queue, we read more samples and
        // push them onto both queues.
        let mut state = self.state.lock().unwrap();
        let samples = loop {
            let queue = if self.is_right { &mut state.right } else { &mut state.left };
            if let Some(s) = queue.pop_front() {
                break s;
            } else if !state.read_next()? {
                return Ok(None);
            }
        };
        if samples.channels != 2 {
            return Err(Error::StreamNotStereo);
        }

        let mut channel_bytes = Vec::with_capacity(samples.bytes.len() / 2);
        for sample in samples.iter() {
            let (left_sample, right_sample) = sample.split_at(sample.len() / 2);
            channel_bytes.extend(if self.is_right { right_sample } else { left_sample });
        }

        Ok(Some(Samples::<'static, F> {
            params: (),
            start_address: 0,
            end_address: F::byte_to_address(channel_bytes.len() - 1),
            channels: 1,
            bytes: channel_bytes.into(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::format::{PcmS16Be, PcmS16Le};
    use std::convert::TryFrom;

    #[derive(Clone)]
    struct PcmS16LeParams;
    impl StaticFormat for PcmS16LeParams {
        type Params = i32;
        fn format_static() -> Format {
            Format::PcmS16Le
        }
    }

    #[test]
    fn test_any_without_context() {
        let bytes: Vec<u8> = (0..16).collect();
        let original = Samples::<PcmS16Le> {
            params: (),
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

        // The parameters are the same, but this should fail because the formats differ
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
    fn test_any_with_params() {
        let bytes: Vec<u8> = (0..16).collect();
        let original = Samples::<PcmS16LeParams> {
            params: 123,
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

        // The formats are the same, but this should fail because the parameters differ
        let casted = any.cast::<PcmS16Le>();
        assert!(casted.is_err());
        let any = casted.err().unwrap();

        let casted = any.cast::<PcmS16LeParams>().ok().unwrap();
        assert_eq!(casted.format(), Format::PcmS16Le);
        assert_eq!(casted.params, original.params);
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
            params: (),
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
            params: (),
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
            params: (),
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
            params: (),
            start_address: 1,
            end_address: 6,
            channels: 1,
            bytes: Cow::Borrowed(&lbytes),
        };
        let right = Samples::<PcmS16Le> {
            params: (),
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

    #[test]
    fn test_split_channels() -> Result<()> {
        let bytes: Vec<u8> = (0..16).collect();
        let stereo = Samples::<PcmS16Le> {
            params: (),
            start_address: 2,
            end_address: 5,
            channels: 2,
            bytes: Cow::Borrowed(&bytes),
        };

        let splitter = SplitChannels::new(stereo.into_reader());
        let mut left_split = splitter.left();
        let mut right_split = splitter.right();
        let left = left_split.read_samples()?.unwrap();
        let right = right_split.read_samples()?.unwrap();
        assert!(left_split.read_samples()?.is_none());
        assert!(right_split.read_samples()?.is_none());

        assert_eq!(left.start_address, 0);
        assert_eq!(left.end_address, 1);
        assert_eq!(left.channels, 1);
        assert_eq!(left.bytes.as_ref(), &[0x4, 0x5, 0x8, 0x9,]);

        assert_eq!(right.start_address, 0);
        assert_eq!(right.end_address, 1);
        assert_eq!(right.channels, 1);
        assert_eq!(right.bytes.as_ref(), &[0x6, 0x7, 0xa, 0xb,]);

        Ok(())
    }
}
