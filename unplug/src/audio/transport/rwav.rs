// rwav is a Wii-specific format, but supporting it allows us to read sounds from the New Play
// Control release.

use crate::audio::format::adpcm::{self, GcAdpcm};
use crate::audio::format::{AnyFormat, Format, PcmS16Le};
use crate::audio::{
    Cue, Error, ProgressHint, ReadSamples, Result, Samples, SourceChannel, SourceTag,
};
use crate::common::{ReadFrom, ReadSeek, Region};
use arrayvec::ArrayVec;
use byteorder::{ReadBytesExt, BE};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::borrow::Cow;
use std::convert::TryFrom;
use std::fmt::{self, Debug};
use std::io::{self, Read, Seek, SeekFrom};
use std::iter;
use tracing::error;

const RWAV_MAGIC: u32 = 0x52574156; // 'RWAV'
const RWAV_VERSION: u16 = 0x0102;

const BIG_ENDIAN: u16 = 0xfeff;

const SECTION_HEADER_SIZE: u64 = 0x8;

/// Convenience type for an opaque decoder.
type RwavDecoder<'r, 's> = Box<dyn ReadSamples<'s, Format = PcmS16Le> + 'r>;

/// The RWAV file header.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct FileHeader {
    /// Magic number (`RWAV_MAGIC`).
    magic: u32,
    /// 0xfeff if big endian, 0xfffe if little endian
    endian: u16,
    /// File version (we expect 0x102).
    version: u16,
    /// Total file size.
    file_size: u32,
    /// Size of this header.
    header_size: u16,
    /// Number of sections in the file.
    num_sections: u16,
    /// Offset to the info section.
    info_offset: u32,
    /// Size of the info section.
    info_size: u32,
    /// Offset to the data section.
    data_offset: u32,
    /// Size of the data section.
    data_size: u32,
}

impl<R: Read + ?Sized> ReadFrom<R> for FileHeader {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let magic = reader.read_u32::<BE>()?;
        if magic != RWAV_MAGIC {
            return Err(Error::InvalidRwav);
        }
        let endian = reader.read_u16::<BE>()?;
        if endian != BIG_ENDIAN {
            error!("Only big-endian RWAV data is supported");
            return Err(Error::InvalidRwav);
        }
        let version = reader.read_u16::<BE>()?;
        if version != RWAV_VERSION {
            error!("Only RWAV version 0x102 is supported");
            return Err(Error::InvalidRwav);
        }
        let file_size = reader.read_u32::<BE>()?;
        let header_size = reader.read_u16::<BE>()?;
        let num_sections = reader.read_u16::<BE>()?;
        if num_sections < 2 {
            error!("RWAV data must have at least 2 sections");
            return Err(Error::InvalidRwav);
        }
        Ok(Self {
            magic,
            endian,
            version,
            file_size,
            header_size,
            num_sections,
            info_offset: reader.read_u32::<BE>()?,
            info_size: reader.read_u32::<BE>()?,
            data_offset: reader.read_u32::<BE>()?,
            data_size: reader.read_u32::<BE>()?,
        })
    }
}

/// The header at the beginning of each RWAV section.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct SectionHeader {
    /// The section identifier (`INFO` or `DATA`).
    magic: u32,
    /// The size of the section data, including this header.
    size: u32,
}

impl<R: Read + ?Sized> ReadFrom<R> for SectionHeader {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self { magic: reader.read_u32::<BE>()?, size: reader.read_u32::<BE>()? })
    }
}

/// RWAV audio codecs.
#[derive(Debug, Copy, Clone, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum Codec {
    Pcm8 = 0,
    Pcm16 = 1,
    GcAdpcm = 2,
}

impl From<Codec> for Format {
    fn from(codec: Codec) -> Self {
        match codec {
            Codec::Pcm8 => Format::PcmS8,
            Codec::Pcm16 => Format::PcmS16Be,
            Codec::GcAdpcm => Format::GcAdpcm,
        }
    }
}

/// The header at the beginning of the INFO section data.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct InfoHeader {
    /// The codec used to store the sound data.
    codec: Codec,
    /// True if the sound should loop.
    looping: bool,
    /// The number of channels in the sound (1 or 2).
    num_channels: u8,
    /// The sample rate.
    sample_rate: u16,
    /// The address that decoding should start at.
    start_address: u32,
    /// The address that decoding should end at.
    end_address: u32,
    /// The offset of the channel list within the INFO section.
    channels_offset: u32,
    /// The offset of the DATA section from the INFO section.
    data_offset: u32,
    unk_18: u32,
}

impl<R: Read + ?Sized> ReadFrom<R> for InfoHeader {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let codec_byte = reader.read_u8()?;
        let codec = match Codec::try_from(codec_byte) {
            Ok(c) => c,
            Err(_) => {
                error!("Unsupported RWAV codec: {}", codec_byte);
                return Err(Error::InvalidRwav);
            }
        };
        let looping = reader.read_u8()? != 0;
        let num_channels = reader.read_u8()?;
        if !(1..=2).contains(&num_channels) {
            error!("Only mono or stereo RWAV files are supported");
            return Err(Error::InvalidRwav);
        }
        let _pad = reader.read_u8()?;
        let sample_rate = reader.read_u16::<BE>()?;
        let _pad = reader.read_u16::<BE>()?;
        Ok(Self {
            codec,
            looping,
            num_channels,
            sample_rate,
            start_address: reader.read_u32::<BE>()?,
            end_address: reader.read_u32::<BE>()?,
            channels_offset: reader.read_u32::<BE>()?,
            data_offset: reader.read_u32::<BE>()?,
            unk_18: reader.read_u32::<BE>()?,
        })
    }
}

/// The header for channel data. `channels_offset` in `InfoHeader` points to a list of offsets to
/// these structures.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct ChannelHeader {
    /// The offset of the channel data within the DATA section.
    data_offset: u32,
    /// The offset of the channel's codec info within the INFO section.
    codec_info_offset: u32,
    unk_08: u32,
    unk_0c: u32,
    unk_10: u32,
    unk_14: u32,
    unk_18: u32,
}

impl<R: Read + ?Sized> ReadFrom<R> for ChannelHeader {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self {
            data_offset: reader.read_u32::<BE>()?,
            codec_info_offset: reader.read_u32::<BE>()?,
            unk_08: reader.read_u32::<BE>()?,
            unk_0c: reader.read_u32::<BE>()?,
            unk_10: reader.read_u32::<BE>()?,
            unk_14: reader.read_u32::<BE>()?,
            unk_18: reader.read_u32::<BE>()?,
        })
    }
}

/// Codec info for ADPCM sounds.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct AdpcmCodecInfo {
    adpcm: adpcm::Info,
    loop_context: adpcm::FrameContext,
}

impl<R: Read + ?Sized> ReadFrom<R> for AdpcmCodecInfo {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let info = Self {
            adpcm: ReadFrom::read_from(reader)?,
            loop_context: ReadFrom::read_from(reader)?,
        };
        let _pad = reader.read_u16::<BE>()?;
        Ok(info)
    }
}

/// The INFO section data in an RWAV.
#[derive(Debug, Clone, PartialEq, Eq)]
struct InfoSection {
    header: InfoHeader,
    channels: ArrayVec<ChannelHeader, 2>,
    codec_info: ArrayVec<AdpcmCodecInfo, 2>,
}

impl<R: Read + Seek + ?Sized> ReadFrom<R> for InfoSection {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        // Wrap the reader in a region which locks it to the section data. All seeks will now be
        // relative to the start of the section.
        let section_header = SectionHeader::read_from(reader)?;
        let start_offset = reader.stream_position()?;
        let mut section =
            Region::new(reader, start_offset, section_header.size as u64 - SECTION_HEADER_SIZE);
        let header = InfoHeader::read_from(&mut section)?;

        // channels_offset is an offset to a list of offsets to channel headers
        section.seek(SeekFrom::Start(header.channels_offset as u64))?;
        let mut channel_offsets: ArrayVec<u32, 2> = ArrayVec::new();
        for _ in 0..header.num_channels {
            channel_offsets.push(section.read_u32::<BE>()?);
        }

        let mut channels = ArrayVec::new();
        let mut codec_info = ArrayVec::new();
        for offset in channel_offsets {
            section.seek(SeekFrom::Start(offset as u64))?;
            let channel = ChannelHeader::read_from(&mut section)?;
            channels.push(channel);
            if header.codec == Codec::GcAdpcm {
                section.seek(SeekFrom::Start(channel.codec_info_offset as u64))?;
                codec_info.push(AdpcmCodecInfo::read_from(&mut section)?);
            }
        }

        Ok(Self { header, channels, codec_info })
    }
}

/// Data for one channel in an RWAV file.
#[derive(Clone)]
pub struct Channel {
    /// ADPCM parameters.
    pub adpcm: adpcm::Info,
    /// ADPCM loop context.
    pub loop_context: adpcm::FrameContext,
    /// The channel's audio data.
    pub data: Vec<u8>,
}

#[allow(clippy::missing_fields_in_debug)]
impl Debug for Channel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Channel")
            .field("adpcm", &self.adpcm)
            .field("loop_context", &self.loop_context)
            .finish()
    }
}

#[derive(Clone)]
struct DataSection {
    start_address: u32,
    end_address: u32,
    channels: ArrayVec<Channel, 2>,
}

impl DataSection {
    fn read_from(reader: &mut (impl Read + Seek + ?Sized), info: &InfoSection) -> Result<Self> {
        let format = Format::from(info.header.codec);
        let mut start_address = info.header.start_address;
        let mut end_address = info.header.end_address;

        // Wrap the reader in a region which locks it to the DATA section. All seeks will now be
        // relative to the start of the DATA section.
        let section_header = SectionHeader::read_from(reader)?;
        let start_offset = reader.stream_position()?;
        let mut section =
            Region::new(reader, start_offset, section_header.size as u64 - SECTION_HEADER_SIZE);

        // Read the data for each channel
        let mut channels = ArrayVec::new();
        for (i, channel) in info.channels.iter().enumerate() {
            // Correct the data offset and addresses in the INFO section so that we don't include
            // data that never gets decoded. This is not strictly necessary, but it is more
            // technically correct and ensures we don't read data we don't need.
            let start_aligned = format.frame_address(start_address as usize);
            start_address -= start_aligned as u32;
            end_address -= start_aligned as u32;
            let start_offset = channel.data_offset as usize + format.address_to_byte(start_aligned);
            let size = format.address_to_byte_up((end_address + 1) as usize);

            section.seek(SeekFrom::Start(start_offset as u64))?;
            let mut data = vec![];
            section.by_ref().take(size as u64).read_to_end(&mut data)?;
            if data.len() != size {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
            }

            let (adpcm, loop_context) = if format == Format::GcAdpcm {
                (info.codec_info[i].adpcm, info.codec_info[i].loop_context)
            } else {
                Default::default()
            };

            channels.push(Channel { adpcm, loop_context, data });
        }

        Ok(Self { start_address, end_address, channels })
    }
}

/// An RWAV audio file.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Rwav {
    /// The audio format.
    pub format: Format,
    /// True if the sound should loop.
    pub looping: bool,
    /// The audio sample rate.
    pub sample_rate: u32,
    /// The start address of the sample data.
    pub start_address: u32,
    /// The end address of the sample data.
    pub end_address: u32,
    /// The data for each channel in the sound.
    pub channels: ArrayVec<Channel, 2>,
    /// The audio source tag for debugging purposes.
    pub tag: SourceTag,
}

impl Rwav {
    pub fn open(reader: &mut dyn ReadSeek, tag: impl Into<SourceTag>) -> Result<Self> {
        Self::open_impl(reader, tag.into())
    }

    fn open_impl(reader: &mut dyn ReadSeek, tag: SourceTag) -> Result<Self> {
        let header = FileHeader::read_from(reader)?;

        reader.seek(SeekFrom::Start(header.info_offset as u64))?;
        let info = InfoSection::read_from(reader)?;

        reader.seek(SeekFrom::Start(header.data_offset as u64))?;
        let data = DataSection::read_from(reader, &info)?;

        Ok(Self {
            format: info.header.codec.into(),
            looping: info.header.looping,
            sample_rate: info.header.sample_rate as u32,
            start_address: data.start_address,
            end_address: data.end_address,
            channels: data.channels,
            tag,
        })
    }

    /// Creates a `ChannelReader` over a channel in the stream.
    /// ***Panics*** if the channel index is out-of-bounds.
    pub fn reader(&self, channel: usize) -> ChannelReader<'_> {
        assert!(channel < self.channels.len(), "invalid channel index");
        let tag = match (self.channels.len(), channel) {
            (2, 0) => self.tag.clone().for_channel(SourceChannel::Left),
            (2, 1) => self.tag.clone().for_channel(SourceChannel::Right),
            _ => self.tag.clone(),
        };
        ChannelReader {
            channel: Some(&self.channels[channel]),
            format: self.format,
            sample_rate: self.sample_rate,
            end_address: self.end_address,
            tag,
        }
    }

    /// Creates a decoder which decodes the samples in `channel` into PCM16 format.
    /// ***Panics*** if the channel index is out-of-bounds.
    pub fn channel_decoder(&self, channel: usize) -> RwavDecoder<'_, '_> {
        let reader = self.reader(channel);
        Box::new(adpcm::Decoder::new(reader.cast()))
    }

    /// Creates a decoder which decodes all channels into PCM16 format and joins them.
    pub fn decoder(&self) -> RwavDecoder<'_, '_> {
        if self.channels.len() == 1 {
            self.channel_decoder(0)
        } else {
            let left = self.channel_decoder(0);
            let right = self.channel_decoder(1);
            Box::new(left.with_right_channel(right))
        }
    }
}

/// Reads sample data from a single RWAV channel.
pub struct ChannelReader<'a> {
    channel: Option<&'a Channel>,
    format: Format,
    sample_rate: u32,
    end_address: u32,
    tag: SourceTag,
}

impl<'a> ReadSamples<'a> for ChannelReader<'a> {
    type Format = AnyFormat;

    fn read_samples(&mut self) -> Result<Option<Samples<'a, Self::Format>>> {
        let channel = match self.channel.take() {
            Some(c) => c,
            None => return Ok(None),
        };
        let format = self.format;
        match format {
            Format::GcAdpcm => Ok(Some(
                Samples::<'a, GcAdpcm> {
                    channels: 1,
                    rate: self.sample_rate,
                    len: self.end_address as usize + 1,
                    data: Cow::Borrowed(&channel.data),
                    params: channel.adpcm,
                }
                .cast(),
            )),
            other => {
                // TODO
                error!("Sample format not supported yet: {:?}", other);
                Err(Error::UnsupportedFormat(format))
            }
        }
    }

    fn format(&self) -> Format {
        self.format
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
            Some(_) => Some(self.end_address as u64 + 1),
            None => Some(0),
        }
    }

    fn cues(&self) -> Box<dyn Iterator<Item = Cue> + '_> {
        Box::from(iter::empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[rustfmt::skip]
    const RWAV_BYTES: &[u8] = &[
        0x52, 0x57, 0x41, 0x56, // magic
        0xfe, 0xff, // endian
        0x01, 0x02, // version
        0x00, 0x00, 0x01, 0x28, // file_size
        0x00, 0x20, // header_size
        0x00, 0x02, // num_sections
        0x00, 0x00, 0x00, 0x20, // info_offset
        0x00, 0x00, 0x00, 0xe0, // info_size
        0x00, 0x00, 0x01, 0x00, // data_offset
        0x00, 0x00, 0x00, 0x28, // data_size

        // info section
        0x49, 0x4e, 0x46, 0x4f, // magic
        0x00, 0x00, 0x00, 0xe0, // size
        0x02, // codec
        0x00, // looping
        0x02, // num_channels
        0x00,
        0xac, 0x44, // sample_rate
        0x00, 0x00,
        0x00, 0x00, 0x00, 0x02, // start_address
        0x00, 0x00, 0x00, 0x1f, // end_address
        0x00, 0x00, 0x00, 0x1c, // channels_offset
        0x00, 0x00, 0x00, 0xd8, // data_offset
        0x00, 0x00, 0x00, 0x00, // unk_18

        // channels
        0x00, 0x00, 0x00, 0x24, // channels[0] offset
        0x00, 0x00, 0x00, 0x40, // channels[1] offset

        // channels[0]
        0x00, 0x00, 0x00, 0x00, // data_offset
        0x00, 0x00, 0x00, 0x5c, // codec_info_offset
        0x01, 0x00, 0x00, 0x00, // unk_08
        0x01, 0x00, 0x00, 0x00, // unk_0c
        0x01, 0x00, 0x00, 0x00, // unk_10
        0x01, 0x00, 0x00, 0x00, // unk_14
        0x00, 0x00, 0x00, 0x00, // unk_18

        // channels[1]
        0x00, 0x00, 0x00, 0x10, // data_offset
        0x00, 0x00, 0x00, 0x8c, // codec_info_offset
        0x01, 0x00, 0x00, 0x00, // unk_08
        0x01, 0x00, 0x00, 0x00, // unk_0c
        0x01, 0x00, 0x00, 0x00, // unk_10
        0x01, 0x00, 0x00, 0x00, // unk_14
        0x00, 0x00, 0x00, 0x00, // unk_18

        // channels[0].codec_info
        0x00, 0x00, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03, // coefficients[0..4]
        0x00, 0x04, 0x00, 0x05, 0x00, 0x06, 0x00, 0x07, // coefficient[4..8]
        0x00, 0x08, 0x00, 0x09, 0x00, 0x0a, 0x00, 0x0b, // coefficient[8..12]
        0x00, 0x0c, 0x00, 0x0d, 0x00, 0x0e, 0x00, 0x0f, // coefficient[12..16]
        0x00, 0x00, // gain
        0x00, 0x17, 0x00, 0x00, 0x00, 0x00, // context
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // loop_context
        0x00, 0x00,

        // channels[1].codec_info
        0x00, 0x00, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03, // coefficients[0..4]
        0x00, 0x04, 0x00, 0x05, 0x00, 0x06, 0x00, 0x07, // coefficients[4..8]
        0x00, 0x08, 0x00, 0x09, 0x00, 0x0a, 0x00, 0x0b, // coefficients[8..12]
        0x00, 0x0c, 0x00, 0x0d, 0x00, 0x0e, 0x00, 0x0f, // coefficients[12..16]
        0x00, 0x00, // gain
        0x00, 0x17, 0x00, 0x00, 0x00, 0x00, // context
        0x00, 0x17, 0x00, 0x00, 0x00, 0x00, // loop_context
        0x00, 0x00,

        // padding
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,

        // data section
        0x44, 0x41, 0x54, 0x41, // magic
        0x00, 0x00, 0x00, 0x28, // size

        // channels[0].data
        0x17, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
        0x17, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,

        // channels[1].data
        0x17, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17,
        0x17, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
    ];

    #[test]
    fn test_read_rwav() -> Result<()> {
        let rwav = Rwav::open(&mut Cursor::new(RWAV_BYTES), "RWAV_BYTES")?;
        assert_eq!(rwav.format, Format::GcAdpcm);
        assert!(!rwav.looping);
        assert_eq!(rwav.sample_rate, 44100);
        assert_eq!(rwav.start_address, 0x2);
        assert_eq!(rwav.end_address, 0x1f);
        assert_eq!(rwav.channels.len(), 2);

        let channel0 = &rwav.channels[0];
        let expected_coefficients = (0..16).collect::<Vec<i16>>();
        assert_eq!(channel0.adpcm.coefficients, expected_coefficients.as_slice());
        assert_eq!(channel0.data, &RWAV_BYTES[0x108..0x118]);

        let channel1 = &rwav.channels[1];
        assert_eq!(channel1.adpcm.coefficients, expected_coefficients.as_slice());
        assert_eq!(channel1.data, &RWAV_BYTES[0x118..0x128]);

        let samples0 = rwav.reader(0).read_samples()?.unwrap();
        let samples0: Samples<'_, GcAdpcm> = samples0.cast();
        assert_eq!(samples0.format(), Format::GcAdpcm);
        assert_eq!(samples0.len, 32);
        assert_eq!(samples0.channels, 1);
        assert_eq!(samples0.data, &RWAV_BYTES[0x108..0x118]);

        let samples1 = rwav.reader(1).read_samples()?.unwrap();
        let samples1: Samples<'_, GcAdpcm> = samples1.cast();
        assert_eq!(samples1.format(), Format::GcAdpcm);
        assert_eq!(samples1.len, 32);
        assert_eq!(samples1.channels, 1);
        assert_eq!(samples1.data, &RWAV_BYTES[0x118..0x128]);
        Ok(())
    }
}
