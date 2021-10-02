/*
 * This ADPCM encoding implementation is derived from VGAudio, obtained at
 * <https://github.com/Thealexbarney/VGAudio/>.
 *
 * The MIT License (MIT)
 *
 * Copyright (c) 2016
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */

use super::{Coefficients, GcAdpcm, Info};
use crate::audio::adpcm::calculate_coefficients;
use crate::audio::format::PcmS16Le;
use crate::audio::{Error, ReadSamples, Result, Samples};
use crate::common::clamp_i16;
use byteorder::{ByteOrder, LE};
use log::{debug, trace};

const SAMPLES_PER_FRAME: usize = 14;
const BYTES_PER_FRAME: usize = 8;

fn encode(pcm: &[i16], info: &mut Info) -> Vec<u8> {
    let num_frames = (pcm.len() + SAMPLES_PER_FRAME - 1) / SAMPLES_PER_FRAME;
    let mut adpcm = Vec::with_capacity(num_frames * BYTES_PER_FRAME);

    let mut adpcm_buf = [0; BYTES_PER_FRAME];
    let mut pcm_buf = [0; SAMPLES_PER_FRAME + 2];
    pcm_buf[0] = info.context.last_samples[1];
    pcm_buf[1] = info.context.last_samples[0];

    // Encode frame-by-frame
    for samples in pcm.chunks(SAMPLES_PER_FRAME) {
        // The first two pcm_buf values are from the last frame, the rest are from this frame
        let end = samples.len() + 2;
        pcm_buf[2..end].copy_from_slice(samples);
        pcm_buf[end..].fill(0);

        // Write ADPCM bytes to adpcm_buf and update pcm_buf with the re-decoded samples
        encode_frame(&mut pcm_buf, &info.coefficients, &mut adpcm_buf);

        // Copy the last two samples back to the beginning
        pcm_buf.copy_within((end - 2)..end, 0);

        // Append the encoded frame and discard unused bytes
        let frame_size = 1 + (samples.len() + 1) / 2;
        adpcm.extend(&adpcm_buf[..frame_size]);
    }

    info.context.last_samples[1] = pcm_buf[0];
    info.context.last_samples[0] = pcm_buf[1];
    adpcm
}

fn encode_frame(pcm: &mut [i16], coefs: &Coefficients, out: &mut [u8]) {
    // Encode using all possible coefficient pairs
    let mut frames = [Frame::default(); 8];
    for (c, frame) in coefs.chunks(2).zip(&mut frames) {
        *frame = try_coefficients(pcm, c[0].into(), c[1].into());
    }

    // Take the closest one
    let mut best_index = 0;
    for (i, encoding) in frames.iter().enumerate().skip(1) {
        if encoding.distance < frames[best_index].distance {
            best_index = i;
        }
    }
    let best = &frames[best_index];
    pcm[2..].copy_from_slice(&best.pcm[2..]);

    // Frames start with the predictor/scale byte
    let predictor = (best_index as u8) & 0xf;
    let scale = (best.power as u8) & 0xf;
    out[0] = (predictor << 4) | scale;

    // Pack samples into nibbles
    for (adpcm, o) in best.adpcm.chunks(2).zip(&mut out[1..]) {
        *o = (((adpcm[0] as u8) & 0xf) << 4) | ((adpcm[1] as u8) & 0xf);
    }
}

#[derive(Copy, Clone, Default)]
struct Frame {
    pcm: [i16; SAMPLES_PER_FRAME + 2],
    adpcm: [i32; SAMPLES_PER_FRAME],
    power: i32,
    distance: f64,
}

fn try_coefficients(pcm: &[i16], c0: i32, c1: i32) -> Frame {
    assert!(pcm.len() >= 2 && pcm.len() <= SAMPLES_PER_FRAME + 2);

    let mut frame = Frame::default();
    frame.pcm[0] = pcm[0];
    frame.pcm[1] = pcm[1];

    let mut max_distance: i16 = 0;
    for s in pcm.windows(3) {
        let predicted = (c0 * (s[1] as i32) + c1 * (s[0] as i32)) / 2048;
        let distance = clamp_i16(s[2] as i32 - predicted);
        if distance == i16::MIN {
            max_distance = i16::MIN;
            break;
        } else if distance.abs() > max_distance.abs() {
            max_distance = distance;
        }
    }

    let mut power = 0;
    while power <= 12 && (max_distance > 7 || max_distance < -8) {
        max_distance /= 2;
        power += 1;
    }
    power = (power - 2).max(-1);

    loop {
        power += 1;
        let scale = (1 << power) * 2048;
        frame.distance = 0.0;
        let mut max_overflow = 0;

        for (s, adpcm) in frame.adpcm.iter_mut().enumerate() {
            let s0 = frame.pcm[s + 1] as i32;
            let s1 = frame.pcm[s] as i32;
            let predicted = s0 * c0 + s1 * c1;
            let distance = ((pcm[s + 2] as i32) * 2048) - predicted;

            let unclamped = if distance > 0 {
                ((distance as f32 / scale as f32) as f64 + 0.4999999) as i32
            } else {
                ((distance as f32 / scale as f32) as f64 - 0.4999999) as i32
            };

            let clamped = unclamped.max(-8).min(7);
            if clamped != unclamped {
                let overflow = (unclamped - clamped).abs();
                max_overflow = max_overflow.max(overflow);
            }
            *adpcm = clamped;
            frame.pcm[s + 2] = clamp_i16((predicted + clamped * scale + 0x400) >> 11);

            let actual_distance = (pcm[s + 2] as i32 - frame.pcm[s + 2] as i32) as f64;
            frame.distance += actual_distance * actual_distance;
        }

        let mut x = max_overflow + 8;
        while x > 256 {
            power = (power + 1).min(11);
            x >>= 1;
        }
        if power >= 12 || max_overflow <= 1 {
            break;
        }
    }

    frame.power = power;
    frame
}

/// Encodes raw PCM data into GameCube ADPCM format.
#[allow(single_use_lifetimes)]
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
    pub fn new<R>(reader: R) -> Self
    where
        R: ReadSamples<'s, Format = PcmS16Le> + 'r,
    {
        Self::with_block_size(reader, usize::MAX)
    }

    /// Creates an `Encoder` which reads samples from `reader` and outputs blocks of data which are
    /// no larger than `block_size`.
    pub fn with_block_size<R>(reader: R, block_size: usize) -> Self
    where
        R: ReadSamples<'s, Format = PcmS16Le> + 'r,
    {
        let block_size_aligned = block_size & !(BYTES_PER_FRAME - 1);
        assert!(block_size_aligned > 0, "block size is too small");
        Self {
            reader: Box::from(reader),
            pcm: vec![],
            pos: 0,
            block_size: block_size_aligned,
            state: Info::default(),
        }
    }

    fn start_encoding(&mut self) -> Result<()> {
        while let Some(samples) = self.reader.read_samples()? {
            if samples.channels != 1 {
                return Err(Error::StreamNotMono);
            }
            self.pcm.reserve(samples.bytes.len() / 2);
            for sample in samples.iter() {
                self.pcm.push(LE::read_i16(sample));
            }
        }
        if self.pcm.is_empty() {
            return Ok(());
        }

        debug!("Calculating ADPCM coefficients over {} samples", self.pcm.len());
        self.state.coefficients = calculate_coefficients(&self.pcm);
        trace!("coefficients = {:?}", self.state.coefficients);
        Ok(())
    }
}

impl ReadSamples<'static> for Encoder<'_, '_> {
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

        debug!("Encoding {} samples to ADPCM", num_samples);
        let mut initial_state = self.state;
        let bytes = encode(&self.pcm[start..end], &mut self.state);
        initial_state.context.predictor_and_scale = bytes[0] as u16;
        self.pos = end;

        Ok(Some(Samples {
            params: initial_state,
            start_address: 2,
            end_address: bytes.len() * 2 - num_samples % 2 - 1,
            channels: 1,
            bytes: bytes.into(),
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
        let bytes = test::open_test_wav().into_inner();
        let samples = Samples::<PcmS16Le> {
            params: (),
            start_address: 0,
            end_address: bytes.len() / 2 - 1,
            channels: 2,
            bytes: bytes.into(),
        };

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
        assert_eq!(left.start_address, 2);
        assert_eq!(left.end_address, 0x30af8);
        assert_eq!(left.channels, 1);
        assert!(left.bytes == test::TEST_WAV_LEFT_DSP);

        assert_eq!(
            right.params.context,
            FrameContext { predictor_and_scale: 0x16, last_samples: [0; 2] }
        );
        assert_eq!(right.start_address, 2);
        assert_eq!(right.end_address, 0x30af8);
        assert_eq!(right.channels, 1);
        assert!(right.bytes == test::TEST_WAV_RIGHT_DSP);

        Ok(())
    }

    #[test]
    fn test_encode_in_blocks() -> Result<()> {
        let bytes = test::open_test_wav().into_inner();
        let samples = Samples::<PcmS16Le> {
            params: (),
            start_address: 0,
            end_address: bytes.len() / 2 - 1,
            channels: 2,
            bytes: bytes.into(),
        };

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
            let end_offset = offset + block.bytes.len();
            assert_eq!(block.params.coefficients, test::TEST_WAV_LEFT_COEFFICIENTS);
            assert_eq!(block.params.context.predictor_and_scale, block.bytes[0] as u16);
            assert_eq!(block.params.context.last_samples, EXPECTED_LAST_SAMPLES[i]);
            assert_eq!(block.start_address, 0x2);
            assert_eq!(block.end_address, EXPECTED_END_ADDRESSES[i]);
            assert_eq!(block.bytes, &test::TEST_WAV_LEFT_DSP[offset..end_offset]);
            assert_eq!(block.bytes.len(), EXPECTED_BLOCK_LENGTHS[i]);
            offset = end_offset;
        }

        Ok(())
    }
}
