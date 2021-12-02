use super::vgaudio::coefficients::{self, PcmHistory, Vec3};
use super::vgaudio::encode;
use super::{Coefficients, GcAdpcm, Info, BYTES_PER_FRAME, SAMPLES_PER_FRAME};
use crate::audio::format::{PcmS16Le, StaticFormat};
use crate::audio::{Error, Format, ProgressHint, ReadSamples, Result, Samples, SourceTag};
use tracing::{debug, instrument, trace, trace_span};

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

/// Encodes raw PCM data into GameCube ADPCM format.
pub struct Encoder<'r, 's> {
    /// The inner reader to read samples from.
    reader: Box<dyn ReadSamples<'s, Format = PcmS16Le> + 'r>,
    /// The raw PCM samples to encode.
    pcm: Vec<i16>,
    /// The sample rate.
    sample_rate: u32,
    /// The index of the next sample to start encoding from.
    pos: usize,
    /// The size of each block in bytes.
    block_size: usize,
    /// The current encoding state.
    state: Info,
}

impl<'r, 's> Encoder<'r, 's> {
    /// Creates an `Encoder` which reads samples from `reader`.
    pub fn new(reader: impl ReadSamples<'s, Format = PcmS16Le> + 'r) -> Self {
        Self::with_block_size_impl(Box::from(reader), usize::MAX)
    }

    /// Creates an `Encoder` which reads samples from `reader` and outputs blocks of data which are
    /// no larger than `block_size`.
    pub fn with_block_size(
        reader: impl ReadSamples<'s, Format = PcmS16Le> + 'r,
        block_size: usize,
    ) -> Self {
        Self::with_block_size_impl(Box::from(reader), block_size)
    }

    fn with_block_size_impl(
        reader: Box<dyn ReadSamples<'s, Format = PcmS16Le> + 'r>,
        block_size: usize,
    ) -> Self {
        let block_size_aligned = block_size & !(BYTES_PER_FRAME - 1);
        assert!(block_size_aligned > 0, "block size is too small");
        Self {
            reader,
            pcm: vec![],
            sample_rate: 0,
            pos: 0,
            block_size: block_size_aligned,
            state: Info::default(),
        }
    }

    #[instrument(level = "trace", skip_all)]
    fn start_encoding(&mut self) -> Result<()> {
        while let Some(samples) = self.reader.read_samples()? {
            if samples.channels != 1 {
                return Err(Error::StreamNotMono);
            }
            if self.pcm.is_empty() {
                self.sample_rate = samples.rate;
            } else if samples.rate != self.sample_rate {
                return Err(Error::InconsistentSampleRate);
            }
            self.pcm.extend(&*samples.data);
        }
        if self.pcm.is_empty() {
            return Ok(());
        }

        debug!(
            "Calculating ADPCM coefficients over {} samples from {:?}",
            self.pcm.len(),
            self.tag()
        );
        self.state.coefficients = CoefficientCalculator::calculate(&self.pcm);
        debug!(
            "Encoder context initialized (block size = {:#x}, coefficients = {:?})",
            self.block_size, self.state.coefficients
        );
        Ok(())
    }
}

impl<'s> ReadSamples<'s> for Encoder<'_, 's> {
    type Format = GcAdpcm;

    #[instrument(level = "trace", name = "Encoder", skip_all)]
    fn read_samples(&mut self) -> Result<Option<Samples<'static, Self::Format>>> {
        if self.pcm.is_empty() {
            self.start_encoding()?;
        }
        if self.pos >= self.pcm.len() {
            return Ok(None);
        }

        let start = self.pos;
        let remaining = self.pcm.len() - start;
        let remaining_frames = (remaining + SAMPLES_PER_FRAME - 1) / SAMPLES_PER_FRAME;
        let num_frames = remaining_frames.min(self.block_size / BYTES_PER_FRAME);
        let num_samples = (num_frames * SAMPLES_PER_FRAME).min(remaining);
        let end = start + num_samples;

        trace!("Encoding {} samples to ADPCM", num_samples);
        let mut initial_state = self.state;
        let bytes = trace_span!("encode")
            .in_scope(|| encode::encode(&self.pcm[start..end], &mut self.state));
        initial_state.context.predictor_and_scale = bytes[0] as u16;
        self.pos = end;

        Ok(Some(Samples {
            channels: 1,
            rate: self.sample_rate,
            len: bytes.len() * 2 - num_samples % 2,
            data: bytes.into(),
            params: initial_state,
        }))
    }

    fn format(&self) -> Format {
        Self::Format::FORMAT
    }

    fn tag(&self) -> &SourceTag {
        self.reader.tag()
    }

    fn progress(&self) -> Option<ProgressHint> {
        if self.pcm.is_empty() {
            return None; // Not initialized yet, so we don't know
        }
        let current_frame = (self.pos + SAMPLES_PER_FRAME - 1) / SAMPLES_PER_FRAME;
        let total_frames = (self.pcm.len() + SAMPLES_PER_FRAME - 1) / SAMPLES_PER_FRAME;
        let frames_per_block = total_frames.min(self.block_size / BYTES_PER_FRAME);
        let current = (current_frame + frames_per_block - 1) / frames_per_block;
        let total = (total_frames + frames_per_block - 1) / frames_per_block;
        ProgressHint::new(current as u64, total as u64)
    }

    fn data_remaining(&self) -> Option<u64> {
        let samples_per_frame = SAMPLES_PER_FRAME as u64;
        let num_samples = if !self.pcm.is_empty() {
            (self.pcm.len() - self.pos) as u64
        } else {
            self.reader.data_remaining()?
        };
        let num_frames = (num_samples + samples_per_frame - 1) / samples_per_frame;
        Some(num_samples + num_frames * 2)
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
        let samples = Samples::<PcmS16Le>::from_pcm(data, 2, 44100);

        let splitter = samples.into_reader("test").split_channels();
        let mut left_encoder = Encoder::new(splitter.left());
        let mut right_encoder = Encoder::new(splitter.right());
        assert_eq!(left_encoder.data_remaining(), Some(EXPECTED_LEN as u64));
        assert_eq!(right_encoder.data_remaining(), Some(EXPECTED_LEN as u64));

        let left = left_encoder.read_samples()?.unwrap();
        assert_eq!(left.params.coefficients, test::TEST_WAV_LEFT_COEFFICIENTS);
        let right = right_encoder.read_samples()?.unwrap();
        assert_eq!(right.params.coefficients, test::TEST_WAV_RIGHT_COEFFICIENTS);

        assert_eq!(left_encoder.progress(), ProgressHint::new(1, 1));
        assert_eq!(left_encoder.data_remaining(), Some(0));
        assert_eq!(
            left.params.context,
            FrameContext { predictor_and_scale: 0x75, last_samples: [0; 2] }
        );
        assert_eq!(left.len, EXPECTED_LEN);
        assert_eq!(left.channels, 1);
        assert_eq!(left.rate, 44100);
        assert!(left.data == test::TEST_WAV_LEFT_DSP);

        assert_eq!(right_encoder.progress(), ProgressHint::new(1, 1));
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
        const EXPECTED_NUM_BLOCKS: u64 = 4;
        const EXPECTED_BLOCK_LENGTHS: &[usize] = &[0x8000, 0x8000, 0x8000, 0x57d];
        const EXPECTED_END_ADDRESSES: &[usize] = &[0xffff, 0xffff, 0xffff, 0xaf8];
        const EXPECTED_LAST_SAMPLES: &[[i16; 2]] =
            &[[0, 0], [-5232, -5240], [1236, 1218], [33, 42]];

        let data = test::open_test_wav();
        let samples = Samples::<PcmS16Le>::from_pcm(&data, 2, 44100);

        let splitter = samples.into_reader("test").split_channels();
        let mut encoder = Encoder::with_block_size(splitter.left(), BLOCK_SIZE);

        let mut blocks = vec![];
        let mut i = 0;
        let mut expected_remaining =
            EXPECTED_END_ADDRESSES.iter().skip(i as usize).map(|&a| a as u64 + 1).sum();
        assert_eq!(encoder.progress(), None);
        assert_eq!(encoder.data_remaining(), Some(expected_remaining));
        while let Some(block) = encoder.read_samples()? {
            i += 1;
            expected_remaining -= block.len as u64;
            assert_eq!(encoder.progress(), ProgressHint::new(i, EXPECTED_NUM_BLOCKS));
            assert_eq!(encoder.data_remaining(), Some(expected_remaining));
            blocks.push(block);
        }
        assert_eq!(blocks.len(), 4);

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
