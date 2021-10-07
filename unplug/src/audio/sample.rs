use super::format::{AnyData, AnyFormat, AnyParams, Format, FormatTag, RawFormat, StaticFormat};
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
    /// The codec parameters for the samples.
    pub params: F::Params,
    /// The address of the last unit to decode. This is in format dependent units; use
    /// `Format::address_to_index()` and related methods to convert to and from data indexes.
    pub end_address: usize,
    /// The number of channels in the data.
    pub channels: usize,
    /// The raw sample data. This is not necessarily a list of samples; for example, GameCube ADPCM
    /// has packed samples which need to be decoded first.
    pub data: Cow<'a, [F::Data]>,
}

impl<'a, F: FormatTag> Samples<'a, F> {
    /// Gets the format of the sample data.
    pub fn format(&self) -> Format {
        F::format(&self.params)
    }

    /// Moves the samples into a reader which returns them.
    pub fn into_reader(self) -> ReadSampleList<'a, F> {
        ReadSampleList::new(vec![self])
    }
}

impl<F: RawFormat> Samples<'_, F> {
    /// Returns an iterator over the slices for each sample.
    pub fn iter(&self) -> SampleIterator<'_, F> {
        SampleIterator::new(self)
    }
}

impl<'a, F: StaticFormat> Samples<'a, F> {
    /// Converts the samples into samples tagged with `AnyFormat`.
    pub fn into_any(self) -> Samples<'a, AnyFormat> {
        Samples {
            params: AnyParams::new::<F>(self.params),
            end_address: self.end_address,
            channels: self.channels,
            data: AnyData::from_data(self.data),
        }
    }

    /// Appends the samples in `other` to `self`. Both sample objects must be aligned on a frame
    /// boundary and share compatible codec parameters. On success, the sample data will become
    /// owned by `self`.
    pub fn append(&mut self, other: &Samples<'_, F>) -> Result<()> {
        let format = self.format();
        assert_eq!(format, other.format());

        if self.channels != other.channels {
            return Err(if self.channels == 1 {
                Error::StreamNotMono
            } else {
                Error::StreamNotStereo
            });
        }

        // Make sure the end of our data is frame-aligned
        if format.frame_address(self.end_address + 1) != self.end_address + 1 {
            return Err(Error::NotFrameAligned);
        }

        // Our end address must be at the end of the sample data or else we cannot append
        let end_index = format.address_to_index(self.end_address);
        let next_index = format.address_to_index(self.end_address + 1);
        if next_index != self.data.len() || next_index <= end_index {
            return Err(Error::NotFrameAligned);
        }

        F::append(&mut self.data, &mut self.params, &other.data, &other.params)?;
        self.end_address += other.end_address + 1;
        Ok(())
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
                end_address: self.end_address,
                channels: self.channels,
                data: AnyData::to_data(self.data),
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

    /// Reads all available samples and appends them into a single `Samples` object. The samples
    /// must have a static format and follow the rules for `Samples::append()`. If no samples are
    /// available, `Err(NoSamplesAvailable)` is returned.
    fn coalesce_samples(&mut self) -> Result<Samples<'a, Self::Format>>
    where
        Self::Format: StaticFormat,
    {
        let mut result: Option<Samples<'a, Self::Format>> = None;
        while let Some(samples) = self.read_samples()? {
            match &mut result {
                Some(a) => a.append(&samples)?,
                None => result = Some(samples),
            }
        }
        result.ok_or(Error::NoSamplesAvailable)
    }
}

impl<'a, F, R> ReadSamples<'a> for Box<R>
where
    F: FormatTag,
    R: ReadSamples<'a, Format = F> + ?Sized,
{
    type Format = F;
    fn read_samples(&mut self) -> Result<Option<Samples<'a, Self::Format>>> {
        (**self).read_samples()
    }
}

/// `ReadSamples` implementation which yields `Samples` structs from a queue.
pub struct ReadSampleList<'s, F: FormatTag> {
    samples: VecDeque<Samples<'s, F>>,
}

impl<'s, F: FormatTag> ReadSampleList<'s, F> {
    pub fn new(samples: impl IntoIterator<Item = Samples<'s, F>>) -> Self {
        Self { samples: samples.into_iter().collect() }
    }
}

impl<'s, F: FormatTag> ReadSamples<'s> for ReadSampleList<'s, F> {
    type Format = F;
    fn read_samples(&mut self) -> Result<Option<Samples<'s, Self::Format>>> {
        Ok(self.samples.pop_front())
    }
}

/// A wrapper which casts audio samples to a particular type as they are read.
/// If a sample block is not of the expected type, this will stop with `Error::UnsupportedFormat`.
pub struct CastSamples<'r, 's, F: StaticFormat> {
    inner: Box<dyn ReadSamples<'s, Format = AnyFormat> + 'r>,
    _marker: PhantomData<&'s F>,
}

impl<'r, 's, F: StaticFormat> CastSamples<'r, 's, F> {
    /// Creates a new `CastSamples` which casts samples from `inner`.
    pub fn new(inner: impl ReadSamples<'s, Format = AnyFormat> + 'r) -> Self {
        Self { inner: Box::from(inner), _marker: PhantomData }
    }
}

impl<'s, F: StaticFormat> ReadSamples<'s> for CastSamples<'_, 's, F> {
    type Format = F;
    fn read_samples(&mut self) -> Result<Option<Samples<'s, Self::Format>>> {
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
pub struct SampleIterator<'a, F: RawFormat> {
    samples: &'a [F::Data],
    step: usize,
}

impl<'a, F: RawFormat> SampleIterator<'a, F> {
    /// Creates a `SampleIterator` which iterates over the data in `samples`.
    pub fn new(samples: &'a Samples<'a, F>) -> Self {
        let end = samples.end_address + 1;
        Self { samples: &samples.data[..end], step: samples.channels }
    }
}

impl<'a, F: RawFormat> Iterator for SampleIterator<'a, F> {
    type Item = &'a [F::Data];
    fn next(&mut self) -> Option<Self::Item> {
        if self.samples.len() >= self.step {
            let sample = &self.samples[..self.step];
            self.samples = &self.samples[self.step..];
            Some(sample)
        } else {
            None
        }
    }
}

impl<F: RawFormat> FusedIterator for SampleIterator<'_, F> {}

/// Joins two raw mono streams into a single stereo stream.
/// The streams must both come from the same source and have the same block sizes.
pub struct JoinChannels<'r, 's, F: RawFormat> {
    left: Box<dyn ReadSamples<'s, Format = F> + 'r>,
    right: Box<dyn ReadSamples<'s, Format = F> + 'r>,
    _marker: PhantomData<F>,
}

impl<'r, 's, F: RawFormat> JoinChannels<'r, 's, F> {
    pub fn new<R>(left: R, right: R) -> Self
    where
        R: ReadSamples<'s, Format = F> + 'r,
    {
        Self { left: Box::from(left), right: Box::from(right), _marker: PhantomData }
    }
}

impl<F: RawFormat> ReadSamples<'static> for JoinChannels<'_, '_, F> {
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
        let mut merged = Vec::with_capacity(left.data.len() + right.data.len());
        let left_end = left.end_address + 1;
        let right_end = right.end_address + 1;
        for (&l, &r) in left.data.iter().take(left_end).zip(&right.data[..right_end]) {
            merged.push(l);
            merged.push(r);
        }

        Ok(Some(Samples {
            params: (),
            end_address: merged.len() - 1,
            channels: 2,
            data: merged.into(),
        }))
    }
}

/// Splits a stereo stream into two mono streams.
pub struct SplitChannels<'r, 's, F>
where
    F: RawFormat + 'static,
{
    state: Arc<Mutex<SplitChannelsState<'r, 's, F>>>,
}

impl<'r, 's, F> SplitChannels<'r, 's, F>
where
    F: RawFormat + 'static,
{
    /// Creates a new `SplitChannels` which reads samples from `reader`.
    pub fn new(reader: impl ReadSamples<'s, Format = F> + 'r) -> Self {
        Self { state: SplitChannelsState::new(Box::from(reader)) }
    }

    /// Returns a thread-safe reader over the samples in the left channel.
    pub fn left(&self) -> SplitChannelsReader<'r, 's, F> {
        SplitChannelsReader { state: Arc::clone(&self.state), is_right: false }
    }

    /// Returns a thread-safe reader over the samples in the right channel.
    pub fn right(&self) -> SplitChannelsReader<'r, 's, F> {
        SplitChannelsReader { state: Arc::clone(&self.state), is_right: true }
    }
}

/// State for `SplitChannels` which is shared across readers.
struct SplitChannelsState<'r, 's, F>
where
    F: RawFormat + 'static,
{
    /// The inner reader to read new samples from.
    reader: Box<dyn ReadSamples<'s, Format = F> + 'r>,
    /// Samples which have not yet been processed by the left reader.
    left: VecDeque<Rc<Samples<'s, F>>>,
    /// Samples which have not yet been processed by the right reader.
    right: VecDeque<Rc<Samples<'s, F>>>,
}

impl<'r, 's, F> SplitChannelsState<'r, 's, F>
where
    F: RawFormat + 'static,
{
    /// Creates a new `SplitChannelsState` wrapping `reader`.
    fn new(reader: Box<dyn ReadSamples<'s, Format = F> + 'r>) -> Arc<Mutex<Self>> {
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
pub struct SplitChannelsReader<'r, 's, F>
where
    F: RawFormat + 'static,
{
    state: Arc<Mutex<SplitChannelsState<'r, 's, F>>>,
    is_right: bool,
}

impl<F> ReadSamples<'static> for SplitChannelsReader<'_, '_, F>
where
    F: RawFormat + 'static,
{
    type Format = F;
    fn read_samples(&mut self) -> Result<Option<Samples<'static, Self::Format>>> {
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

        let mut channel_data = Vec::with_capacity(samples.data.len() / 2);
        for sample in samples.iter() {
            let (left_sample, right_sample) = (sample[0], sample[1]);
            channel_data.push(if self.is_right { right_sample } else { left_sample });
        }

        Ok(Some(Samples::<'static, F> {
            params: (),
            end_address: channel_data.len() - 1,
            channels: 1,
            data: channel_data.into(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::adpcm::GcAdpcm;
    use crate::audio::format::{PcmS16Be, PcmS16Le};
    use std::convert::TryFrom;
    use std::io::{Read, Write};

    #[derive(Clone)]
    struct PcmS16LeParams;
    impl StaticFormat for PcmS16LeParams {
        type Data = i16;
        type Params = i32;
        fn format_static() -> Format {
            Format::PcmS16Le
        }
        fn write_bytes(writer: impl Write, data: &[Self::Data]) -> Result<()> {
            PcmS16Le::write_bytes(writer, data)
        }
        fn read_bytes(reader: impl Read) -> Result<Vec<Self::Data>> {
            PcmS16Le::read_bytes(reader)
        }
        fn append(
            dest: &mut Cow<'_, [i16]>,
            _dest_params: &mut Self::Params,
            src: &[i16],
            _src_params: &Self::Params,
        ) -> Result<()> {
            dest.to_mut().extend(src);
            Ok(())
        }
    }

    #[test]
    fn test_any_without_context() {
        let samples: Vec<i16> = (0..16).collect();
        let original =
            Samples::<PcmS16Le> { params: (), end_address: 7, channels: 1, data: samples.into() };

        let any = original.clone().into_any();
        assert_eq!(any.format(), Format::PcmS16Le);
        assert_eq!(any.end_address, original.end_address);
        assert_eq!(any.channels, original.channels);

        // The parameters are the same, but this should fail because the formats differ
        let casted = any.cast::<PcmS16Be>();
        assert!(casted.is_err());
        let any = casted.err().unwrap();

        let casted = any.cast::<PcmS16Le>().ok().unwrap();
        assert_eq!(casted.format(), Format::PcmS16Le);
        assert_eq!(casted.end_address, original.end_address);
        assert_eq!(casted.channels, original.channels);
        assert_eq!(casted.data, original.data);
        assert!(matches!(casted.data, Cow::Owned(_)));
    }

    #[test]
    fn test_any_with_params() {
        let samples: Vec<i16> = (0..16).collect();
        let original = Samples::<PcmS16LeParams> {
            params: 123,
            end_address: 31,
            channels: 1,
            data: Cow::Borrowed(&samples),
        };

        let any = original.clone().into_any();
        assert_eq!(any.format(), Format::PcmS16Le);
        assert_eq!(any.end_address, original.end_address);
        assert_eq!(any.channels, original.channels);

        // The formats are the same, but this should fail because the parameters differ
        let casted = any.cast::<PcmS16Le>();
        assert!(casted.is_err());
        let any = casted.err().unwrap();

        let casted = any.cast::<PcmS16LeParams>().ok().unwrap();
        assert_eq!(casted.format(), Format::PcmS16Le);
        assert_eq!(casted.params, original.params);
        assert_eq!(casted.end_address, original.end_address);
        assert_eq!(casted.channels, original.channels);
        assert_eq!(casted.data, original.data);
        assert!(matches!(casted.data, Cow::Borrowed(_)));
    }

    #[test]
    fn test_cast_samples() -> Result<()> {
        let samples: Vec<i16> = (0..16).collect();
        let original = Samples::<PcmS16Le> {
            params: (),
            end_address: 31,
            channels: 1,
            data: Cow::Borrowed(&samples),
        };

        let any = original.clone().into_any().into_reader();
        let mut caster = CastSamples::new(any);
        let casted: Samples<'_, PcmS16Le> = caster.read_samples()?.unwrap();
        assert_eq!(casted.format(), Format::PcmS16Le);
        assert_eq!(casted.end_address, original.end_address);
        assert_eq!(casted.channels, original.channels);
        assert_eq!(casted.data, original.data);
        assert!(matches!(casted.data, Cow::Borrowed(_)));

        // Casting to PcmS16Be should fail with UnsupportedFormat
        let any = original.clone().into_any().into_reader();
        let mut caster: CastSamples<'_, '_, PcmS16Be> = CastSamples::new(any);
        assert!(matches!(caster.read_samples(), Err(Error::UnsupportedFormat(Format::PcmS16Le))));

        Ok(())
    }

    #[test]
    fn test_append_samples() {
        let mut samples1 = Samples::<GcAdpcm> {
            params: Default::default(),
            end_address: 0x1f,
            channels: 1,
            data: Cow::from((0..16).collect::<Vec<_>>()),
        };
        let samples2 = Samples::<GcAdpcm> {
            params: Default::default(),
            end_address: 0x1f,
            channels: 1,
            data: Cow::from((16..32).collect::<Vec<_>>()),
        };
        samples1.append(&samples2).unwrap();
        assert_eq!(samples1.end_address, 0x3f);
        assert_eq!(samples1.data, (0..32).collect::<Vec<_>>());
    }

    #[test]
    fn test_append_samples_partial() {
        let mut samples1 = Samples::<GcAdpcm> {
            params: Default::default(),
            end_address: 0x1f,
            channels: 1,
            data: Cow::from((0..16).collect::<Vec<_>>()),
        };
        let samples2 = Samples::<GcAdpcm> {
            params: Default::default(),
            end_address: 0x1f,
            channels: 1,
            data: Cow::from((16..48).collect::<Vec<_>>()),
        };
        samples1.append(&samples2).unwrap();
        assert_eq!(samples1.end_address, 0x3f);
        assert_eq!(samples1.data, (0..48).collect::<Vec<_>>());
    }

    #[test]
    fn test_append_samples_channel_mismatch() {
        let mut samples1 = Samples::<GcAdpcm> {
            params: Default::default(),
            end_address: 0x1f,
            channels: 1,
            data: Cow::from((0..16).collect::<Vec<_>>()),
        };
        let mut samples2 = Samples::<GcAdpcm> {
            params: Default::default(),
            end_address: 0x1f,
            channels: 2,
            data: Cow::from((16..32).collect::<Vec<_>>()),
        };
        assert!(matches!(samples1.append(&samples2), Err(Error::StreamNotMono)));
        assert!(matches!(samples2.append(&samples1), Err(Error::StreamNotStereo)));
    }

    #[test]
    fn test_append_samples_unaligned() {
        let mut samples1 = Samples::<GcAdpcm> {
            params: Default::default(),
            end_address: 0x1e,
            channels: 1,
            data: Cow::from((0..16).collect::<Vec<_>>()),
        };
        let samples2 = Samples::<GcAdpcm> {
            params: Default::default(),
            end_address: 0x1f,
            channels: 1,
            data: Cow::from((16..32).collect::<Vec<_>>()),
        };
        assert!(matches!(samples1.append(&samples2), Err(Error::NotFrameAligned)));
    }

    #[test]
    fn test_append_samples_before_end() {
        let mut samples1 = Samples::<GcAdpcm> {
            params: Default::default(),
            end_address: 0x1f,
            channels: 1,
            data: Cow::from((0..17).collect::<Vec<_>>()),
        };
        let samples2 = Samples::<GcAdpcm> {
            params: Default::default(),
            end_address: 0x1f,
            channels: 1,
            data: Cow::from((16..32).collect::<Vec<_>>()),
        };
        assert!(matches!(samples1.append(&samples2), Err(Error::NotFrameAligned)));
    }

    #[test]
    fn test_sample_iterator_mono() {
        let data: Vec<i16> = (0..8).collect();
        let samples = Samples::<PcmS16Le> {
            params: (),
            end_address: 6,
            channels: 1,
            data: Cow::Borrowed(&data),
        };
        let iterated: Vec<_> =
            samples.iter().map(|s| <[i16; 1]>::try_from(s).ok().unwrap()).collect();
        assert_eq!(iterated, &[[0x0], [0x1], [0x2], [0x3], [0x4], [0x5], [0x6]]);
    }

    #[test]
    fn test_sample_iterator_stereo() {
        let data: Vec<i16> = (0..8).collect();
        let samples = Samples::<PcmS16Le> {
            params: (),
            end_address: 5,
            channels: 2,
            data: Cow::Borrowed(&data),
        };
        let iterated: Vec<_> =
            samples.iter().map(|s| <[i16; 2]>::try_from(s).ok().unwrap()).collect();
        assert_eq!(iterated, &[[0x0, 0x1], [0x2, 0x3], [0x4, 0x5]]);
    }

    #[test]
    fn test_join_channels() -> Result<()> {
        let ldata: Vec<i16> = (0..16).step_by(2).collect();
        let rdata: Vec<i16> = (0..16).skip(1).step_by(2).collect();
        let left = Samples::<PcmS16Le> {
            params: (),
            end_address: 6,
            channels: 1,
            data: Cow::Borrowed(&ldata),
        };
        let right = Samples::<PcmS16Le> {
            params: (),
            end_address: 6,
            channels: 1,
            data: Cow::Borrowed(&rdata),
        };

        let mut joiner = JoinChannels::new(left.into_reader(), right.into_reader());
        let joined = joiner.read_samples()?.unwrap();
        assert_eq!(joined.end_address, 13);
        assert_eq!(joined.channels, 2);
        assert_eq!(
            joined.data.as_ref(),
            &[0x0, 0x1, 0x2, 0x3, 0x4, 0x5, 0x6, 0x7, 0x8, 0x9, 0xa, 0xb, 0xc, 0xd]
        );

        Ok(())
    }

    #[test]
    fn test_split_channels() -> Result<()> {
        let samples: Vec<i16> = (0..16).collect();
        let stereo =
            Samples::<PcmS16Le> { params: (), end_address: 13, channels: 2, data: samples.into() };

        let splitter = SplitChannels::new(stereo.into_reader());
        let mut left_split = splitter.left();
        let mut right_split = splitter.right();
        let left = left_split.read_samples()?.unwrap();
        let right = right_split.read_samples()?.unwrap();
        assert!(left_split.read_samples()?.is_none());
        assert!(right_split.read_samples()?.is_none());

        assert_eq!(left.end_address, 6);
        assert_eq!(left.channels, 1);
        assert_eq!(left.data.as_ref(), &[0x0, 0x2, 0x4, 0x6, 0x8, 0xa, 0xc]);

        assert_eq!(right.end_address, 6);
        assert_eq!(right.channels, 1);
        assert_eq!(right.data.as_ref(), &[0x1, 0x3, 0x5, 0x7, 0x9, 0xb, 0xd]);

        Ok(())
    }

    #[test]
    fn test_coalesce_samples() {
        let samples1 = Samples::<PcmS16Le> {
            params: (),
            end_address: 0xf,
            channels: 1,
            data: Cow::from((0..16).collect::<Vec<_>>()),
        };
        let samples2 = Samples::<PcmS16Le> {
            params: (),
            end_address: 0xf,
            channels: 1,
            data: Cow::from((16..32).collect::<Vec<_>>()),
        };
        let samples3 = Samples::<PcmS16Le> {
            params: (),
            end_address: 0xf,
            channels: 1,
            data: Cow::from((32..48).collect::<Vec<_>>()),
        };
        let mut reader = ReadSampleList::new(vec![samples1, samples2, samples3]);
        let coalesced = reader.coalesce_samples().unwrap();
        assert_eq!(coalesced.end_address, 0x2f);
        assert_eq!(coalesced.channels, 1);
        assert_eq!(coalesced.data, (0..48).collect::<Vec<_>>());
        assert!(matches!(reader.coalesce_samples(), Err(Error::NoSamplesAvailable)));
    }
}
