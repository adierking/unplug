use super::{Error, Format, Result};
use crate::common::{ReadFrom, WriteTo};
use byteorder::{ReadBytesExt, WriteBytesExt, BE};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::convert::TryFrom;
use std::io::{Read, Write};

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

impl<W: Write> WriteTo<W> for DspFormat {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_u16::<BE>((*self).into())?;
        Ok(())
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

impl<W: Write> WriteTo<W> for AudioAddress {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_u16::<BE>(self.looping.into())?;
        self.format.write_to(writer)?;
        writer.write_u32::<BE>(self.loop_address)?;
        writer.write_u32::<BE>(self.end_address)?;
        writer.write_u32::<BE>(self.current_address)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_write_and_read;

    #[test]
    fn test_write_and_read_audio_address() {
        assert_write_and_read!(AudioAddress {
            looping: true,
            format: DspFormat::Pcm16,
            loop_address: 1,
            end_address: 2,
            current_address: 3,
        });
    }
}
