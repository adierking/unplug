use super::format::*;
use super::{Error, Result};
use std::any;
use std::borrow::Cow;
use std::collections::VecDeque;
use std::fmt::{self, Debug, Formatter};
use std::iter::{self, FusedIterator};
use std::marker::PhantomData;
use std::rc::Rc;
use std::result::Result as StdResult;
use std::sync::{Arc, Mutex};

/// A block of audio sample data read from an audio source.
#[derive(Clone)]
pub struct Samples<'a, F: FormatTag> {
    /// The number of channels in the data.
    pub channels: usize,
    /// The number of samples per second.
    pub rate: u32,
    /// The number of values to decode. This is in format-specific address units; use
    /// `Format::address_to_index()` and related methods to convert to and from data indexes.
    pub len: usize,
    /// The raw sample data. This is not necessarily a list of samples; for example, GameCube ADPCM
    /// has packed samples which need to be decoded first.
    pub data: Cow<'a, [F::Data]>,
    /// The codec parameters for the samples.
    pub params: F::Params,
}

impl<'a, F: FormatTag> Samples<'a, F> {
    /// Ensures that the sample data is owned, cloning it if necessary.
    pub fn into_owned(self) -> Samples<'static, F> {
        Samples {
            channels: self.channels,
            rate: self.rate,
            len: self.len,
            data: Cow::Owned(self.data.into_owned()),
            params: self.params,
        }
    }
}

impl<'a, F: DynamicFormat> Samples<'a, F> {
    /// Gets the format of the sample data.
    pub fn format(&self) -> Format {
        F::format_from_params(&self.params)
    }

    /// Moves the samples into a reader which returns them. `tag` is a string or tag to identify
    /// the reader for debugging purposes.
    pub fn into_reader(self, tag: impl Into<SourceTag>) -> ReadSampleList<'a, F> {
        ReadSampleList::new(iter::once(self), tag.into())
    }

    /// Casts the samples into another compatible format. This will fail if the format is dynamic
    /// and does not match.
    pub fn try_cast<To: DynamicFormat>(mut self) -> StdResult<Samples<'a, To>, Self>
    where
        F: Cast<To>,
    {
        match F::cast_params(self.params) {
            Ok(params) => Ok(Samples::<To> {
                channels: self.channels,
                len: self.len,
                rate: self.rate,
                data: F::cast_data(self.data),
                params,
            }),
            Err(params) => {
                self.params = params;
                Err(self)
            }
        }
    }

    /// Casts the samples into another compatible format.
    /// ***Panics*** if the cast fails.
    pub fn cast<To: DynamicFormat>(self) -> Samples<'a, To>
    where
        F: Cast<To>,
    {
        match self.try_cast() {
            Ok(s) => s,
            Err(s) => {
                panic!(
                    "failed to cast samples from {:?} to {}",
                    s.format(),
                    any::type_name::<To>()
                );
            }
        }
    }
}

impl<F: PcmFormat> Samples<'_, F> {
    /// Returns an iterator over the samples.
    pub fn iter(&self) -> SampleIterator<'_, F> {
        SampleIterator::new(self)
    }
}

impl<F: ExtendSamples> Samples<'_, F> {
    /// Appends the samples in `other` to `self`. Both sample objects must be aligned on a frame
    /// boundary and share compatible codec parameters. On success, the sample data will become
    /// owned by `self`.
    pub fn extend(&mut self, other: &Samples<'_, F>) -> Result<()> {
        let format = self.format();
        assert_eq!(format, other.format());

        if self.channels != other.channels {
            return Err(Error::InconsistentChannels);
        }
        if self.rate != other.rate {
            return Err(Error::InconsistentSampleRate);
        }

        // Make sure the end of our data is frame-aligned
        if format.frame_address(self.len) != self.len {
            return Err(Error::NotFrameAligned);
        }

        // Our length must match the length of our data buffer
        let next_index = format.address_to_index(self.len);
        let last_index = format.address_to_index(self.len - 1);
        if next_index != self.data.len() || next_index <= last_index {
            return Err(Error::NotFrameAligned);
        }

        F::extend_samples(&mut self.data, &mut self.params, &other.data, &other.params)?;
        self.len += other.len;
        Ok(())
    }
}

impl<'a, F: PcmFormat> Samples<'a, F> {
    /// Creates a sample block from PCM data.
    pub fn from_pcm(data: impl Into<Cow<'a, [F::Data]>>, channels: usize, rate: u32) -> Self {
        let data = data.into();
        Self { channels, rate, len: data.len(), data, params: () }
    }
}

/// A tag which identifies the origin of an audio source.
#[derive(Default, Clone, PartialEq, Eq)]
pub struct SourceTag {
    /// The name of the audio source to use for debugging purposes. Do not make assumptions about
    /// the format of this.
    pub name: String,
    /// The channel(s) that are read by the associated reader.
    pub channel: SourceChannel,
}

impl SourceTag {
    /// Creates a new `SourceTag` for `name` which processes all channels.
    pub fn new(name: String) -> Self {
        Self::with_channel(name, SourceChannel::All)
    }

    /// Creates a new `SourceTag` for `name` and `channel`.
    pub fn with_channel(name: String, channel: SourceChannel) -> Self {
        Self { name, channel }
    }

    /// Updates the tag with a new channel and returns the new tag.
    pub fn for_channel(self, channel: SourceChannel) -> Self {
        Self { name: self.name, channel }
    }

    /// Joins this tag with `other` to produce a new tag with `SourceChannel::All`. If the names do
    /// not match, they will be concatenated.
    pub fn join(&self, other: &SourceTag) -> Self {
        if self.name == other.name {
            Self::new(self.name.clone())
        } else {
            Self::new(format!("{}+{}", self.name, other.name))
        }
    }
}

impl Debug for SourceTag {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let name = if !self.name.is_empty() { &self.name } else { "<unnamed>" };
        match self.channel {
            SourceChannel::All => write!(f, "{}", name),
            SourceChannel::Left => write!(f, "{}[L]", name),
            SourceChannel::Right => write!(f, "{}[R]", name),
        }
    }
}

impl From<String> for SourceTag {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for SourceTag {
    fn from(s: &str) -> Self {
        Self::new(s.to_owned())
    }
}

impl From<Cow<'_, str>> for SourceTag {
    fn from(s: Cow<'_, str>) -> Self {
        match s {
            Cow::Borrowed(s) => Self::new(s.to_owned()),
            Cow::Owned(s) => Self::new(s),
        }
    }
}

/// Indicates the audio channel(s) that will be read by an audio source.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum SourceChannel {
    /// All channels are read.
    All,
    /// Only the left channel is read.
    Left,
    /// Only the right channel is read.
    Right,
}

impl Default for SourceChannel {
    fn default() -> Self {
        Self::All
    }
}

/// Trait for an audio source.
pub trait ReadSamples<'s> {
    /// The format that samples are decoded as. If this is `AnyFormat`, the stream can be in any
    /// format and the actual format can be retrieved with `format()`.
    type Format: FormatTag;

    /// Reads the next block of samples. If there are no more samples, returns `Ok(None)`.
    fn read_samples(&mut self) -> Result<Option<Samples<'s, Self::Format>>>;

    /// Returns the format that all samples will be decoded as.
    fn format(&self) -> Format;

    /// Returns a tag which identifies the audio source for debugging purposes.
    fn tag(&self) -> &SourceTag;

    /// Reads all available samples and concatenates them into a single `Samples` object. The
    /// samples must have a static format and follow the rules for `Samples::append()`. If no
    /// samples are available, `Err(EmptyStream)` is returned.
    fn read_all_samples(&mut self) -> Result<Samples<'s, Self::Format>>
    where
        Self::Format: ExtendSamples,
    {
        let mut result: Option<Samples<'s, Self::Format>> = None;
        while let Some(samples) = self.read_samples()? {
            match &mut result {
                Some(a) => a.extend(&samples)?,
                None => result = Some(samples),
            }
        }
        result.ok_or(Error::EmptyStream)
    }

    /// Creates an adapter which ensures all samples contain owned data. Borrowed sample data
    /// returned by the underlying reader will be cloned.
    fn owned(self) -> OwnedSamples<'s, Self>
    where
        Self: Sized,
    {
        OwnedSamples::new(self)
    }

    /// Creates an adapter which casts audio samples to a compatible format as they are read. If a
    /// cast fails, this will stop with `Error::UnsupportedFormat`.
    fn cast<F: DynamicFormat>(self) -> CastSamples<'s, Self, F>
    where
        Self: Sized,
        Self::Format: Cast<F>,
    {
        CastSamples::new(self)
    }

    /// Creates an adapter with a `peek_samples()` method that allows peeking at the next packets of
    /// samples without consuming them.
    fn peekable(self) -> PeekSamples<'s, Self>
    where
        Self: Sized,
    {
        PeekSamples::new(self)
    }

    /// Creates an adapter which converts audio samples to another format.
    fn convert<'r, To>(self) -> Box<dyn ReadSamples<'s, Format = To> + 'r>
    where
        's: 'r,
        To: StaticFormat,
        Self: Sized + 'r,
        Self::Format: DynamicFormat + Convert<To>,
    {
        Self::Format::convert(Box::from(self))
    }

    /// Creates an adaptor which joins this mono stream with another mono stream to form a single
    /// stereo stream. The streams must return sample blocks whose sizes match. This stream will be
    /// the left channel, and `right` will be the right channel.
    fn with_right_channel<'r>(
        self,
        right: impl ReadSamples<'s, Format = Self::Format> + 'r,
    ) -> JoinChannels<'r, 's, Self::Format>
    where
        Self: Sized + 'r,
        Self::Format: PcmFormat,
    {
        JoinChannels::new(self, right)
    }

    /// Creates an adapter which splits a stereo stream into two mono streams.
    fn split_channels<'r>(self) -> SplitChannels<'r, 's, Self::Format>
    where
        Self: Sized + 'r,
        Self::Format: PcmFormat,
    {
        SplitChannels::new(self)
    }
}

impl<'a, F, R> ReadSamples<'a> for &mut R
where
    F: FormatTag,
    R: ReadSamples<'a, Format = F> + ?Sized,
{
    type Format = F;
    fn read_samples(&mut self) -> Result<Option<Samples<'a, Self::Format>>> {
        (**self).read_samples()
    }
    fn format(&self) -> Format {
        (**self).format()
    }
    fn tag(&self) -> &SourceTag {
        (**self).tag()
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
    fn format(&self) -> Format {
        (**self).format()
    }
    fn tag(&self) -> &SourceTag {
        (**self).tag()
    }
}

/// `ReadSamples` implementation which yields `Samples` structs from a queue. All samples must have
/// the same format and channel count.
pub struct ReadSampleList<'s, F: DynamicFormat> {
    samples: VecDeque<Samples<'s, F>>,
    format: Format,
    tag: SourceTag,
}

impl<'s, F: DynamicFormat> ReadSampleList<'s, F> {
    pub fn new(
        samples: impl IntoIterator<Item = Samples<'s, F>>,
        tag: impl Into<SourceTag>,
    ) -> Self {
        Self::new_impl(samples.into_iter().collect(), tag.into())
    }

    fn new_impl(samples: VecDeque<Samples<'s, F>>, tag: SourceTag) -> Self {
        assert!(!samples.is_empty(), "sample list is empty");
        let front = samples.front().unwrap();
        let format = front.format();
        let channels = front.channels;
        for s in samples.iter().skip(1) {
            assert_eq!(s.format(), format);
            assert_eq!(s.channels, channels);
        }
        Self { samples: samples.into_iter().collect(), format, tag }
    }
}

impl<'s, F: DynamicFormat> ReadSamples<'s> for ReadSampleList<'s, F> {
    type Format = F;
    fn read_samples(&mut self) -> Result<Option<Samples<'s, Self::Format>>> {
        Ok(self.samples.pop_front())
    }
    fn format(&self) -> Format {
        self.format
    }
    fn tag(&self) -> &SourceTag {
        &self.tag
    }
}

/// An adapter which ensures all samples contain owned data. Borrowed sample data returned by the
/// underlying reader will be cloned.
pub struct OwnedSamples<'s, R: ReadSamples<'s>> {
    inner: R,
    _marker: PhantomData<&'s ()>,
}

impl<'s, R: ReadSamples<'s>> OwnedSamples<'s, R> {
    pub fn new(inner: R) -> Self {
        Self { inner, _marker: PhantomData }
    }
}

impl<'s, R: ReadSamples<'s>> ReadSamples<'static> for OwnedSamples<'s, R> {
    type Format = R::Format;
    fn read_samples(&mut self) -> Result<Option<Samples<'static, Self::Format>>> {
        self.inner.read_samples().map(|s| s.map(Samples::into_owned))
    }
    fn format(&self) -> Format {
        self.inner.format()
    }
    fn tag(&self) -> &SourceTag {
        self.inner.tag()
    }
}

/// An adapter which casts audio samples to a compatible format as they are read. If a cast fails,
/// this will stop with `Error::UnsupportedFormat`.
pub struct CastSamples<'s, R: ReadSamples<'s>, F: DynamicFormat>
where
    R::Format: Cast<F>,
{
    inner: R,
    _marker: PhantomData<&'s F>,
}

impl<'s, R: ReadSamples<'s>, F: DynamicFormat> CastSamples<'s, R, F>
where
    R::Format: Cast<F>,
{
    /// Creates a new `CastSamples` which casts samples from `inner`.
    pub fn new(inner: R) -> Self {
        Self { inner, _marker: PhantomData }
    }
}

impl<'s, R: ReadSamples<'s>, F: DynamicFormat> ReadSamples<'s> for CastSamples<'s, R, F>
where
    R::Format: DynamicFormat + Cast<F>,
{
    type Format = F;

    fn read_samples(&mut self) -> Result<Option<Samples<'s, Self::Format>>> {
        match self.inner.read_samples() {
            Ok(Some(samples)) => match samples.try_cast() {
                Ok(samples) => Ok(Some(samples)),
                Err(samples) => Err(Error::UnsupportedFormat(samples.format())),
            },
            Ok(None) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn format(&self) -> Format {
        self.inner.format()
    }

    fn tag(&self) -> &SourceTag {
        self.inner.tag()
    }
}

/// An adapter with a `peek_samples()` method that allows peeking at the next packets of samples
/// without consuming them.
pub struct PeekSamples<'s, R: ReadSamples<'s>> {
    inner: R,
    next: Option<Samples<'s, R::Format>>,
}

impl<'s, R: ReadSamples<'s>> PeekSamples<'s, R> {
    /// Creates a new `PeekSamples` which wraps `inner`.
    pub fn new(inner: R) -> Self {
        Self { inner, next: None }
    }

    /// Returns a reference to the next-available packet of samples without consuming it. This may
    /// read from the underlying stream if no samples are cached. If there are no more samples,
    /// returns `Ok(None)`.
    pub fn peek_samples(&mut self) -> Result<Option<&Samples<'s, R::Format>>> {
        if self.next.is_none() {
            self.next = self.inner.read_samples()?;
        }
        Ok(self.next.as_ref())
    }
}

impl<'s, R: ReadSamples<'s>> ReadSamples<'s> for PeekSamples<'s, R> {
    type Format = R::Format;

    fn read_samples(&mut self) -> Result<Option<Samples<'s, Self::Format>>> {
        match self.next.take() {
            Some(next) => Ok(Some(next)),
            None => self.inner.read_samples(),
        }
    }

    fn format(&self) -> Format {
        self.inner.format()
    }

    fn tag(&self) -> &SourceTag {
        self.inner.tag()
    }
}

/// An iterator over PCM audio samples. Items are slices containing the samples from left to right.
pub struct SampleIterator<'a, F: PcmFormat> {
    samples: &'a [F::Data],
    step: usize,
}

impl<'a, F: PcmFormat> SampleIterator<'a, F> {
    /// Creates a `SampleIterator` which iterates over the data in `samples`.
    pub fn new(samples: &'a Samples<'a, F>) -> Self {
        Self { samples: &samples.data[..samples.len], step: samples.channels }
    }
}

impl<'a, F: PcmFormat> Iterator for SampleIterator<'a, F> {
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

impl<F: PcmFormat> FusedIterator for SampleIterator<'_, F> {}

/// Joins two raw mono streams into a single stereo stream. The streams must return sample blocks
/// whose sizes match and have the same format.
pub struct JoinChannels<'r, 's, F: PcmFormat> {
    left: Box<dyn ReadSamples<'s, Format = F> + 'r>,
    right: Box<dyn ReadSamples<'s, Format = F> + 'r>,
    format: Format,
    tag: SourceTag,
    _marker: PhantomData<F>,
}

impl<'r, 's, F: PcmFormat> JoinChannels<'r, 's, F> {
    pub fn new(
        left: impl ReadSamples<'s, Format = F> + 'r,
        right: impl ReadSamples<'s, Format = F> + 'r,
    ) -> Self {
        assert_eq!(left.format(), right.format());
        let format = left.format();
        let tag = left.tag().join(right.tag());
        Self { left: Box::from(left), right: Box::from(right), format, tag, _marker: PhantomData }
    }
}

impl<'s, F: PcmFormat> ReadSamples<'s> for JoinChannels<'_, 's, F> {
    type Format = F;

    fn read_samples(&mut self) -> Result<Option<Samples<'s, Self::Format>>> {
        let left = self.left.read_samples()?;
        let right = self.right.read_samples()?;
        let (left, right) = match (left, right) {
            (Some(l), Some(r)) => (l, r),
            _ => return Ok(None),
        };
        if left.channels != 1 || right.channels != 1 {
            return Err(Error::StreamNotMono);
        }
        if left.rate != right.rate {
            return Err(Error::InconsistentSampleRate);
        }

        // TODO: Optimize?
        let mut merged = Vec::with_capacity(left.data.len() + right.data.len());
        for (&l, &r) in left.data.iter().take(left.len).zip(&right.data[..right.len]) {
            merged.push(l);
            merged.push(r);
        }
        Ok(Some(Samples::from_pcm(merged, 2, left.rate)))
    }

    fn format(&self) -> Format {
        self.format
    }

    fn tag(&self) -> &SourceTag {
        &self.tag
    }
}

/// An adapter which splits a stereo stream into two mono streams.
pub struct SplitChannels<'r, 's, F: PcmFormat> {
    state: Arc<Mutex<SplitChannelsState<'r, 's, F>>>,
}

impl<'r, 's, F: PcmFormat> SplitChannels<'r, 's, F> {
    /// Creates a new `SplitChannels` which reads samples from `reader`.
    pub fn new(reader: impl ReadSamples<'s, Format = F> + 'r) -> Self {
        Self { state: SplitChannelsState::new(Box::from(reader)) }
    }

    /// Returns a thread-safe reader over the samples in the left channel.
    pub fn left(&self) -> SplitChannelsReader<'r, 's, F> {
        SplitChannelsReader::new(Arc::clone(&self.state), SourceChannel::Left)
    }

    /// Returns a thread-safe reader over the samples in the right channel.
    pub fn right(&self) -> SplitChannelsReader<'r, 's, F> {
        SplitChannelsReader::new(Arc::clone(&self.state), SourceChannel::Right)
    }
}

/// State for `SplitChannels` which is shared across readers.
struct SplitChannelsState<'r, 's, F: PcmFormat> {
    /// The inner reader to read new samples from.
    reader: Box<dyn ReadSamples<'s, Format = F> + 'r>,
    /// Samples which have not yet been processed by the left reader.
    left: VecDeque<Rc<Samples<'s, F>>>,
    /// Samples which have not yet been processed by the right reader.
    right: VecDeque<Rc<Samples<'s, F>>>,
}

impl<'r, 's, F: PcmFormat> SplitChannelsState<'r, 's, F> {
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
pub struct SplitChannelsReader<'r, 's, F: PcmFormat> {
    state: Arc<Mutex<SplitChannelsState<'r, 's, F>>>,
    format: Format,
    tag: SourceTag,
}

impl<'r, 's, F: PcmFormat> SplitChannelsReader<'r, 's, F> {
    /// Creates a new `SplitChannelsReader` which shares `state` and reads `channel`.
    fn new(state: Arc<Mutex<SplitChannelsState<'r, 's, F>>>, channel: SourceChannel) -> Self {
        debug_assert!(matches!(channel, SourceChannel::Left | SourceChannel::Right));
        let lock = state.lock().unwrap();
        let format = lock.reader.format();
        let tag = lock.reader.tag().clone().for_channel(channel);
        drop(lock);
        Self { state, format, tag }
    }
}

impl<'s, F: PcmFormat> ReadSamples<'s> for SplitChannelsReader<'_, 's, F> {
    type Format = F;

    fn read_samples(&mut self) -> Result<Option<Samples<'s, Self::Format>>> {
        // We assume here that the channels can be read from in any order and that we are only
        // expected to read as much as we need. Each channel must hold onto samples it hasn't
        // returned yet, and we also shouldn't read every sample from the inner reader all at once.
        // The basic idea here is that we share samples across both channels using queues with
        // refcounted sample data. When we try to read from an empty queue, we read more samples and
        // push them onto both queues.
        let mut state = self.state.lock().unwrap();
        let is_right = self.tag.channel == SourceChannel::Right;
        let samples = loop {
            let queue = if is_right { &mut state.right } else { &mut state.left };
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
            channel_data.push(if is_right { right_sample } else { left_sample });
        }
        Ok(Some(Samples::from_pcm(channel_data, 1, samples.rate)))
    }

    fn format(&self) -> Format {
        self.format
    }

    fn tag(&self) -> &SourceTag {
        &self.tag
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::format::GcAdpcm;
    use std::convert::TryFrom;

    #[derive(Clone)]
    struct PcmS16LeParams;
    impl FormatTag for PcmS16LeParams {
        type Data = i16;
        type Params = i32;
    }
    impl StaticFormat for PcmS16LeParams {
        const FORMAT: Format = Format::PcmS16Le;
    }
    impl ExtendSamples for PcmS16LeParams {
        fn extend_samples(
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
    fn test_owned() {
        let samples: Vec<i16> = (0..16).collect();
        let borrowed = Samples::<PcmS16Le>::from_pcm(&samples, 1, 44100).into_reader("test");
        let owned = OwnedSamples::new(borrowed).read_all_samples().unwrap();
        assert!(owned.channels == 1);
        assert!(owned.len == 16);
        assert!(matches!(owned.data, Cow::Owned(_)));
        assert!(owned.data == samples);
    }

    #[test]
    fn test_any_without_context() {
        let samples: Vec<i16> = (0..16).collect();
        let original = Samples::<PcmS16Le>::from_pcm(samples, 1, 44100);

        let any = original.clone().cast::<AnyFormat>();
        assert_eq!(any.format(), Format::PcmS16Le);
        assert_eq!(any.channels, original.channels);
        assert_eq!(any.len, original.len);

        // The parameters are the same, but this should fail because the formats differ
        let casted = any.try_cast::<PcmS32Le>();
        assert!(casted.is_err());
        let any = casted.err().unwrap();

        let casted = any.cast::<PcmS16Le>();
        assert_eq!(casted.format(), Format::PcmS16Le);
        assert_eq!(casted.channels, original.channels);
        assert_eq!(casted.len, original.len);
        assert_eq!(casted.data, original.data);
        assert!(matches!(casted.data, Cow::Owned(_)));
    }

    #[test]
    fn test_any_with_params() {
        let samples: Vec<i16> = (0..16).collect();
        let original = Samples::<PcmS16LeParams> {
            channels: 1,
            rate: 44100,
            len: 16,
            data: Cow::Borrowed(&samples),
            params: 123,
        };

        let any = original.clone().cast::<AnyFormat>();
        assert_eq!(any.format(), Format::PcmS16Le);
        assert_eq!(any.channels, original.channels);
        assert_eq!(any.len, original.len);

        // The formats are the same, but this should fail because the parameters differ
        let casted = any.try_cast::<PcmS16Le>();
        assert!(casted.is_err());
        let any = casted.err().unwrap();

        let casted = any.cast::<PcmS16LeParams>();
        assert_eq!(casted.format(), Format::PcmS16Le);
        assert_eq!(casted.channels, original.channels);
        assert_eq!(casted.len, original.len);
        assert_eq!(casted.data, original.data);
        assert_eq!(casted.params, original.params);
        assert!(matches!(casted.data, Cow::Borrowed(_)));
    }

    #[test]
    fn test_any_into_any() {
        let samples: Vec<i16> = (0..16).collect();
        let original = Samples::<PcmS16Le>::from_pcm(samples, 1, 44100);

        let any = original.clone().cast::<AnyFormat>().cast::<AnyFormat>();
        assert_eq!(any.format(), Format::PcmS16Le);
        assert_eq!(any.channels, original.channels);
        assert_eq!(any.len, original.len);

        let casted = any.cast::<PcmS16Le>();
        assert_eq!(casted.format(), Format::PcmS16Le);
        assert_eq!(casted.channels, original.channels);
        assert_eq!(casted.len, original.len);
        assert_eq!(casted.data, original.data);
        assert!(matches!(casted.data, Cow::Owned(_)));
    }

    #[test]
    fn test_any_samples() -> Result<()> {
        let samples: Vec<i16> = (0..16).collect();
        let original = Samples::<PcmS16Le>::from_pcm(&samples, 1, 44100);

        let mut caster = CastSamples::new(original.into_reader("test"));
        let casted: Samples<'_, AnyFormat> = caster.read_samples()?.unwrap();
        assert_eq!(casted.format(), Format::PcmS16Le);
        assert!(matches!(casted.data, Cow::Borrowed(_)));
        Ok(())
    }

    #[test]
    fn test_cast_samples() -> Result<()> {
        let samples: Vec<i16> = (0..16).collect();
        let original = Samples::<PcmS16Le>::from_pcm(&samples, 1, 44100);

        let any = original.clone().cast::<AnyFormat>().into_reader("test");
        let mut caster = CastSamples::new(any);
        let casted: Samples<'_, PcmS16Le> = caster.read_samples()?.unwrap();
        assert_eq!(casted.format(), Format::PcmS16Le);
        assert_eq!(casted.channels, original.channels);
        assert_eq!(casted.len, original.len);
        assert_eq!(casted.data, original.data);
        assert!(matches!(casted.data, Cow::Borrowed(_)));

        // Casting to PcmS32Le should fail with UnsupportedFormat
        let any = original.clone().cast::<AnyFormat>().into_reader("test");
        let mut caster = CastSamples::new(any);
        let result: Result<Option<Samples<'_, PcmS32Le>>> = caster.read_samples();
        assert!(matches!(result, Err(Error::UnsupportedFormat(Format::PcmS16Le))));

        Ok(())
    }

    #[test]
    fn test_extend_samples() {
        let mut samples1 = Samples::<GcAdpcm> {
            channels: 1,
            rate: 44100,
            len: 32,
            data: Cow::from((0..16).collect::<Vec<_>>()),
            params: Default::default(),
        };
        let samples2 = Samples::<GcAdpcm> {
            channels: 1,
            rate: 44100,
            len: 32,
            data: Cow::from((16..32).collect::<Vec<_>>()),
            params: Default::default(),
        };
        samples1.extend(&samples2).unwrap();
        assert_eq!(samples1.len, 64);
        assert_eq!(samples1.data, (0..32).collect::<Vec<_>>());
    }

    #[test]
    fn test_extend_samples_partial() {
        let mut samples1 = Samples::<GcAdpcm> {
            channels: 1,
            rate: 44100,
            len: 32,
            data: Cow::from((0..16).collect::<Vec<_>>()),
            params: Default::default(),
        };
        let samples2 = Samples::<GcAdpcm> {
            channels: 1,
            rate: 44100,
            len: 32,
            data: Cow::from((16..48).collect::<Vec<_>>()),
            params: Default::default(),
        };
        samples1.extend(&samples2).unwrap();
        assert_eq!(samples1.len, 64);
        assert_eq!(samples1.data, (0..48).collect::<Vec<_>>());
    }

    #[test]
    fn test_extend_samples_channel_mismatch() {
        let mut samples1 = Samples::<GcAdpcm> {
            channels: 1,
            rate: 44100,
            len: 32,
            data: Cow::from((0..16).collect::<Vec<_>>()),
            params: Default::default(),
        };
        let mut samples2 = Samples::<GcAdpcm> {
            channels: 2,
            rate: 44100,
            len: 32,
            data: Cow::from((16..32).collect::<Vec<_>>()),
            params: Default::default(),
        };
        assert!(matches!(samples1.extend(&samples2), Err(Error::InconsistentChannels)));
        assert!(matches!(samples2.extend(&samples1), Err(Error::InconsistentChannels)));
    }

    #[test]
    fn test_extend_samples_rate_mismatch() {
        let mut samples1 = Samples::<GcAdpcm> {
            channels: 1,
            rate: 44100,
            len: 32,
            data: Cow::from((0..16).collect::<Vec<_>>()),
            params: Default::default(),
        };
        let mut samples2 = Samples::<GcAdpcm> {
            channels: 1,
            rate: 48000,
            len: 32,
            data: Cow::from((16..32).collect::<Vec<_>>()),
            params: Default::default(),
        };
        assert!(matches!(samples1.extend(&samples2), Err(Error::InconsistentSampleRate)));
        assert!(matches!(samples2.extend(&samples1), Err(Error::InconsistentSampleRate)));
    }

    #[test]
    fn test_extend_samples_unaligned() {
        let mut samples1 = Samples::<GcAdpcm> {
            channels: 1,
            rate: 44100,
            len: 31,
            data: Cow::from((0..16).collect::<Vec<_>>()),
            params: Default::default(),
        };
        let samples2 = Samples::<GcAdpcm> {
            channels: 1,
            rate: 44100,
            len: 32,
            data: Cow::from((16..32).collect::<Vec<_>>()),
            params: Default::default(),
        };
        assert!(matches!(samples1.extend(&samples2), Err(Error::NotFrameAligned)));
    }

    #[test]
    fn test_extend_samples_before_end() {
        let mut samples1 = Samples::<GcAdpcm> {
            channels: 1,
            rate: 44100,
            len: 33,
            data: Cow::from((0..17).collect::<Vec<_>>()),
            params: Default::default(),
        };
        let samples2 = Samples::<GcAdpcm> {
            channels: 1,
            rate: 44100,
            len: 32,
            data: Cow::from((16..32).collect::<Vec<_>>()),
            params: Default::default(),
        };
        assert!(matches!(samples1.extend(&samples2), Err(Error::NotFrameAligned)));
    }

    #[test]
    fn test_sample_iterator_mono() {
        let data: Vec<i16> = (0..8).collect();
        let samples = Samples::<PcmS16Le>::from_pcm(&data[..7], 1, 44100);
        let iterated: Vec<_> =
            samples.iter().map(|s| <[i16; 1]>::try_from(s).ok().unwrap()).collect();
        assert_eq!(iterated, &[[0x0], [0x1], [0x2], [0x3], [0x4], [0x5], [0x6]]);
    }

    #[test]
    fn test_sample_iterator_stereo() {
        let data: Vec<i16> = (0..8).collect();
        let samples = Samples::<PcmS16Le>::from_pcm(&data[..6], 2, 44100);
        let iterated: Vec<_> =
            samples.iter().map(|s| <[i16; 2]>::try_from(s).ok().unwrap()).collect();
        assert_eq!(iterated, &[[0x0, 0x1], [0x2, 0x3], [0x4, 0x5]]);
    }

    #[test]
    fn test_join_channels() -> Result<()> {
        let ldata: Vec<i16> = (0..16).step_by(2).collect();
        let rdata: Vec<i16> = (0..16).skip(1).step_by(2).collect();
        let left = Samples::<PcmS16Le>::from_pcm(&ldata[..7], 1, 44100);
        let right = Samples::<PcmS16Le>::from_pcm(&rdata[..7], 1, 44100);

        let mut joiner = JoinChannels::new(left.into_reader("left"), right.into_reader("right"));
        let joined = joiner.read_samples()?.unwrap();
        assert_eq!(joined.len, 14);
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
        let stereo = Samples::<PcmS16Le>::from_pcm(&samples[..14], 2, 44100);

        let splitter = SplitChannels::new(stereo.into_reader("test"));
        let mut left_split = splitter.left();
        let mut right_split = splitter.right();
        let left = left_split.read_samples()?.unwrap();
        let right = right_split.read_samples()?.unwrap();
        assert!(left_split.read_samples()?.is_none());
        assert!(right_split.read_samples()?.is_none());

        assert_eq!(left.len, 7);
        assert_eq!(left.channels, 1);
        assert_eq!(left.data.as_ref(), &[0x0, 0x2, 0x4, 0x6, 0x8, 0xa, 0xc]);

        assert_eq!(right.len, 7);
        assert_eq!(right.channels, 1);
        assert_eq!(right.data.as_ref(), &[0x1, 0x3, 0x5, 0x7, 0x9, 0xb, 0xd]);

        Ok(())
    }

    #[test]
    fn test_read_all_samples() {
        let samples1 = Samples::<PcmS16Le>::from_pcm((0..16).collect::<Vec<_>>(), 1, 44100);
        let samples2 = Samples::<PcmS16Le>::from_pcm((16..32).collect::<Vec<_>>(), 1, 44100);
        let samples3 = Samples::<PcmS16Le>::from_pcm((32..48).collect::<Vec<_>>(), 1, 44100);
        let mut reader = ReadSampleList::new(vec![samples1, samples2, samples3], "test");
        let all = reader.read_all_samples().unwrap();
        assert_eq!(all.len, 48);
        assert_eq!(all.channels, 1);
        assert_eq!(all.data, (0..48).collect::<Vec<_>>());
        assert!(matches!(reader.read_all_samples(), Err(Error::EmptyStream)));
    }

    #[test]
    fn test_peek_samples() -> Result<()> {
        let samples1 = Samples::<PcmS16Le>::from_pcm((0..1).collect::<Vec<_>>(), 1, 44100);
        let samples2 = Samples::<PcmS16Le>::from_pcm((1..2).collect::<Vec<_>>(), 1, 44100);
        let samples3 = Samples::<PcmS16Le>::from_pcm((2..3).collect::<Vec<_>>(), 1, 44100);
        let mut reader = ReadSampleList::new(vec![samples1, samples2, samples3], "test").peekable();

        assert_eq!(reader.peek_samples()?.unwrap().data[0], 0);
        assert_eq!(reader.peek_samples()?.unwrap().data[0], 0);
        assert_eq!(reader.read_samples()?.unwrap().data[0], 0);

        assert_eq!(reader.read_samples()?.unwrap().data[0], 1);

        assert_eq!(reader.peek_samples()?.unwrap().data[0], 2);
        assert_eq!(reader.read_samples()?.unwrap().data[0], 2);

        assert!(reader.peek_samples()?.is_none());
        assert!(reader.read_samples()?.is_none());
        assert!(reader.peek_samples()?.is_none());
        Ok(())
    }
}
