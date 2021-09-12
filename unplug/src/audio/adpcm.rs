mod coefficients;
mod decode;
mod encode;

pub use coefficients::*;
pub use decode::*;
pub use encode::*;

use super::format::StaticFormat;
use super::{Error, Format, Result};
use crate::common::ReadFrom;
use byteorder::{ReadBytesExt, BE};
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
    pub coefficients: Coefficients,
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
