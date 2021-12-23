use super::{GcAdpcm, BYTES_PER_FRAME, SAMPLES_PER_FRAME};
use crate::audio::format::{PcmS16Le, StaticFormat};
use crate::audio::{Cue, Format, ProgressHint, ReadSamples, Result, Samples, SourceTag};
use crate::common::clamp_i16;
use tracing::{instrument, trace};

/// Decodes GameCube ADPCM samples into PCM.
pub struct Decoder<'r, 's> {
    source: Box<dyn ReadSamples<'s, Format = GcAdpcm> + 'r>,
}

impl<'r, 's> Decoder<'r, 's> {
    /// Creates a new `Decoder` which reads samples from `source`.
    pub fn new(source: impl ReadSamples<'s, Format = GcAdpcm> + 'r) -> Self {
        Self { source: Box::from(source) }
    }
}

impl<'s> ReadSamples<'s> for Decoder<'_, 's> {
    type Format = PcmS16Le;

    #[allow(clippy::verbose_bit_mask)]
    #[instrument(level = "trace", name = "Decoder", skip_all)]
    fn read_samples(&mut self) -> Result<Option<Samples<'s, Self::Format>>> {
        let encoded = match self.source.read_samples() {
            Ok(Some(s)) => s,
            Ok(None) => return Ok(None),
            Err(e) => return Err(e),
        };

        let data = &*encoded.data; // This makes debug builds faster...
        let info = encoded.params;
        let mut context = info.context;
        trace!(
            "Decoding ADPCM block from {:?}: len={:#x} ps={:#x} s0={:#x} s1={:#x}",
            self.tag(),
            data.len(),
            context.predictor_and_scale,
            context.last_samples[0],
            context.last_samples[1],
        );

        // Estimate the final sample count based on how many bytes there are
        let num_bytes = (encoded.len + 1) / 2;
        let estimated = (num_bytes + BYTES_PER_FRAME - 1) / BYTES_PER_FRAME * SAMPLES_PER_FRAME;
        let mut decoded: Vec<i16> = Vec::with_capacity(estimated);

        let mut address = 2; // Skip the first predictor/scale byte
        let end_address = encoded.len - 1;
        while address <= end_address {
            if address & 0xf == 0 {
                // Frames are aligned on 16-byte boundaries and each begins with a new
                // predictor_and_scale byte
                context.predictor_and_scale = data[address / 2] as u16;
                address += 2;
                continue;
            }

            // Read next nibble
            let mut val = if address % 2 == 0 {
                (data[address / 2] >> 4) as i32
            } else {
                (data[address / 2] & 0xf) as i32
            };
            if val >= 8 {
                val -= 16; // Sign-extend
            }
            address += 1;

            let predictor = context.predictor();
            let c0 = info.coefficients[predictor * 2] as i32;
            let c1 = info.coefficients[predictor * 2 + 1] as i32;
            let s0 = context.last_samples[0] as i32;
            let s1 = context.last_samples[1] as i32;
            let predicted = (c0 * s0 + c1 * s1 + 0x400) >> 11;
            let pcm = clamp_i16(predicted + val * context.scale());

            decoded.push(pcm);
            context.push_sample(pcm);
        }
        debug_assert!(estimated >= decoded.len());
        Ok(Some(Samples::from_pcm(decoded, 1, encoded.rate)))
    }

    fn format(&self) -> Format {
        Self::Format::FORMAT
    }

    fn tag(&self) -> &SourceTag {
        self.source.tag()
    }

    fn progress(&self) -> Option<ProgressHint> {
        self.source.progress()
    }

    fn data_remaining(&self) -> Option<u64> {
        self.source.data_remaining().map(|len| {
            let bytes = (len + 1) / 2;
            let bytes_per_frame = BYTES_PER_FRAME as u64;
            let num_frames = (bytes + bytes_per_frame - 1) / bytes_per_frame;
            len - len.min(num_frames * 2)
        })
    }

    fn cues(&self) -> Box<dyn Iterator<Item = Cue> + '_> {
        self.source.cues()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::format::adpcm::{Coefficients, FrameContext, Info};
    use crate::audio::format::ReadWriteBytes;

    /// 128-sample sine wave encoded to GameCube ADPCM
    const SINE_ENCODED: &[u8] = &[
        0x18, 0x06, 0x1f, 0x1f, 0x01, 0xf1, 0xf1, 0xf0, 0x15, 0x6e, 0xf0, 0x00, 0x01, 0xf0, 0x00,
        0x00, 0x11, 0x59, 0x5b, 0x4b, 0x30, 0xe0, 0x1e, 0x3c, 0x10, 0x7b, 0x20, 0xd0, 0x10, 0x1f,
        0x00, 0x1f, 0x11, 0x2d, 0x3d, 0x2f, 0x2f, 0x02, 0xe4, 0xb7, 0x11, 0xa4, 0xe3, 0xe2, 0x00,
        0x02, 0xf1, 0xf1, 0x10, 0xf2, 0x3a, 0x79, 0x6e, 0x3d, 0x3c, 0x4e, 0x10, 0x01, 0xe2, 0x00,
        0xf1, 0xe3, 0xde, 0x4b, 0x11, 0x3d, 0x2e, 0x2f, 0xf0, 0x02, 0xc4, 0xc1, 0x0b, 0x44,
    ];

    /// Sine wave decoding coefficients
    const SINE_COEFFICIENTS: Coefficients = [
        0x055f, -0x0008, 0x0ff8, -0x0800, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
        0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
    ];

    /// Sine wave decoded as PCMS16LE, checked using other tools
    const SINE_DECODED: &[u8] = &[
        0x00, 0x00, 0x00, 0x06, 0xfa, 0x0c, 0xe7, 0x12, 0xc1, 0x19, 0x81, 0x1f, 0x21, 0x25, 0x9c,
        0x2b, 0xeb, 0x30, 0x09, 0x37, 0xf0, 0x3b, 0x9b, 0x41, 0x04, 0x46, 0x27, 0x4a, 0xc0, 0x4e,
        0xca, 0x52, 0x61, 0x56, 0xa2, 0x59, 0x89, 0x5c, 0x13, 0x5f, 0x3e, 0x61, 0x28, 0x63, 0x8f,
        0x64, 0x91, 0x65, 0x2d, 0x66, 0x63, 0x66, 0x33, 0x66, 0x9d, 0x65, 0xab, 0x64, 0x46, 0x63,
        0x88, 0x61, 0x5e, 0x5f, 0xdd, 0x5c, 0xf5, 0x59, 0xb9, 0x56, 0x26, 0x53, 0x3c, 0x4f, 0x03,
        0x4b, 0x81, 0x46, 0xb4, 0x41, 0xab, 0x3c, 0x5d, 0x37, 0xdf, 0x31, 0x2a, 0x2c, 0x4b, 0x26,
        0x46, 0x20, 0x1e, 0x1a, 0xdc, 0x13, 0x87, 0x0d, 0x24, 0x07, 0xbb, 0x00, 0x50, 0xfa, 0xeb,
        0xf3, 0x92, 0xed, 0x4c, 0xe7, 0x1e, 0xe1, 0x13, 0xdb, 0x27, 0xd5, 0x6c, 0xcf, 0xdc, 0xc9,
        0x86, 0xc4, 0x69, 0xbf, 0x91, 0xba, 0xfc, 0xb5, 0xb1, 0xb1, 0xb8, 0xad, 0x0d, 0xaa, 0xc0,
        0xa6, 0xc2, 0xa3, 0x2e, 0xa1, 0xed, 0x9e, 0x15, 0x9d, 0x9c, 0x9b, 0x8d, 0x9a, 0xdf, 0x99,
        0x9b, 0x99, 0xbd, 0x99, 0x45, 0x9a, 0x33, 0x9b, 0x8a, 0x9c, 0x42, 0x9e, 0x5e, 0xa0, 0xd8,
        0xa2, 0xb1, 0xa5, 0xe3, 0xa8, 0x6e, 0xac, 0x50, 0xb0, 0x7c, 0xb4, 0xfb, 0xb8, 0xba, 0xbd,
        0xc1, 0xc2, 0x03, 0xc8, 0x80, 0xcd, 0x2d, 0xd3, 0x0a, 0xd9, 0x0a, 0xdf, 0x2f, 0xe5, 0x6d,
        0xeb, 0xc0, 0xf1, 0x22, 0xf8, 0x8a, 0xfe, 0xf5, 0x04, 0x5b, 0x0b, 0xb6, 0x11, 0xfe, 0x17,
        0x2f, 0x1e, 0x40, 0x24, 0x30, 0x2a, 0xf3, 0x2f, 0x84, 0x35, 0xe3, 0x3a, 0x02, 0x40, 0xe7,
        0x44, 0x81, 0x49, 0xd5, 0x4d, 0xd7, 0x51, 0x8b, 0x55, 0xe7, 0x58, 0xe8, 0x5b, 0x8d, 0x5e,
        0xd3, 0x60, 0xbc, 0x62, 0x3a, 0x64, 0x5c, 0x65, 0x11, 0x66, 0x62, 0x66, 0x57, 0x64, 0xf8,
        0x62,
    ];

    #[test]
    fn test_decode_sine() -> Result<()> {
        let encoded = Samples::<'_, GcAdpcm> {
            channels: 1,
            rate: 44100,
            len: 0x94,
            data: SINE_ENCODED.into(),
            params: Info {
                coefficients: SINE_COEFFICIENTS,
                gain: 0,
                context: FrameContext { predictor_and_scale: 0x18, last_samples: [0, 0] },
            },
        };

        let expected = PcmS16Le::read_bytes(SINE_DECODED)?;
        let mut decoder = Decoder::new(encoded.into_reader("test"));
        assert_eq!(decoder.data_remaining(), Some(expected.len() as u64));
        let decoded = decoder.read_samples()?.unwrap();
        assert_eq!(decoder.data_remaining(), Some(0));
        assert_eq!(decoded.data, expected);

        Ok(())
    }
}
