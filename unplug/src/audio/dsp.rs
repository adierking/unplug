use super::{Error, Format, Result};
use crate::common::ReadFrom;
use byteorder::{ReadBytesExt, BE};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::convert::TryFrom;
use std::io::Read;

/// GameCube DSP audio sample formats.
#[derive(Debug, Copy, Clone, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u16)]
pub enum DspFormat {
    /// GameCube ADPCM
    Adpcm = 0,
    /// 16-bit big endian PCM
    Pcm16 = 10,
    /// 8-bit PCM
    Pcm8 = 25,
}

impl Default for DspFormat {
    fn default() -> Self {
        Self::Adpcm
    }
}

impl<R: Read> ReadFrom<R> for DspFormat {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let id = reader.read_u16::<BE>()?;
        match Self::try_from(id) {
            Ok(format) => Ok(format),
            Err(_) => Err(Error::UnrecognizedSampleFormat(id)),
        }
    }
}

impl From<DspFormat> for Format {
    fn from(gc: DspFormat) -> Self {
        match gc {
            DspFormat::Adpcm => Self::GcAdpcm,
            DspFormat::Pcm16 => Self::PcmS16Be,
            DspFormat::Pcm8 => Self::PcmS8,
        }
    }
}

/// A pointer to audio data.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct AudioAddress {
    /// True if the audio is looping. Note that this is a DSP parameter and should always be true,
    /// even if the HPS as a whole does not loop.
    pub looping: bool,
    /// Format of each sample.
    pub format: DspFormat,
    /// Address of the sample that looping starts at.
    pub loop_address: u32,
    /// Address of the last sample.
    pub end_address: u32,
    /// Address that playback should begin at.
    pub current_address: u32,
}

impl<R: Read> ReadFrom<R> for AudioAddress {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self {
            looping: reader.read_u16::<BE>()? != 0,
            format: DspFormat::read_from(reader)?,
            loop_address: reader.read_u32::<BE>()?,
            end_address: reader.read_u32::<BE>()?,
            current_address: reader.read_u32::<BE>()?,
        })
    }
}
