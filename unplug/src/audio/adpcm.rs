mod decode;
mod encode;
mod vgaudio;

pub use decode::*;
pub use encode::*;

use super::format::{ExtendSamples, FormatTag, ReadWriteBytes, StaticFormat};
use super::{Error, Format, Result};
use crate::common::{ReadFrom, WriteTo};
use byteorder::{ByteOrder, ReadBytesExt, WriteBytesExt, BE};
use std::borrow::Cow;
use std::io::{Read, Write};

const SAMPLES_PER_FRAME: usize = 14;
const BYTES_PER_FRAME: usize = 8;

#[derive(Copy, Clone)]
pub struct GcAdpcm;

impl FormatTag for GcAdpcm {
    type Data = u8;
    type Params = Info;
}

impl StaticFormat for GcAdpcm {
    fn format() -> Format {
        Format::GcAdpcm
    }
}

impl ExtendSamples for GcAdpcm {
    fn extend_samples(
        dest: &mut Cow<'_, [u8]>,
        dest_params: &mut Self::Params,
        src: &[u8],
        src_params: &Self::Params,
    ) -> Result<()> {
        if src_params.coefficients != dest_params.coefficients {
            return Err(Error::DifferentCoefficients);
        }
        dest.to_mut().extend(src);
        Ok(())
    }
}

impl ReadWriteBytes for GcAdpcm {
    fn read_bytes(mut reader: impl Read) -> Result<Vec<Self::Data>> {
        let mut bytes = vec![];
        reader.read_to_end(&mut bytes)?;
        Ok(bytes)
    }

    fn write_bytes(mut writer: impl Write, data: &[Self::Data]) -> Result<()> {
        writer.write_all(data)?;
        Ok(())
    }
}

/// GameCube ADPCM coefficients.
pub type Coefficients = [i16; 16];

/// GameCube ADPCM audio info.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct Info {
    /// ADPCM coefficients.
    pub coefficients: Coefficients,
    /// Gain level.
    pub gain: u16,
    /// The initial playback context.
    pub context: FrameContext,
}

impl<R: Read> ReadFrom<R> for Info {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let mut info = Info::default();
        reader.read_i16_into::<BE>(&mut info.coefficients)?;
        info.gain = reader.read_u16::<BE>()?;
        info.context = FrameContext::read_from(reader)?;
        Ok(info)
    }
}

impl<W: Write> WriteTo<W> for Info {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        let mut coefficient_bytes = [0u8; 32];
        BE::write_i16_into(&self.coefficients, &mut coefficient_bytes);
        writer.write_all(&coefficient_bytes)?;
        writer.write_u16::<BE>(self.gain)?;
        self.context.write_to(writer)?;
        Ok(())
    }
}

/// ADPCM context for an audio frame.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct FrameContext {
    /// This is used as a byte where the high nibble is the predictor (coefficient index) and the
    /// low nibble is the scale. Use `predictor()` and `scale()` to unpack this.
    pub predictor_and_scale: u16,
    /// Previously-decoded samples, where `last_samples[1]` is the oldest. Use `push_sample()` to
    /// insert new samples into this.
    pub last_samples: [i16; 2],
}

impl FrameContext {
    /// Creates a zero-initialized `FrameContext`.
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

impl<R: Read> ReadFrom<R> for FrameContext {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let predictor_and_scale = reader.read_u16::<BE>()?;
        let mut last_samples = [0i16; 2];
        reader.read_i16_into::<BE>(&mut last_samples)?;
        Ok(Self { predictor_and_scale, last_samples })
    }
}

impl<W: Write> WriteTo<W> for FrameContext {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_u16::<BE>(self.predictor_and_scale)?;
        writer.write_i16::<BE>(self.last_samples[0])?;
        writer.write_i16::<BE>(self.last_samples[1])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_write_and_read;
    use std::convert::TryInto;

    fn make_info(coefficients: Coefficients, predictor_and_scale: u16) -> Info {
        Info {
            coefficients,
            gain: 0,
            context: FrameContext { predictor_and_scale, last_samples: [0; 2] },
        }
    }

    #[test]
    fn test_write_and_read_info() {
        assert_write_and_read!(Info {
            coefficients: (1..=16).collect::<Vec<i16>>().try_into().unwrap(),
            gain: 1,
            context: FrameContext { predictor_and_scale: 2, last_samples: [3, 4] },
        });
    }

    #[test]
    fn test_extend_samples() {
        let mut dest = Cow::from(vec![0x17, 0x2, 0x3, 0x4, 0x5, 0x6, 0x7, 0x8]);
        let mut dest_params = make_info([0; 16], 0x17);
        let src: Vec<u8> = vec![0x27, 0x1];
        let src_params = make_info([1; 16], 0x27);
        assert!(matches!(
            GcAdpcm::extend_samples(&mut dest, &mut dest_params, &src, &src_params),
            Err(Error::DifferentCoefficients)
        ));

        let mut dest = Cow::from(vec![0x17, 0x2, 0x3, 0x4, 0x5, 0x6, 0x7, 0x8]);
        let mut dest_params = make_info([0; 16], 0x17);
        let src: Vec<u8> = vec![0x27, 0x1];
        let src_params = make_info([0; 16], 0x27);
        assert!(GcAdpcm::extend_samples(&mut dest, &mut dest_params, &src, &src_params).is_ok());
        assert_eq!(&*dest, &[0x17, 0x2, 0x3, 0x4, 0x5, 0x6, 0x7, 0x8, 0x27, 0x1]);
    }
}
