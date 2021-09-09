use super::adpcm::{Context, Decoder, GcAdpcm, Info};
use super::dsp::{AudioAddress, DspFormat};
use super::format::{AnyFormat, Format, PcmS16Le};
use super::sample::{CastSamples, JoinChannels};
use super::{Error, ReadSamples, Result, Samples};
use crate::common::ReadFrom;
use arrayvec::ArrayVec;
use byteorder::{ReadBytesExt, BE};
use log::{debug, error};
use std::borrow::Cow;
use std::fmt::{self, Debug};
use std::io::{self, Read, Seek, SeekFrom};

/// The size of the file header.
const HEADER_SIZE: u64 = 0x10;
/// The alignment of data in an SSM file.
const DATA_ALIGN: u64 = 0x20;

/// Convenience type for an opaque decoder.
type SsmDecoder<'a> = Box<dyn ReadSamples<'static, Format = PcmS16Le> + 'a>;

/// Aligns `offset` to the next multiple of the data alignment.
fn align(offset: u64) -> u64 {
    (offset + DATA_ALIGN - 1) & !(DATA_ALIGN - 1)
}

/// SSM file header.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
struct FileHeader {
    /// Size of the sound header data in bytes.
    index_size: u32,
    /// Size of the sample data in bytes.
    data_size: u32,
    /// The number of sounds stored in the bank.
    num_sounds: u32,
    /// The global index of the first sound in the bank.
    base_index: u32,
}

impl<R: Read> ReadFrom<R> for FileHeader {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self {
            index_size: reader.read_u32::<BE>()?,
            data_size: reader.read_u32::<BE>()?,
            num_sounds: reader.read_u32::<BE>()?,
            base_index: reader.read_u32::<BE>()?,
        })
    }
}

/// Header for sound channel data.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
struct ChannelHeader {
    /// The pointer to the sound data.
    address: AudioAddress,
    /// ADPCM decoder info.
    adpcm: Info,
    /// ADPCM loop context.
    loop_context: Context,
}

impl<R: Read> ReadFrom<R> for ChannelHeader {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let header = Self {
            address: AudioAddress::read_from(reader)?,
            adpcm: Info::read_from(reader)?,
            loop_context: Context::read_from(reader)?,
        };
        let _padding = reader.read_u16::<BE>()?;
        Ok(header)
    }
}

/// Header for sound data (stored in the sound list at the beginning).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SoundHeader {
    /// The sound's sample rate.
    sample_rate: u32,
    /// The headers for each channel.
    channels: ArrayVec<[ChannelHeader; 2]>,
}

impl<R: Read> ReadFrom<R> for SoundHeader {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let num_channels = reader.read_u32::<BE>()?;
        let sample_rate = reader.read_u32::<BE>()?;
        let mut channels: ArrayVec<[ChannelHeader; 2]> = ArrayVec::new();
        for _ in 0..num_channels {
            channels.push(ChannelHeader::read_from(reader)?);
        }
        Ok(Self { sample_rate, channels })
    }
}

/// Contains complete data for a channel in a sound.
#[derive(Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Channel {
    pub address: AudioAddress,
    pub adpcm: Info,
    pub loop_context: Context,
    pub data: Vec<u8>,
}

impl Channel {
    /// Creates a `SoundReader` over the raw sample data.
    pub fn reader(&self) -> SoundReader<'_> {
        SoundReader { channel: Some(self) }
    }

    /// Creates a decoder which decodes the channel into PCM16 format.
    pub fn decoder(&self) -> SsmDecoder<'_> {
        let reader = self.reader();
        let casted = CastSamples::new(reader);
        Box::new(Decoder::new(Box::new(casted), &self.adpcm.coefficients))
    }

    fn from_bank(header: &ChannelHeader, bank_data: &[u8]) -> Self {
        // Copying into a standalone buffer will make it easier to edit sounds in the future
        let format = Format::from(header.address.format);
        let start_address = if format == Format::GcAdpcm {
            // Align to a frame boundary
            header.address.current_address & !0xf
        } else {
            header.address.current_address
        };
        let end_address = header.address.end_address;
        let nibbles = (end_address - start_address + 1) as usize;
        let start_offset = format.address_to_byte(start_address as usize);
        let end_offset = start_offset + format.size_of(nibbles);
        let data = Vec::from(&bank_data[start_offset..end_offset]);

        // Since we have a separate data buffer now, we have to update the addresses
        let mut address = header.address;
        address.loop_address -= start_address;
        address.end_address -= start_address;
        address.current_address -= start_address;

        Channel { address, adpcm: header.adpcm, loop_context: header.loop_context, data }
    }
}

impl Debug for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Channel")
            .field("address", &self.address)
            .field("adpcm", &self.adpcm)
            .field("loop_context", &self.loop_context)
            .finish()
    }
}

/// Contains the complete data for a sound.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Sound {
    /// The sound's sample rate.
    pub sample_rate: u32,
    /// The data for each channel in the sound.
    pub channels: ArrayVec<[Channel; 2]>,
}

impl Sound {
    /// Creates a decoder which decodes all channels into PCM16 format and joins them.
    pub fn decoder(&self) -> SsmDecoder<'_> {
        if self.channels.len() == 1 {
            self.channels[0].decoder()
        } else {
            let left = self.channels[0].decoder();
            let right = self.channels[1].decoder();
            Box::new(JoinChannels::new(left, right))
        }
    }

    fn from_bank(header: &SoundHeader, bank_data: &[u8]) -> Self {
        let channels: ArrayVec<_> =
            header.channels.iter().map(|channel| Channel::from_bank(channel, bank_data)).collect();
        Self { sample_rate: header.sample_rate, channels }
    }
}

/// A SSM sound bank made up of multiple sounds.
#[derive(Clone)]
#[non_exhaustive]
pub struct SoundBank {
    /// The global index of the first sound in the bank.
    pub base_index: u32,
    /// The sounds in the bank.
    pub sounds: Vec<Sound>,
}

impl<R: Read + Seek> ReadFrom<R> for SoundBank {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let header = FileHeader::read_from(reader)?;

        // Sound headers follow the main header
        let mut sound_headers = vec![];
        for _ in 0..header.num_sounds {
            sound_headers.push(SoundHeader::read_from(reader)?);
        }

        // The sample data follows the sound headers, aligned to the next 64-byte boundary
        let data_offset = align(HEADER_SIZE + header.index_size as u64);
        reader.seek(SeekFrom::Start(data_offset))?;
        let mut data = vec![];
        reader.take(header.data_size as u64).read_to_end(&mut data)?;
        if data.len() != header.data_size as usize {
            error!("Sound bank data is too small (expected {:#x})", header.data_size);
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
        }

        // Split the data up across all the different sounds
        let sounds: Vec<_> =
            sound_headers.into_iter().map(|sound| Sound::from_bank(&sound, &data)).collect();

        debug!("Loaded sound bank with {} sounds", sounds.len());
        Ok(Self { base_index: header.base_index, sounds })
    }
}

impl Debug for SoundBank {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SoundBank")
            .field("base_index", &self.base_index)
            .field("sounds", &self.sounds)
            .finish()
    }
}

/// Reads sample data from a sound channel.
pub struct SoundReader<'a> {
    channel: Option<&'a Channel>,
}

impl<'a> ReadSamples<'a> for SoundReader<'a> {
    type Format = AnyFormat;
    fn read_samples(&mut self) -> Result<Option<Samples<'a, Self::Format>>> {
        let channel = match self.channel.take() {
            Some(c) => c,
            None => return Ok(None),
        };
        let context = channel.adpcm.context;
        let start_address = channel.address.current_address as usize;
        let end_address = channel.address.end_address as usize;
        let format = channel.address.format;
        match format {
            DspFormat::Adpcm => Ok(Some(
                Samples::<GcAdpcm> {
                    context,
                    start_address,
                    end_address,
                    channels: 1,
                    bytes: Cow::Borrowed(&channel.data),
                }
                .into_any(),
            )),
            other => {
                // TODO
                error!("Sample format not supported yet: {:?}", other);
                Err(Error::UnsupportedFormat(format.into()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[rustfmt::skip]
    const SSM_BYTES: &[u8] = &[
        0x00, 0x00, 0x00, 0xd0, // index_size
        0x00, 0x00, 0x00, 0x30, // data_size
        0x00, 0x00, 0x00, 0x02, // num_sounds
        0x00, 0x00, 0x01, 0x23, // base_index

        // sounds[0]
        0x00, 0x00, 0x00, 0x01, // num_channels
        0x00, 0x00, 0x3e, 0x80, // sample_rate

        // sounds[0].channels[0].address
        0x00, 0x00, // looping
        0x00, 0x00, // format
        0x00, 0x00, 0x00, 0x02, // loop_address
        0x00, 0x00, 0x00, 0x1f, // end_address
        0x00, 0x00, 0x00, 0x02, // current_address

        // sounds[0].channels[0].info
        0x00, 0x00, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03, // coefficients[0..4]
        0x00, 0x04, 0x00, 0x05, 0x00, 0x06, 0x00, 0x07, // coefficients[4..8]
        0x00, 0x08, 0x00, 0x09, 0x00, 0x0a, 0x00, 0x0b, // coefficients[8..12]
        0x00, 0x0c, 0x00, 0x0d, 0x00, 0x0e, 0x00, 0x0f, // coefficients[12..16]
        0x00, 0x00, // gain
        0x00, 0x17, 0x00, 0x00, 0x00, 0x00, // context
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // loop_context
        0x00, 0x00, // padding

        // sounds[1]
        0x00, 0x00, 0x00, 0x02, // num_channels
        0x00, 0x00, 0x3e, 0x80, // sample_rate

        // sounds[1].channels[0].address
        0x00, 0x00, // looping
        0x00, 0x00, // format
        0x00, 0x00, 0x00, 0x22, // loop_address
        0x00, 0x00, 0x00, 0x3f, // end_address
        0x00, 0x00, 0x00, 0x22, // current_address

        // sounds[1].channels[0].info
        0x00, 0x00, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03, // coefficients[0..4]
        0x00, 0x04, 0x00, 0x05, 0x00, 0x06, 0x00, 0x07, // coefficients[4..8]
        0x00, 0x08, 0x00, 0x09, 0x00, 0x0a, 0x00, 0x0b, // coefficients[8..12]
        0x00, 0x0c, 0x00, 0x0d, 0x00, 0x0e, 0x00, 0x0f, // coefficients[12..16]
        0x00, 0x00, // gain
        0x00, 0x17, 0x00, 0x00, 0x00, 0x00, // context
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // loop_context
        0x00, 0x00, // padding

        // sounds[1].channels[1].address
        0x00, 0x00, // looping
        0x00, 0x00, // format
        0x00, 0x00, 0x00, 0x42, // loop_address
        0x00, 0x00, 0x00, 0x5f, // end_address
        0x00, 0x00, 0x00, 0x42, // current_address

        // sounds[1].channels[1].info
        0x00, 0x00, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03, // coefficients[0..4]
        0x00, 0x04, 0x00, 0x05, 0x00, 0x06, 0x00, 0x07, // coefficients[4..8]
        0x00, 0x08, 0x00, 0x09, 0x00, 0x0a, 0x00, 0x0b, // coefficients[8..12]
        0x00, 0x0c, 0x00, 0x0d, 0x00, 0x0e, 0x00, 0x0f, // coefficients[12..16]
        0x00, 0x00, // gain
        0x00, 0x17, 0x00, 0x00, 0x00, 0x00, // context
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // loop_context
        0x00, 0x00, // padding

        // sound 0 channel 0
        0x17, 0x02, 0x04, 0x06, 0x08, 0x0a, 0x0c, 0x0e,
        0x17, 0x12, 0x14, 0x16, 0x18, 0x1a, 0x1c, 0x1e,

        // sound 1 channel 0
        0x17, 0x22, 0x24, 0x26, 0x28, 0x2a, 0x2c, 0x2e,
        0x17, 0x32, 0x34, 0x36, 0x38, 0x3a, 0x3c, 0x3e,

        // sound 1 channel 1
        0x17, 0x42, 0x44, 0x46, 0x48, 0x4a, 0x4c, 0x4e,
        0x17, 0x52, 0x54, 0x56, 0x58, 0x5a, 0x5c, 0x5e,
    ];

    #[test]
    fn test_read_sound_bank() -> Result<()> {
        let ssm = SoundBank::read_from(&mut Cursor::new(SSM_BYTES))?;
        assert_eq!(ssm.base_index, 0x123);
        assert_eq!(ssm.sounds.len(), 2);

        let samples0_0 = ssm.sounds[0].channels[0].reader().read_samples()?.unwrap();
        assert_eq!(samples0_0.format(), Format::GcAdpcm);
        assert_eq!(samples0_0.start_address, 0x2);
        assert_eq!(samples0_0.end_address, 0x1f);
        assert_eq!(samples0_0.channels, 1);
        assert_eq!(samples0_0.bytes, &SSM_BYTES[0xe0..0xf0]);

        let samples1_0 = ssm.sounds[1].channels[0].reader().read_samples()?.unwrap();
        assert_eq!(samples1_0.format(), Format::GcAdpcm);
        assert_eq!(samples1_0.start_address, 0x2);
        assert_eq!(samples1_0.end_address, 0x1f);
        assert_eq!(samples1_0.channels, 1);
        assert_eq!(samples1_0.bytes, &SSM_BYTES[0xf0..0x100]);

        let samples1_1 = ssm.sounds[1].channels[1].reader().read_samples()?.unwrap();
        assert_eq!(samples1_1.format(), Format::GcAdpcm);
        assert_eq!(samples1_1.start_address, 0x2);
        assert_eq!(samples1_1.end_address, 0x1f);
        assert_eq!(samples1_1.channels, 1);
        assert_eq!(samples1_1.bytes, &SSM_BYTES[0x100..0x110]);
        Ok(())
    }
}