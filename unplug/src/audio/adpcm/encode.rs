use super::vgaudio::{calculate_coefficients, encode};
use super::{GcAdpcm, Info, BYTES_PER_FRAME, SAMPLES_PER_FRAME};
use crate::audio::format::PcmS16Le;
use crate::audio::{Error, ReadSamples, Result, Samples};
use log::{debug, trace};

/// Encodes raw PCM data into GameCube ADPCM format.
pub struct Encoder<'r, 's> {
    /// The inner reader to read samples from.
    reader: Box<dyn ReadSamples<'s, Format = PcmS16Le> + 'r>,
    /// The raw PCM samples to encode.
    pcm: Vec<i16>,
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
        Self { reader, pcm: vec![], pos: 0, block_size: block_size_aligned, state: Info::default() }
    }

    fn start_encoding(&mut self) -> Result<()> {
        while let Some(samples) = self.reader.read_samples()? {
            if samples.channels != 1 {
                return Err(Error::StreamNotMono);
            }
            self.pcm.extend(&*samples.data);
        }
        if self.pcm.is_empty() {
            return Ok(());
        }

        debug!("Calculating ADPCM coefficients over {} samples", self.pcm.len());
        self.state.coefficients = calculate_coefficients(&self.pcm);
        debug!(
            "Encoder context initialized (block size = {:#x}, coefficients = {:?})",
            self.block_size, self.state.coefficients
        );
        Ok(())
    }
}

impl<'s> ReadSamples<'s> for Encoder<'_, 's> {
    type Format = GcAdpcm;
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
        let bytes = encode(&self.pcm[start..end], &mut self.state);
        initial_state.context.predictor_and_scale = bytes[0] as u16;
        self.pos = end;

        Ok(Some(Samples {
            channels: 1,
            len: bytes.len() * 2 - num_samples % 2,
            data: bytes.into(),
            params: initial_state,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::adpcm::FrameContext;
    use crate::audio::sample::SplitChannels;
    use crate::audio::Result;
    use crate::test;

    #[test]
    fn test_encode() -> Result<()> {
        let data = test::open_test_wav();
        let samples = Samples::<PcmS16Le>::from_pcm(data, 2);

        let splitter = SplitChannels::new(samples.into_reader());
        let mut left_encoder = Encoder::new(splitter.left());
        let mut right_encoder = Encoder::new(splitter.right());

        let left = left_encoder.read_samples()?.unwrap();
        assert_eq!(left.params.coefficients, test::TEST_WAV_LEFT_COEFFICIENTS);
        let right = right_encoder.read_samples()?.unwrap();
        assert_eq!(right.params.coefficients, test::TEST_WAV_RIGHT_COEFFICIENTS);

        assert_eq!(
            left.params.context,
            FrameContext { predictor_and_scale: 0x75, last_samples: [0; 2] }
        );
        assert_eq!(left.len, 0x30af9);
        assert_eq!(left.channels, 1);
        assert!(left.data == test::TEST_WAV_LEFT_DSP);

        assert_eq!(
            right.params.context,
            FrameContext { predictor_and_scale: 0x16, last_samples: [0; 2] }
        );
        assert_eq!(right.len, 0x30af9);
        assert_eq!(right.channels, 1);
        assert!(right.data == test::TEST_WAV_RIGHT_DSP);

        Ok(())
    }

    #[test]
    fn test_encode_in_blocks() -> Result<()> {
        let data = test::open_test_wav();
        let samples = Samples::<PcmS16Le>::from_pcm(data, 2);

        let splitter = SplitChannels::new(samples.into_reader());
        let block_size = 0x8000;
        let mut encoder = Encoder::with_block_size(splitter.left(), block_size);
        let mut blocks = vec![];
        while let Some(block) = encoder.read_samples()? {
            blocks.push(block);
        }
        assert_eq!(blocks.len(), 4);

        const EXPECTED_BLOCK_LENGTHS: &[usize] = &[0x8000, 0x8000, 0x8000, 0x57d];
        const EXPECTED_END_ADDRESSES: &[usize] = &[0xffff, 0xffff, 0xffff, 0xaf8];
        const EXPECTED_LAST_SAMPLES: &[[i16; 2]] =
            &[[0, 0], [-5232, -5240], [1236, 1218], [33, 42]];

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
