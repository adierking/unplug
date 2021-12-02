use crate::audio::format::adpcm::{Decoder, Encoder, FrameContext, GcAdpcm, Info};
use crate::audio::format::dsp::{AudioAddress, DspFormat};
use crate::audio::format::{AnyFormat, Format, PcmS16Be, PcmS16Le, PcmS8, ReadWriteBytes};
use crate::audio::{Error, ProgressHint, ReadSamples, Result, Samples, SourceChannel, SourceTag};
use crate::common::io::pad;
use crate::common::{align, ReadFrom, ReadSeek, WriteTo};
use arrayvec::ArrayVec;
use byteorder::{ReadBytesExt, WriteBytesExt, BE};
use std::fmt::{self, Debug};
use std::io::{self, Read, Seek, SeekFrom, Write};
use tracing::{debug, error, instrument};

/// The size of the file header.
const HEADER_SIZE: u64 = 0x10;
/// The alignment of data in an SSM file.
const DATA_ALIGN: u64 = 0x20;
/// The alignment of each audio frame in bytes.
const FRAME_ALIGN: u64 = 8;

/// Convenience type for an opaque decoder.
type SsmDecoder<'r, 's> = Box<dyn ReadSamples<'s, Format = PcmS16Le> + 'r>;

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

impl<R: Read + ?Sized> ReadFrom<R> for FileHeader {
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

impl<W: Write + ?Sized> WriteTo<W> for FileHeader {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_u32::<BE>(self.index_size)?;
        writer.write_u32::<BE>(self.data_size)?;
        writer.write_u32::<BE>(self.num_sounds)?;
        writer.write_u32::<BE>(self.base_index)?;
        Ok(())
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
    loop_context: FrameContext,
}

impl<R: Read + ?Sized> ReadFrom<R> for ChannelHeader {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let header = Self {
            address: AudioAddress::read_from(reader)?,
            adpcm: Info::read_from(reader)?,
            loop_context: FrameContext::read_from(reader)?,
        };
        let _padding = reader.read_u16::<BE>()?;
        Ok(header)
    }
}

impl<W: Write + ?Sized> WriteTo<W> for ChannelHeader {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        self.address.write_to(writer)?;
        self.adpcm.write_to(writer)?;
        self.loop_context.write_to(writer)?;
        writer.write_u16::<BE>(0)?;
        Ok(())
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

impl<R: Read + ?Sized> ReadFrom<R> for SoundHeader {
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

impl<W: Write + ?Sized> WriteTo<W> for SoundHeader {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_u32::<BE>(self.channels.len() as u32)?;
        writer.write_u32::<BE>(self.sample_rate)?;
        for channel in &self.channels {
            channel.write_to(writer)?;
        }
        Ok(())
    }
}

/// Contains complete data for a channel in a sound.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Channel {
    pub address: AudioAddress,
    pub adpcm: Info,
    pub loop_context: FrameContext,
    pub data: Vec<u8>,
}

impl Channel {
    /// Creates a `SoundReader` over the raw sample data.
    pub fn reader(&self, sample_rate: u32, tag: SourceTag) -> SoundReader<'_> {
        SoundReader { channel: Some(self), format: self.address.format, sample_rate, tag }
    }

    /// Creates a decoder which decodes the channel into PCM16 format.
    pub fn decoder(&self, sample_rate: u32, tag: SourceTag) -> SsmDecoder<'_, '_> {
        let reader = self.reader(sample_rate, tag);
        match self.address.format {
            DspFormat::Adpcm => Box::new(Decoder::new(reader.cast())),
            DspFormat::Pcm8 | DspFormat::Pcm16 => reader.convert(),
        }
    }

    /// Creates a new `Channel` from ADPCM sample data.
    pub fn from_adpcm(reader: &mut dyn ReadSamples<'_, Format = GcAdpcm>) -> Result<Self> {
        let samples = reader.read_all_samples()?;
        if samples.channels != 1 {
            return Err(Error::StreamNotMono);
        }
        let mut data = vec![];
        GcAdpcm::write_bytes(&mut data, &samples.data)?;
        Ok(Self {
            address: AudioAddress {
                looping: false, // TODO
                format: DspFormat::Adpcm,
                loop_address: 0x2,
                end_address: (samples.len - 1) as u32,
                current_address: 0x2,
            },
            adpcm: samples.params,
            loop_context: FrameContext::default(), // TODO
            data,
        })
    }

    fn from_bank(header: &ChannelHeader, bank_data: &[u8]) -> Self {
        // Copying into a standalone buffer will make it easier to edit sounds in the future
        let format = Format::from(header.address.format);
        let start_address = format.frame_address(header.address.current_address as usize);
        let end_address = header.address.end_address as usize;
        let len = end_address - start_address + 1;
        let start_offset = format.address_to_byte(start_address);
        let end_offset = align(start_offset + format.address_to_byte_up(len), FRAME_ALIGN as usize);
        let data = Vec::from(&bank_data[start_offset..end_offset]);

        // Since we have a separate data buffer now, we have to update the addresses
        let mut address = header.address;
        address.loop_address -= start_address as u32;
        address.end_address -= start_address as u32;
        address.current_address -= start_address as u32;

        Channel { address, adpcm: header.adpcm, loop_context: header.loop_context, data }
    }

    fn make_header(&self, data_offset: u64) -> ChannelHeader {
        let mut address = self.address;
        let format = Format::from(address.format);
        let start_address = format.byte_to_address(data_offset as usize) as u32;
        address.loop_address += start_address;
        address.end_address += start_address;
        address.current_address += start_address;
        ChannelHeader { address, adpcm: self.adpcm, loop_context: self.loop_context }
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
    pub fn decoder(&self, tag: SourceTag) -> SsmDecoder<'_, '_> {
        let rate = self.sample_rate;
        if self.channels.len() == 1 {
            self.channels[0].decoder(rate, tag)
        } else {
            let left = self.channels[0].decoder(rate, tag.clone().for_channel(SourceChannel::Left));
            let right = self.channels[1].decoder(rate, tag.for_channel(SourceChannel::Right));
            Box::new(left.with_right_channel(right))
        }
    }

    /// Creates a new `Sound` by encoding mono/stereo PCMS16LE sample data to ADPCM format.
    #[instrument(level = "trace", skip_all)]
    pub fn from_pcm(reader: &mut dyn ReadSamples<'_, Format = PcmS16Le>) -> Result<Self> {
        let samples = reader.read_all_samples()?;
        let channels = samples.channels;
        let samples = samples.into_reader(reader.tag().clone());
        if channels == 2 {
            let splitter = samples.split_channels();
            let mut left = Encoder::new(splitter.left());
            let mut right = Encoder::new(splitter.right());
            Self::from_adpcm_stereo(&mut left, &mut right)
        } else if channels == 1 {
            let mut encoder = Encoder::new(samples);
            Self::from_adpcm_mono(&mut encoder)
        } else {
            Err(Error::UnsupportedChannels)
        }
    }

    /// Creates a new `Sound` from mono ADPCM sample data.
    #[instrument(level = "trace", skip_all)]
    pub fn from_adpcm_mono(reader: &mut dyn ReadSamples<'_, Format = GcAdpcm>) -> Result<Self> {
        // Pull the sample rate from the first samples in the stream
        let mut reader = reader.peekable();
        let sample_rate = match reader.peek_samples()? {
            Some(s) => s.rate,
            None => return Err(Error::EmptyStream),
        };

        let mut channels = ArrayVec::new();
        channels.push(Channel::from_adpcm(&mut reader)?);
        Ok(Self { sample_rate, channels })
    }

    /// Creates a new `Sound` from stereo ADPCM sample data.
    #[instrument(level = "trace", skip_all)]
    pub fn from_adpcm_stereo(
        left: &mut dyn ReadSamples<'_, Format = GcAdpcm>,
        right: &mut dyn ReadSamples<'_, Format = GcAdpcm>,
    ) -> Result<Self> {
        // Pull the sample rate from the first samples in the streams and make sure the left and
        // right streams match
        let mut left = left.peekable();
        let mut right = right.peekable();
        let (lrate, rrate) = match (left.peek_samples()?, right.peek_samples()?) {
            (Some(l), Some(r)) => (l.rate, r.rate),
            _ => return Err(Error::EmptyStream),
        };
        if lrate != rrate {
            return Err(Error::InconsistentSampleRate);
        }

        let mut channels = ArrayVec::new();
        channels.push(Channel::from_adpcm(&mut left)?);
        channels.push(Channel::from_adpcm(&mut right)?);
        Ok(Self { sample_rate: lrate, channels })
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
    /// The audio source tag for debugging purposes.
    tag: SourceTag,
}

impl SoundBank {
    /// Opens an SSM sound bank read from `reader`. `tag` is a string or tag to identify sound
    /// streams for debugging purposes.
    pub fn open(reader: &mut dyn ReadSeek, tag: impl Into<SourceTag>) -> Result<Self> {
        Self::open_impl(reader, tag.into())
    }

    #[instrument(level = "trace", skip_all)]
    fn open_impl(reader: &mut dyn ReadSeek, tag: SourceTag) -> Result<Self> {
        let header = FileHeader::read_from(reader)?;

        // Sound headers follow the main header
        let mut sound_headers = vec![];
        for _ in 0..header.num_sounds {
            sound_headers.push(SoundHeader::read_from(reader)?);
        }

        // The sample data follows the sound headers, aligned to the next 64-byte boundary
        let data_offset = align(HEADER_SIZE + header.index_size as u64, DATA_ALIGN);
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

        debug!("Loaded sound bank {:?} with {} sounds", tag, sounds.len());
        Ok(Self { base_index: header.base_index, sounds, tag })
    }

    /// Returns a reader over the raw sample data for a channel in a sound.
    pub fn reader(&self, index: usize, channel: usize) -> SoundReader<'_> {
        let sound = &self.sounds[index];
        sound.channels[channel].reader(sound.sample_rate, self.sound_tag(index))
    }

    /// Returns a decoder which decodes a sound into PCM16 format.
    pub fn decoder(&self, index: usize) -> SsmDecoder<'_, '_> {
        self.sounds[index].decoder(self.sound_tag(index))
    }

    /// Creates a `SourceTag` for a sound in the bank.
    fn sound_tag(&self, index: usize) -> SourceTag {
        SourceTag::new(format!("{}[{}]", self.tag.name, index))
    }
}

impl<W: Write + Seek + ?Sized> WriteTo<W> for SoundBank {
    type Error = Error;

    #[instrument(level = "trace", skip_all)]
    fn write_to(&self, writer: &mut W) -> Result<()> {
        assert_eq!(writer.seek(SeekFrom::Current(0))?, 0);

        // Write a placeholder header which we can fill in with the sizes later
        let mut header = FileHeader {
            index_size: 0,
            data_size: 0,
            num_sounds: self.sounds.len() as u32,
            base_index: self.base_index,
        };
        header.write_to(writer)?;

        // Write all the sound headers and keep track of what the current channel's data offset
        // should be so that we can write everything in one pass
        let mut data_offset = 0;
        for sound in &self.sounds {
            let mut channels = ArrayVec::new();
            for channel in &sound.channels {
                channels.push(channel.make_header(data_offset));
                // Audio data must be aligned on a frame boundary
                data_offset = align(data_offset + channel.data.len() as u64, FRAME_ALIGN);
            }
            let sound_header = SoundHeader { sample_rate: sound.sample_rate, channels };
            sound_header.write_to(writer)?;
        }
        header.index_size = (writer.seek(SeekFrom::Current(0))? - HEADER_SIZE) as u32;
        header.data_size = align(data_offset, DATA_ALIGN) as u32;
        pad(&mut *writer, DATA_ALIGN, 0)?;

        for sound in &self.sounds {
            for channel in &sound.channels {
                writer.write_all(&channel.data)?;
                pad(&mut *writer, FRAME_ALIGN, 0)?;
            }
        }
        // The data section size is aligned in the official SSM files
        pad(&mut *writer, DATA_ALIGN, 0)?;

        let end_offset = writer.seek(SeekFrom::Current(0))?;
        writer.seek(SeekFrom::Start(0))?;
        header.write_to(writer)?;
        writer.seek(SeekFrom::Start(end_offset))?;
        Ok(())
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
    format: DspFormat,
    sample_rate: u32,
    tag: SourceTag,
}

impl<'a> ReadSamples<'a> for SoundReader<'a> {
    type Format = AnyFormat;

    #[instrument(level = "trace", name = "SoundReader", skip_all)]
    fn read_samples(&mut self) -> Result<Option<Samples<'a, Self::Format>>> {
        let channel = match self.channel.take() {
            Some(c) => c,
            None => return Ok(None),
        };
        let data = &channel.data;
        let len = channel.address.end_address as usize + 1;
        match self.format {
            DspFormat::Adpcm => Ok(Some(
                Samples::<GcAdpcm> {
                    channels: 1,
                    rate: self.sample_rate,
                    len,
                    data: data.into(),
                    params: channel.adpcm,
                }
                .cast(),
            )),
            DspFormat::Pcm16 => {
                let samples = PcmS16Be::read_bytes(&data[..(len * 2)])?;
                Ok(Some(Samples::<PcmS16Be>::from_pcm(samples, 1, self.sample_rate).cast()))
            }
            DspFormat::Pcm8 => {
                let samples = PcmS8::read_bytes(&data[..len])?;
                Ok(Some(Samples::<PcmS8>::from_pcm(samples, 1, self.sample_rate).cast()))
            }
        }
    }

    fn format(&self) -> Format {
        self.format.into()
    }

    fn tag(&self) -> &SourceTag {
        &self.tag
    }

    fn progress(&self) -> Option<ProgressHint> {
        match &self.channel {
            Some(_) => ProgressHint::new(0, 1),
            None => ProgressHint::new(1, 1),
        }
    }

    fn data_remaining(&self) -> Option<u64> {
        match &self.channel {
            Some(c) => Some(c.address.end_address as u64 + 1),
            None => Some(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_write_and_read;
    use crate::test;
    use std::io::Cursor;

    #[rustfmt::skip]
    const SSM_BYTES: &[u8] = &[
        0x00, 0x00, 0x00, 0xd0, // index_size
        0x00, 0x00, 0x00, 0x40, // data_size
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

        // padding
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    #[test]
    fn test_read_sound_bank() -> Result<()> {
        let ssm = SoundBank::open(&mut Cursor::new(SSM_BYTES), "SSM_BYTES")?;
        assert_eq!(ssm.base_index, 0x123);
        assert_eq!(ssm.sounds.len(), 2);

        let samples0_0 = ssm.reader(0, 0).read_samples()?.unwrap();
        let samples0_0: Samples<'_, GcAdpcm> = samples0_0.cast();
        assert_eq!(samples0_0.format(), Format::GcAdpcm);
        assert_eq!(samples0_0.len, 32);
        assert_eq!(samples0_0.channels, 1);
        assert_eq!(samples0_0.data, &SSM_BYTES[0xe0..0xf0]);

        let samples1_0 = ssm.reader(1, 0).read_samples()?.unwrap();
        let samples1_0: Samples<'_, GcAdpcm> = samples1_0.cast();
        assert_eq!(samples1_0.format(), Format::GcAdpcm);
        assert_eq!(samples1_0.len, 32);
        assert_eq!(samples1_0.channels, 1);
        assert_eq!(samples1_0.data, &SSM_BYTES[0xf0..0x100]);

        let samples1_1 = ssm.reader(1, 1).read_samples()?.unwrap();
        let samples1_1: Samples<'_, GcAdpcm> = samples1_1.cast();
        assert_eq!(samples1_1.format(), Format::GcAdpcm);
        assert_eq!(samples1_1.len, 32);
        assert_eq!(samples1_1.channels, 1);
        assert_eq!(samples1_1.data, &SSM_BYTES[0x100..0x110]);
        Ok(())
    }

    #[test]
    fn test_read_and_write_sound_bank() -> Result<()> {
        let ssm = SoundBank::open(&mut Cursor::new(SSM_BYTES), "SSM_BYTES")?;
        let mut writer = Cursor::new(vec![]);
        ssm.write_to(&mut writer)?;
        assert_eq!(writer.into_inner(), SSM_BYTES);
        Ok(())
    }

    #[test]
    fn test_write_and_read_file_header() {
        assert_write_and_read!(FileHeader {
            index_size: 1,
            data_size: 2,
            num_sounds: 3,
            base_index: 4,
        });
    }

    #[test]
    fn test_write_and_read_channel_header() {
        assert_write_and_read!(ChannelHeader {
            address: AudioAddress::default(),
            adpcm: Info::default(),
            loop_context: FrameContext::default(),
        });
    }

    #[test]
    fn test_write_and_read_sound_header() {
        assert_write_and_read!(SoundHeader {
            sample_rate: 44100,
            channels: ArrayVec::from([ChannelHeader::default(); 2]),
        });
    }

    fn assert_left_channel(channel: &Channel) {
        assert!(!channel.address.looping);
        assert_eq!(channel.address.format, DspFormat::Adpcm);
        assert_eq!(channel.address.loop_address, 0x2);
        assert_eq!(channel.address.end_address as usize, test::TEST_WAV_DSP_END_ADDRESS);
        assert_eq!(channel.address.current_address, 0x2);
        assert_eq!(channel.adpcm.coefficients, test::TEST_WAV_LEFT_COEFFICIENTS);
        assert_eq!(channel.data, test::TEST_WAV_LEFT_DSP);
    }

    fn assert_right_channel(channel: &Channel) {
        assert!(!channel.address.looping);
        assert_eq!(channel.address.format, DspFormat::Adpcm);
        assert_eq!(channel.address.loop_address, 0x2);
        assert_eq!(channel.address.end_address as usize, test::TEST_WAV_DSP_END_ADDRESS);
        assert_eq!(channel.address.current_address, 0x2);
        assert_eq!(channel.adpcm.coefficients, test::TEST_WAV_RIGHT_COEFFICIENTS);
        assert_eq!(channel.data, test::TEST_WAV_RIGHT_DSP);
    }

    #[test]
    fn test_sound_from_mono() -> Result<()> {
        let samples = Samples::<GcAdpcm> {
            channels: 1,
            rate: 44100,
            len: test::TEST_WAV_DSP_END_ADDRESS + 1,
            data: test::TEST_WAV_LEFT_DSP.into(),
            params: Info {
                coefficients: test::TEST_WAV_LEFT_COEFFICIENTS,
                gain: 0,
                context: FrameContext::default(),
            },
        };
        let sound = Sound::from_adpcm_mono(&mut samples.into_reader("test"))?;
        assert_eq!(sound.sample_rate, 44100);
        assert_eq!(sound.channels.len(), 1);
        assert_left_channel(&sound.channels[0]);
        Ok(())
    }

    #[test]
    fn test_sound_from_stereo() -> Result<()> {
        let lsamples = Samples::<GcAdpcm> {
            channels: 1,
            rate: 44100,
            len: test::TEST_WAV_DSP_END_ADDRESS + 1,
            data: test::TEST_WAV_LEFT_DSP.into(),
            params: Info {
                coefficients: test::TEST_WAV_LEFT_COEFFICIENTS,
                gain: 0,
                context: FrameContext::default(),
            },
        };
        let rsamples = Samples::<GcAdpcm> {
            channels: 1,
            rate: 44100,
            len: test::TEST_WAV_DSP_END_ADDRESS + 1,
            data: test::TEST_WAV_RIGHT_DSP.into(),
            params: Info {
                coefficients: test::TEST_WAV_RIGHT_COEFFICIENTS,
                gain: 0,
                context: FrameContext::default(),
            },
        };
        let sound = Sound::from_adpcm_stereo(
            &mut lsamples.into_reader("test"),
            &mut rsamples.into_reader("test"),
        )?;
        assert_eq!(sound.sample_rate, 44100);
        assert_eq!(sound.channels.len(), 2);
        assert_left_channel(&sound.channels[0]);
        assert_right_channel(&sound.channels[1]);
        Ok(())
    }

    #[test]
    fn test_sound_from_pcm() -> Result<()> {
        let data = test::open_test_wav();
        let samples = Samples::<PcmS16Le>::from_pcm(data, 2, 44100);
        let sound = Sound::from_pcm(&mut samples.into_reader("test"))?;
        assert_eq!(sound.sample_rate, 44100);
        assert_eq!(sound.channels.len(), 2);
        assert_left_channel(&sound.channels[0]);
        assert_right_channel(&sound.channels[1]);
        Ok(())
    }
}
