use super::vgaudio::coefficients::{self, PcmHistory, Vec3};
use super::vgaudio::encode;
use super::{Coefficients, GcAdpcm, Info, BYTES_PER_FRAME, SAMPLES_PER_FRAME};
use crate::audio::format::{PcmS16Le, StaticFormat};
use crate::audio::sample::ReadSampleList;
use crate::audio::{Error, Format, ProgressHint, ReadSamples, Result, Samples, SourceTag};
use arrayvec::ArrayVec;
use std::collections::VecDeque;
use std::mem;
use std::num::NonZeroU64;
use tracing::{instrument, trace_span};

/// Calculates ADPCM coefficients for sample data.
#[derive(Default, Clone)]
pub struct CoefficientCalculator {
    records: Vec<Vec3>,
    pcm_hist: PcmHistory,
    frame_len: usize,
}

impl CoefficientCalculator {
    /// Creates a new `CoefficientCalculator`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Updates the calculator by processing the sample data in `samples`.
    #[instrument(level = "trace", name = "CoefficientCalculator::process", skip_all)]
    pub fn process(&mut self, samples: &[i16]) {
        let mut i = 0;
        while i < samples.len() {
            let remaining = samples.len() - i;
            let start = SAMPLES_PER_FRAME + self.frame_len;
            let len = (SAMPLES_PER_FRAME - self.frame_len).min(remaining);
            let end = start + len;
            self.pcm_hist[start..end].copy_from_slice(&samples[i..(i + len)]);
            self.frame_len += len;
            if self.frame_len == SAMPLES_PER_FRAME {
                coefficients::process_frame(&mut self.pcm_hist, &mut self.records);
                self.frame_len = 0;
            }
            i += len;
        }
    }

    /// Finishes calculating and returns the calculated coefficients.
    #[instrument(level = "trace", name = "CoefficientCalculator::finish", skip_all)]
    pub fn finish(mut self) -> Coefficients {
        if self.frame_len > 0 {
            self.pcm_hist[(SAMPLES_PER_FRAME + self.frame_len)..].fill(0);
            coefficients::process_frame(&mut self.pcm_hist, &mut self.records);
        }
        coefficients::finish(&self.records)
    }

    /// Helper function for calculating coefficients over one set of samples.
    pub fn calculate(samples: &[i16]) -> Coefficients {
        let mut calculator = Self::new();
        calculator.process(samples);
        calculator.finish()
    }
}

type ProgressCallback<'a> = Box<dyn FnMut(Option<ProgressHint>) + 'a>;
type EncoderPair<'s> = (Encoder<'s, 's>, Option<Encoder<'s, 's>>);

/// A helper API for building up an `Encoder` by reading and analyzing samples.
pub struct EncoderBuilder<'r, 's: 'r> {
    reader: Box<dyn ReadSamples<'s, Format = PcmS16Le> + 'r>,
    mono_block_size: usize,
    stereo_block_size: usize,
    on_progress: Option<ProgressCallback<'r>>,
}

impl<'r, 's: 'r> EncoderBuilder<'r, 's> {
    /// Creates a new `EncoderBuilder` which analyzes samples in `reader` and builds an encoder to
    /// encode them.
    pub fn new(reader: impl ReadSamples<'s, Format = PcmS16Le> + 'r) -> Self {
        Self::new_impl(Box::from(reader))
    }

    fn new_impl(reader: Box<dyn ReadSamples<'s, Format = PcmS16Le> + 'r>) -> Self {
        Self {
            reader,
            mono_block_size: usize::MAX,
            stereo_block_size: usize::MAX,
            on_progress: None,
        }
    }

    /// Sets the encoder block size for both the mono and stereo channel layouts.
    pub fn block_size(mut self, size: usize) -> Self {
        self.mono_block_size = size;
        self.stereo_block_size = size;
        self
    }

    /// Sets the encoder block size to use for mono streams.
    pub fn mono_block_size(mut self, size: usize) -> Self {
        self.mono_block_size = size;
        self
    }

    /// Sets the encoder block size to use for stereo streams.
    pub fn stereo_block_size(mut self, size: usize) -> Self {
        self.stereo_block_size = size;
        self
    }

    /// Sets a callback to run for progress updates. If the total amount of work is unknown, the
    /// callback will still be invoked with a `None` hint.
    pub fn on_progress(mut self, callback: impl FnMut(Option<ProgressHint>) + 'r) -> Self {
        self.on_progress = Some(Box::from(callback));
        self
    }

    /// Analyzes the audio stream and builds encoder(s) to encode the samples. The return value is a
    /// `(left, right)` pair of encoders, where the right encoder is only present for stereo
    /// streams.
    pub fn build(mut self) -> Result<EncoderPair<'s>> {
        let mut reader = self.reader.peekable();
        let first = match reader.peek_samples()? {
            Some(s) => s,
            None => return Err(Error::EmptyStream),
        };
        let channels = first.channels;
        self.reader = Box::from(reader);
        match channels {
            1 => self.build_mono(),
            2 => self.build_stereo(),
            _ => Err(Error::InvalidChannelCount(channels as u32)),
        }
    }

    /// Helper function for quickly building `Encoder`s over samples read from `reader` using
    /// default settings. See `build()` for an explanation of the return value.
    pub fn simple(reader: impl ReadSamples<'s, Format = PcmS16Le> + 'r) -> Result<EncoderPair<'s>> {
        Self::new_impl(Box::from(reader)).build()
    }

    fn build_mono(mut self) -> Result<EncoderPair<'s>> {
        let mut channel = EncoderChannel::new(self.mono_block_size, self.reader.tag().clone());
        Self::update_progress(self.on_progress.as_mut(), self.reader.progress());
        while let Some(samples) = self.reader.read_samples()? {
            channel.add_samples(samples);
            Self::update_progress(self.on_progress.as_mut(), self.reader.progress());
        }
        Ok((channel.into_encoder(), None))
    }

    fn build_stereo(self) -> Result<EncoderPair<'s>> {
        let splitter = self.reader.split_channels();
        let mut left = splitter.left();
        let mut right = splitter.right();
        let mut left_channel = EncoderChannel::new(self.stereo_block_size, left.tag().clone());
        let mut right_channel = EncoderChannel::new(self.stereo_block_size, right.tag().clone());
        let mut on_progress = self.on_progress;
        Self::update_progress(on_progress.as_mut(), left.progress());
        while let (Some(l), Some(r)) = (left.read_samples()?, right.read_samples()?) {
            left_channel.add_samples(l);
            right_channel.add_samples(r);
            Self::update_progress(on_progress.as_mut(), left.progress());
        }
        Ok((left_channel.into_encoder(), Some(right_channel.into_encoder())))
    }

    fn update_progress(
        on_progress: Option<&mut ProgressCallback<'r>>,
        progress: Option<ProgressHint>,
    ) {
        if let Some(on_progress) = on_progress {
            on_progress(progress);
        }
    }
}

/// Implementation detail for `EncoderBuilder` which builds an encoder for a single channel.
struct EncoderChannel<'s> {
    block_size: usize,
    tag: SourceTag,
    samples: Vec<Samples<'s, PcmS16Le>>,
    coeff: CoefficientCalculator,
}

impl<'s> EncoderChannel<'s> {
    fn new(block_size: usize, tag: SourceTag) -> Self {
        Self { block_size, tag, samples: vec![], coeff: Default::default() }
    }

    fn add_samples(&mut self, samples: Samples<'s, PcmS16Le>) {
        self.coeff.process(&samples.data[..samples.len]);
        // We have to keep a list of all the samples so we can "replay" them to the encoder.
        // Unfortunately it isn't possible to encode in a single pass due to the coefficient
        // calculation.
        self.samples.push(samples);
    }

    fn into_encoder(self) -> Encoder<'s, 's> {
        let state = Info { coefficients: self.coeff.finish(), ..Default::default() };
        let reader = ReadSampleList::new(self.samples, self.tag);
        Encoder::with_block_size(reader, state, self.block_size)
    }
}

/// A partial block of encoded GameCube ADPCM data.
#[derive(Default)]
struct Block {
    data: Vec<u8>,
    len: usize,
    initial_state: Info,
}

impl Block {
    /// Encodes `samples`, adds them to the block, and updates `state`.
    fn encode(&mut self, samples: &[i16], state: &mut Info) {
        let bytes = trace_span!("encode").in_scope(|| encode::encode(samples, state));
        self.len += bytes.len() * 2 - samples.len() % 2;
        self.data.extend(bytes);
    }

    /// Completes the block and turns it into a `Samples` object.
    fn finish<'s>(mut self, rate: u32, max_size: usize) -> Samples<'s, GcAdpcm> {
        assert!(!self.data.is_empty());
        assert!(self.data.len() <= max_size);
        self.initial_state.context.predictor_and_scale = self.data[0] as u16;
        Samples {
            channels: 1,
            rate,
            len: self.len,
            data: self.data.into(),
            params: self.initial_state,
        }
    }
}

/// Encodes raw PCM data into GameCube ADPCM format. Samples are encoded on-demand as they are read
/// from the encoder.
pub struct Encoder<'r, 's> {
    /// The inner reader to read samples from.
    reader: Box<dyn ReadSamples<'s, Format = PcmS16Le> + 'r>,
    /// The maximum size of each block in bytes.
    block_size: usize,
    /// The sample rate.
    sample_rate: u32,
    /// The number of samples which have been encoded so far.
    samples_encoded: u64,
    /// The total number of samples which will need to be encoded.
    total_samples: Option<NonZeroU64>,
    /// The current frame which needs to be filled before it can be encoded.
    frame: ArrayVec<[i16; SAMPLES_PER_FRAME]>,
    /// A buffer for samples which have been read but not encoded yet.
    buffer: VecDeque<Samples<'s, PcmS16Le>>,
    /// The offset within the samples at the front of `buffer` to start encoding from.
    sample_offset: usize,
    /// The block currently being built.
    block: Block,
    /// The current encoding state.
    state: Info,
    /// `true` if encoding has finished and there are no more samples to read.
    done: bool,
}

impl<'r, 's> Encoder<'r, 's> {
    /// Creates an `Encoder` which reads samples from `reader` and has initial state `state`. This
    /// is a low-level interface; consider using `EncoderBuilder` instead.
    pub fn new(reader: impl ReadSamples<'s, Format = PcmS16Le> + 'r, state: Info) -> Self {
        Self::with_block_size_impl(Box::from(reader), state, usize::MAX)
    }

    /// Creates an `Encoder` which reads samples from `reader`, has initial state `state`, and
    /// outputs blocks of data which are no larger than `block_size`. This is a low-level interface;
    /// consider using `EncoderBuilder` instead.
    pub fn with_block_size(
        reader: impl ReadSamples<'s, Format = PcmS16Le> + 'r,
        state: Info,
        block_size: usize,
    ) -> Self {
        Self::with_block_size_impl(Box::from(reader), state, block_size)
    }

    /// Returns a copy of the current encoding state.
    pub fn state(&self) -> Info {
        self.state
    }

    fn with_block_size_impl(
        reader: Box<dyn ReadSamples<'s, Format = PcmS16Le> + 'r>,
        state: Info,
        block_size: usize,
    ) -> Self {
        let block_size_aligned = block_size & !(BYTES_PER_FRAME - 1);
        assert!(block_size_aligned > 0, "block size is too small");
        let total_samples = reader.data_remaining().and_then(NonZeroU64::new);
        Self {
            reader,
            block_size: block_size_aligned,
            sample_rate: 0,
            samples_encoded: 0,
            total_samples,
            frame: ArrayVec::new(),
            buffer: VecDeque::new(),
            sample_offset: 0,
            block: Block { data: vec![], len: 0, initial_state: state },
            state,
            done: false,
        }
    }

    fn encode(&mut self, samples: &[i16]) {
        self.block.encode(samples, &mut self.state);
        self.samples_encoded += samples.len() as u64;
    }

    fn extend_frame(&mut self, samples: &[i16]) {
        self.frame.try_extend_from_slice(samples).expect("cannot fit samples in the frame buffer");
    }

    fn encode_more(&mut self) -> Option<Block> {
        while let Some(mut samples) = self.buffer.pop_front() {
            let mut offset = mem::take(&mut self.sample_offset);

            // If the last sample packet produced a partial frame, append the samples from the
            // beginning of this packet to see if we can complete it. We can only encode a partial
            // frame if it is the last frame (see `finish()`).
            if !self.frame.is_empty() {
                let num_samples = samples.len.min(SAMPLES_PER_FRAME - self.frame.len());
                self.extend_frame(&samples.data[offset..(offset + num_samples)]);
                offset += num_samples;
                samples.len -= num_samples;
                if self.frame.len() == SAMPLES_PER_FRAME {
                    let frame = mem::take(&mut self.frame);
                    self.encode(&frame);
                }
            }

            // Encode as many whole frames as possible until we either run out or complete a block
            while samples.len >= SAMPLES_PER_FRAME && self.block.data.len() < self.block_size {
                let remaining_frames = (self.block_size - self.block.data.len()) / BYTES_PER_FRAME;
                let available_frames = samples.len / SAMPLES_PER_FRAME;
                let num_frames = remaining_frames.min(available_frames);
                let num_samples = num_frames * SAMPLES_PER_FRAME;
                self.encode(&samples.data[offset..(offset + num_samples)]);
                offset += num_samples;
                samples.len -= num_samples;
            }

            debug_assert!(self.block.data.len() <= self.block_size);
            if self.block.data.len() == self.block_size {
                // A full block is complete
                let block = mem::take(&mut self.block);
                self.block.initial_state = self.state;
                if samples.len > 0 {
                    // There are still samples left in this packet
                    self.buffer.push_front(samples);
                    self.sample_offset = offset;
                }
                return Some(block);
            }

            // We have a partial frame; save any unencoded samples and keep going
            self.extend_frame(&samples.data[offset..]);
        }
        None
    }

    fn finish(&mut self) -> Option<Block> {
        if !self.frame.is_empty() {
            // The last frame is allowed to be incomplete because there will be no more samples
            let frame = mem::take(&mut self.frame);
            self.encode(&frame);
        }
        if self.block.len > 0 {
            Some(mem::take(&mut self.block))
        } else {
            None
        }
    }
}

impl<'s> ReadSamples<'s> for Encoder<'_, 's> {
    type Format = GcAdpcm;

    #[instrument(level = "trace", name = "Encoder", skip_all)]
    fn read_samples(&mut self) -> Result<Option<Samples<'static, Self::Format>>> {
        let block = loop {
            if let Some(block) = self.encode_more() {
                break Some(block);
            } else if self.done {
                return Ok(None);
            } else if let Some(samples) = self.reader.read_samples()? {
                if samples.rate == 0 {
                    return Err(Error::InvalidSampleRate(samples.rate));
                } else if self.sample_rate == 0 {
                    self.sample_rate = samples.rate;
                } else if samples.rate != self.sample_rate {
                    return Err(Error::InconsistentSampleRate);
                }
                self.buffer.push_front(samples);
            } else {
                self.done = true;
                break self.finish();
            }
        };
        Ok(block.map(|b| b.finish(self.sample_rate, self.block_size)))
    }

    fn format(&self) -> Format {
        Self::Format::FORMAT
    }

    fn tag(&self) -> &SourceTag {
        self.reader.tag()
    }

    fn progress(&self) -> Option<ProgressHint> {
        if let Some(total_samples) = self.total_samples {
            ProgressHint::new(self.samples_encoded, total_samples.get())
        } else {
            self.reader.progress()
        }
    }

    fn data_remaining(&self) -> Option<u64> {
        let samples_per_frame = SAMPLES_PER_FRAME as u64;
        let num_samples = self.total_samples?.get() - self.samples_encoded;
        let num_frames = (num_samples + samples_per_frame - 1) / samples_per_frame;
        Some(num_samples + num_frames * 2)
    }

    fn cues(&self) -> Box<dyn Iterator<Item = crate::audio::Cue> + '_> {
        self.reader.cues()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::format::adpcm::FrameContext;
    use crate::audio::Result;
    use crate::test;

    const EXPECTED_LEN: usize = 0x30af9;

    #[test]
    fn test_calculate_coefficients() -> Result<()> {
        let data = test::open_test_wav();
        let num_samples = data.len() / 2;
        let mut left_samples = Vec::with_capacity(num_samples);
        let mut right_samples = Vec::with_capacity(num_samples);
        for samples in data.chunks(2) {
            left_samples.push(samples[0]);
            right_samples.push(samples[1]);
        }
        let left_coefficients = CoefficientCalculator::calculate(&left_samples);
        let right_coefficients = CoefficientCalculator::calculate(&right_samples);
        assert_eq!(left_coefficients, test::TEST_WAV_LEFT_COEFFICIENTS);
        assert_eq!(right_coefficients, test::TEST_WAV_RIGHT_COEFFICIENTS);
        Ok(())
    }

    #[test]
    fn test_calculate_coefficients_chunked() -> Result<()> {
        const CHUNK_SIZE: usize = 1000;

        let data = test::open_test_wav();
        let num_samples = data.len() / 2;
        let mut left_samples = Vec::with_capacity(num_samples);
        let mut right_samples = Vec::with_capacity(num_samples);
        for samples in data.chunks(2) {
            left_samples.push(samples[0]);
            right_samples.push(samples[1]);
        }

        let mut left_calc = CoefficientCalculator::new();
        for samples in left_samples.chunks(CHUNK_SIZE) {
            left_calc.process(samples);
        }
        let left_coefficients = left_calc.finish();

        let mut right_calc = CoefficientCalculator::new();
        for samples in right_samples.chunks(CHUNK_SIZE) {
            right_calc.process(samples);
        }
        let right_coefficients = right_calc.finish();

        assert_eq!(left_coefficients, test::TEST_WAV_LEFT_COEFFICIENTS);
        assert_eq!(right_coefficients, test::TEST_WAV_RIGHT_COEFFICIENTS);
        Ok(())
    }

    #[test]
    fn test_encode() -> Result<()> {
        let data = test::open_test_wav();
        let samples = Samples::<PcmS16Le>::from_pcm(&data, 2, 44100);
        let total_samples = data.len() as u64 / 2;

        let splitter = samples.into_reader("test").split_channels();
        let left_state =
            Info { coefficients: test::TEST_WAV_LEFT_COEFFICIENTS, ..Default::default() };
        let right_state =
            Info { coefficients: test::TEST_WAV_RIGHT_COEFFICIENTS, ..Default::default() };
        let mut left_encoder = Encoder::new(splitter.left(), left_state);
        let mut right_encoder = Encoder::new(splitter.right(), right_state);
        assert_eq!(left_encoder.progress(), ProgressHint::new(0, total_samples));
        assert_eq!(right_encoder.progress(), ProgressHint::new(0, total_samples));
        assert_eq!(left_encoder.data_remaining(), Some(EXPECTED_LEN as u64));
        assert_eq!(right_encoder.data_remaining(), Some(EXPECTED_LEN as u64));

        let left = left_encoder.read_samples()?.unwrap();
        assert!(left_encoder.read_samples()?.is_none());
        assert_eq!(left.params.coefficients, test::TEST_WAV_LEFT_COEFFICIENTS);

        let right = right_encoder.read_samples()?.unwrap();
        assert!(right_encoder.read_samples()?.is_none());
        assert_eq!(right.params.coefficients, test::TEST_WAV_RIGHT_COEFFICIENTS);

        assert_eq!(left_encoder.progress(), ProgressHint::new(total_samples, total_samples));
        assert_eq!(left_encoder.data_remaining(), Some(0));
        assert_eq!(
            left.params.context,
            FrameContext { predictor_and_scale: 0x75, last_samples: [0; 2] }
        );
        assert_eq!(left.len, EXPECTED_LEN);
        assert_eq!(left.channels, 1);
        assert_eq!(left.rate, 44100);
        assert!(left.data == test::TEST_WAV_LEFT_DSP);

        assert_eq!(right_encoder.progress(), ProgressHint::new(total_samples, total_samples));
        assert_eq!(right_encoder.data_remaining(), Some(0));
        assert_eq!(
            right.params.context,
            FrameContext { predictor_and_scale: 0x16, last_samples: [0; 2] }
        );
        assert_eq!(right.len, EXPECTED_LEN);
        assert_eq!(right.channels, 1);
        assert_eq!(right.rate, 44100);
        assert!(right.data == test::TEST_WAV_RIGHT_DSP);

        Ok(())
    }

    #[test]
    fn test_encode_in_blocks() -> Result<()> {
        const BLOCK_SIZE: usize = 0x8000;
        const EXPECTED_NUM_BLOCKS: usize = 4;
        const EXPECTED_BLOCK_LENGTHS: &[usize] = &[0x8000, 0x8000, 0x8000, 0x57d];
        const EXPECTED_END_ADDRESSES: &[usize] = &[0xffff, 0xffff, 0xffff, 0xaf8];
        const EXPECTED_LAST_SAMPLES: &[[i16; 2]] =
            &[[0, 0], [-5232, -5240], [1236, 1218], [33, 42]];

        let data = test::open_test_wav();
        let total_samples = data.len() as u64 / 2;
        let mut samples = vec![];
        for chunk in data.chunks(1000) {
            samples.push(Samples::from_pcm(chunk, 2, 44100));
        }

        let splitter = ReadSampleList::new(samples, "test").split_channels();
        let state = Info { coefficients: test::TEST_WAV_LEFT_COEFFICIENTS, ..Default::default() };
        let mut encoder = Encoder::with_block_size(splitter.left(), state, BLOCK_SIZE);

        let mut blocks = vec![];
        let mut expected_remaining = EXPECTED_LEN as u64;
        assert_eq!(encoder.progress(), ProgressHint::new(0, total_samples));
        assert_eq!(encoder.data_remaining(), Some(expected_remaining));
        while let Some(block) = encoder.read_samples()? {
            expected_remaining -= block.len as u64;
            let expected_samples = total_samples - expected_remaining * 14 / 16;
            assert_eq!(encoder.progress(), ProgressHint::new(expected_samples, total_samples));
            assert_eq!(encoder.data_remaining(), Some(expected_remaining));
            blocks.push(block);
        }
        assert_eq!(blocks.len(), EXPECTED_NUM_BLOCKS);

        let mut offset = 0;
        for (i, block) in blocks.iter().enumerate() {
            let end_offset = offset + block.data.len();
            assert_eq!(block.params.coefficients, test::TEST_WAV_LEFT_COEFFICIENTS);
            assert_eq!(block.params.context.predictor_and_scale, block.data[0] as u16);
            assert_eq!(block.params.context.last_samples, EXPECTED_LAST_SAMPLES[i]);
            assert_eq!(block.len, EXPECTED_END_ADDRESSES[i] + 1);
            assert_eq!(block.data, &test::TEST_WAV_LEFT_DSP[offset..end_offset]);
            assert_eq!(block.data.len(), EXPECTED_BLOCK_LENGTHS[i]);
            offset = end_offset;
        }
        Ok(())
    }
}
