use super::format::{PcmS16Le, StaticFormat};
use super::{Error, Format, ReadSamples, Result, Samples};
use crate::common::ReadFrom;
use byteorder::{ReadBytesExt, WriteBytesExt, BE, LE};
use log::trace;
use std::convert::TryInto;
use std::io::Read;

#[derive(Copy, Clone)]
pub struct GcAdpcm;
impl StaticFormat for GcAdpcm {
    type Context = Context;
    fn format_static() -> Format {
        Format::GcAdpcm
    }
}

/// GameCube ADPCM decoder info.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct Info {
    /// ADPCM coefficients.
    pub coefficients: [i16; 16],
    /// Audio gain level.
    pub gain: u16,
    /// Initial decoder parameters.
    pub context: Context,
}

impl<R: Read> ReadFrom<R> for Info {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let mut info = Info::default();
        reader.read_i16_into::<BE>(&mut info.coefficients)?;
        info.gain = reader.read_u16::<BE>()?;
        info.context = Context::read_from(reader)?;
        Ok(info)
    }
}

/// GameCube ADPCM decoder context.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct Context {
    /// This is used as a byte where the high nibble is the predictor (coefficient index) and the
    /// low nibble is the scale. Use `predictor()` and `scale()` to unpack this.
    pub predictor_and_scale: u16,
    /// Previously-decoded samples, where `last_samples[1]` is the oldest. Use `push_sample()` to
    /// insert new samples into this.
    pub last_samples: [i16; 2],
}

impl Context {
    /// Creates an empty `Context`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Unpacks the current predictor (coefficient index) value.
    pub fn predictor(&self) -> usize {
        ((self.predictor_and_scale >> 4) & 0x7) as usize
    }

    /// Unpacks the current scale value.
    pub fn scale(&self) -> i32 {
        1 << (self.predictor_and_scale & 0xf)
    }

    /// Pushes a new sample into the sample history, pushing out the oldest sample.
    pub fn push_sample(&mut self, sample: i16) {
        self.last_samples[1] = self.last_samples[0];
        self.last_samples[0] = sample;
    }
}

impl<R: Read> ReadFrom<R> for Context {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let predictor_and_scale = reader.read_u16::<BE>()?;
        let mut last_samples = [0i16; 2];
        reader.read_i16_into::<BE>(&mut last_samples)?;
        Ok(Self { predictor_and_scale, last_samples })
    }
}

/// Decodes GameCube ADPCM samples into PCM.
#[allow(single_use_lifetimes)]
pub struct Decoder<'a, 'b: 'a> {
    source: Box<dyn ReadSamples<'b, Format = GcAdpcm> + 'a>,
    coefficients: [i16; 16],
}

impl<'a, 'b: 'a> Decoder<'a, 'b> {
    /// Creates a new `Decoder` which reads samples from `source` and decodes them using
    /// `coefficients`.
    pub fn new(
        source: Box<dyn ReadSamples<'b, Format = GcAdpcm> + 'a>,
        coefficients: &[i16],
    ) -> Self {
        Self { source, coefficients: coefficients.try_into().expect("expected 16 coefficients") }
    }
}

impl ReadSamples<'static> for Decoder<'_, '_> {
    type Format = PcmS16Le;

    #[allow(clippy::verbose_bit_mask)]
    fn read_samples(&mut self) -> Result<Option<Samples<'static, Self::Format>>> {
        let encoded = match self.source.read_samples() {
            Ok(Some(s)) => s,
            Ok(None) => return Ok(None),
            Err(e) => return Err(e),
        };

        let mut context = encoded.context;
        trace!(
            "Decoding ADPCM block: len={:#x} ps={:#x} s0={:#x} s1={:#x}",
            encoded.bytes.len(),
            context.predictor_and_scale,
            context.last_samples[0],
            context.last_samples[1],
        );

        // There are 14 samples for every 16 bytes, so we can estimate the final sample count
        let estimated = ((encoded.end_address - encoded.start_address) * 14 + 15) / 16;
        let mut decoded: Vec<u8> = Vec::with_capacity(estimated * 2);

        let mut address = encoded.start_address;
        while address <= encoded.end_address {
            if address & 0xf == 0 {
                // Frames are aligned on 16-byte boundaries and each begins with a new
                // predictor_and_scale byte
                context.predictor_and_scale = encoded.bytes[address / 2] as u16;
                address += 2;
                continue;
            }

            // Read next nibble
            let mut val = if address % 2 == 0 {
                (encoded.bytes[address / 2] >> 4) as i32
            } else {
                (encoded.bytes[address / 2] & 0xf) as i32
            };
            if val >= 8 {
                val -= 16; // Sign-extend
            }
            address += 1;

            let predictor = context.predictor();
            let c0 = self.coefficients[predictor * 2] as i32;
            let c1 = self.coefficients[predictor * 2 + 1] as i32;
            let s0 = context.last_samples[0] as i32;
            let s1 = context.last_samples[1] as i32;
            let predicted = (c0 * s0 + c1 * s1 + 0x400) >> 11;

            let pcm32 = predicted + val * context.scale();
            let pcm16 = pcm32.max(i16::MIN as i32).min(i16::MAX as i32) as i16;

            decoded.write_i16::<LE>(pcm16)?;
            context.push_sample(pcm16);
        }

        Ok(Some(Samples {
            context: (),
            start_address: 0,
            end_address: decoded.len() / 2 - 1,
            channels: 1,
            bytes: decoded.into(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 128-sample sine wave encoded to GameCube ADPCM
    const SINE_ENCODED: &[u8] = &[
        0x18, 0x06, 0x1f, 0x1f, 0x01, 0xf1, 0xf1, 0xf0, 0x15, 0x6e, 0xf0, 0x00, 0x01, 0xf0, 0x00,
        0x00, 0x11, 0x59, 0x5b, 0x4b, 0x30, 0xe0, 0x1e, 0x3c, 0x10, 0x7b, 0x20, 0xd0, 0x10, 0x1f,
        0x00, 0x1f, 0x11, 0x2d, 0x3d, 0x2f, 0x2f, 0x02, 0xe4, 0xb7, 0x11, 0xa4, 0xe3, 0xe2, 0x00,
        0x02, 0xf1, 0xf1, 0x10, 0xf2, 0x3a, 0x79, 0x6e, 0x3d, 0x3c, 0x4e, 0x10, 0x01, 0xe2, 0x00,
        0xf1, 0xe3, 0xde, 0x4b, 0x11, 0x3d, 0x2e, 0x2f, 0xf0, 0x02, 0xc4, 0xc1, 0x0b, 0x44,
    ];

    /// Sine wave decoding coefficients
    const SINE_COEFFICIENTS: &[i16] = &[
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
        let encoded = Samples::<GcAdpcm> {
            context: Context { predictor_and_scale: 0x18, last_samples: [0, 0] },
            start_address: 0x2,
            end_address: 0x93,
            channels: 1,
            bytes: SINE_ENCODED.into(),
        };

        let reader = encoded.into_reader();
        let mut decoder = Decoder::new(Box::new(reader), SINE_COEFFICIENTS);
        let decoded = decoder.read_samples()?.unwrap();
        assert_eq!(decoded.bytes, SINE_DECODED);

        Ok(())
    }
}
