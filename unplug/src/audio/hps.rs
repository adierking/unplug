use super::adpcm::{Context, Decoder, GcAdpcm};
use super::format::{AnyFormat, Format, GcFormat, PcmS16Le};
use super::sample::{CastSamples, JoinChannels};
use super::{Error, ReadSamples, Result, Samples};
use crate::common::ReadFrom;
use arrayvec::ArrayVec;
use byteorder::{ReadBytesExt, BE};
use log::{debug, error, trace};
use std::collections::HashMap;
use std::fmt::{self, Debug};
use std::io::{Read, Seek, SeekFrom};

/// The magic string at the beginning of an HPS file.
const HPS_MAGIC: &[u8; 8] = b" HALPST\0";

/// The offset of the first block in an HPS file.
const FIRST_BLOCK_OFFSET: u32 = 0x80;
/// The block offset indicating the end of the stream.
const END_BLOCK_OFFSET: u32 = u32::MAX;

/// The alignment of data in an HPS file.
const DATA_ALIGN: usize = 0x20;

/// Convenience type for an opaque decoder.
type HpsDecoder<'a> = Box<dyn ReadSamples<'static, Format = PcmS16Le> + 'a>;

/// Aligns `offset` to the next multiple of the data alignment.
fn align(offset: usize) -> usize {
    (offset + DATA_ALIGN - 1) & !(DATA_ALIGN - 1)
}

/// HPS file header.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
struct FileHeader {
    /// Must be HPS_MAGIC.
    magic: [u8; 8],
    /// The sample rate (e.g. 44100).
    sample_rate: u32,
    /// Number of channels (1 or 2).
    num_channels: u32,
    /// Channel headers.
    channels: [Channel; 2],
}

impl<R: Read> ReadFrom<R> for FileHeader {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let mut header = Self::default();
        reader.read_exact(&mut header.magic)?;
        if header.magic != *HPS_MAGIC {
            return Err(Error::InvalidHpsMagic);
        }
        header.sample_rate = reader.read_u32::<BE>()?;
        header.num_channels = reader.read_u32::<BE>()?;
        if !(1..=2).contains(&header.num_channels) {
            return Err(Error::InvalidChannelCount(header.num_channels));
        }
        Channel::read_all_from(reader, &mut header.channels)?;
        Ok(header)
    }
}

/// Audio channel header.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct Channel {
    /// True if the channel is looping. Note that this is a DSP parameter and should always be true,
    /// even if the HPS as a whole does not loop.
    pub looping: bool,
    /// Format of each sample in the channel.
    pub format: GcFormat,
    /// Address of the sample that looping starts at. Unused and always points to the first sample.
    pub start_address: u32,
    /// Address of the last sample. Unused.
    pub end_address: u32,
    /// Address that the DSP should begin at. Unused and always points to the first sample.
    pub current_address: u32,
    /// ADPCM coefficients.
    pub coefficients: [i16; 16],
    /// Audio gain level.
    pub gain: u16,
    /// Initial decoder parameters.
    pub initial_context: Context,
}

impl<R: Read> ReadFrom<R> for Channel {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let mut channel = Self {
            looping: reader.read_u16::<BE>()? != 0,
            format: GcFormat::read_from(reader)?,
            start_address: reader.read_u32::<BE>()?,
            end_address: reader.read_u32::<BE>()?,
            current_address: reader.read_u32::<BE>()?,
            ..Default::default()
        };
        reader.read_i16_into::<BE>(&mut channel.coefficients)?;
        channel.gain = reader.read_u16::<BE>()?;
        channel.initial_context = Context::read_from(reader)?;
        Ok(channel)
    }
}

/// A marker in an audio stream which allows the game to trigger events when it is reached.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Marker {
    /// Index of the sample where the marker is at.
    pub sample_index: i32,
    /// User-assigned ID value for this marker.
    pub id: u32,
}

impl Default for Marker {
    fn default() -> Self {
        Self { sample_index: -1, id: 0 }
    }
}

impl<R: Read> ReadFrom<R> for Marker {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self { sample_index: reader.read_i32::<BE>()?, id: reader.read_u32::<BE>()? })
    }
}

/// Audio block header.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct BlockHeader {
    /// Total size (in bytes) of the audio data in the block.
    size: u32,
    /// The end address of the audio data.
    end_address: u32,
    /// The offset of the next block to play.
    next_offset: u32,
    /// Initial decoder parameters for each channel.
    channel_contexts: [Context; 2],
    /// Markers in this block.
    markers: Vec<Marker>,
}

impl<R: Read> ReadFrom<R> for BlockHeader {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let mut header = Self {
            size: reader.read_u32::<BE>()?,
            end_address: reader.read_u32::<BE>()?,
            next_offset: reader.read_u32::<BE>()?,
            ..Default::default()
        };

        for context in &mut header.channel_contexts {
            *context = Context::read_from(reader)?;
            let _padding = reader.read_u16::<BE>()?;
        }

        let num_markers = reader.read_u8()?;
        let _padding = reader.read_u24::<BE>()?;

        // Marker data follows the header if it's present
        for _ in 0..num_markers {
            header.markers.push(Marker::read_from(reader)?);
        }

        // In order to preserve alignment, data is always reserved for a multiple of 4 markers
        let aligned = (num_markers + 3) & !3;
        for _ in num_markers..aligned {
            Marker::read_from(reader)?;
        }

        Ok(header)
    }
}

/// An HPS audio stream.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct HpsStream {
    /// The stream's sample rate in Hz (e.g. 44100).
    pub sample_rate: u32,
    /// Information about each channel in the stream.
    pub channels: ArrayVec<[Channel; 2]>,
    /// The index of the block to loop back to when the end is reached.
    pub loop_start: Option<usize>,
    /// The blocks making up the stream data.
    pub blocks: Vec<Block>,
}

impl HpsStream {
    /// Creates a `ChannelReader` over a channel in the stream.
    /// ***Panics*** if the channel index is out-of-bounds.
    pub fn reader(&self, channel: usize) -> ChannelReader<'_> {
        assert!(channel < self.channels.len(), "invalid channel index");
        ChannelReader { blocks: &self.blocks, channel, format: self.channels[channel].format }
    }

    /// Creates a decoder which decodes all channels into PCM16 format and joins them.
    pub fn decoder(&self) -> HpsDecoder<'_> {
        if self.channels.len() == 1 {
            self.channel_decoder(0)
        } else {
            let left = self.channel_decoder(0);
            let right = self.channel_decoder(1);
            Box::new(JoinChannels::new(left, right))
        }
    }

    /// Creates a decoder which decodes the samples in `channel` into PCM16 format.
    /// ***Panics*** if the channel index is out-of-bounds.
    pub fn channel_decoder(&self, channel: usize) -> HpsDecoder<'_> {
        let reader = self.reader(channel);
        let casted = CastSamples::new(reader);
        Box::new(Decoder::new(Box::new(casted), &self.channels[channel].coefficients))
    }
}

impl<R: Read + Seek> ReadFrom<R> for HpsStream {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let header = FileHeader::read_from(reader)?;
        let channels: ArrayVec<[Channel; 2]> =
            header.channels.iter().take(header.num_channels as usize).copied().collect();

        let mut blocks = vec![];
        let mut blocks_by_offset = HashMap::new();
        let mut loop_start = None;
        let mut current_offset = FIRST_BLOCK_OFFSET;
        loop {
            reader.seek(SeekFrom::Start(current_offset as u64))?;
            let block_header = BlockHeader::read_from(reader)?;
            let block = Block::read_from(reader, &block_header, &channels)?;
            blocks_by_offset.insert(current_offset, blocks.len());
            trace!("Block {:#x}: {:?}", current_offset, block);
            blocks.push(block);

            // Advance to the offset in the block header, unless it's the end or we've already
            // visited the next block.
            let next_offset = block_header.next_offset;
            if next_offset == END_BLOCK_OFFSET {
                break;
            }
            let next_index = blocks_by_offset.get(&next_offset).copied();
            if let Some(index) = next_index {
                // Looping back to a previous block
                loop_start = Some(index);
                break;
            }
            current_offset = next_offset;
        }

        debug!(
            "Loaded HPS stream: {} Hz, {}, {} blocks",
            header.sample_rate,
            if channels.len() == 2 { "stereo" } else { "mono" },
            blocks.len(),
        );
        Ok(Self { sample_rate: header.sample_rate, channels, loop_start, blocks })
    }
}

/// A block within an HPS stream.
#[derive(Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Block {
    /// The end address of the audio data in the block.
    pub end_address: u32,
    /// The info for each channel in the block.
    pub channels: ArrayVec<[BlockChannel; 2]>,
    /// The markers in the block.
    pub markers: Vec<Marker>,
    /// The complete block data.
    pub data: Vec<u8>,
}

impl Block {
    /// Creates an empty `Block`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a slice over the data for channel `i`.
    pub fn channel_data(&self, i: usize) -> &[u8] {
        let ch = &self.channels[i];
        let start = ch.data_offset;
        let end = start + ch.data_size;
        &self.data[start..end]
    }

    /// Returns a mutable slice over the data for channel `i`.
    pub fn channel_data_mut(&mut self, i: usize) -> &mut [u8] {
        let ch = &self.channels[i];
        let start = ch.data_offset;
        let end = start + ch.data_size;
        &mut self.data[start..end]
    }

    /// Reads block data from `reader` using information from `header` and `channels`.
    fn read_from<R: Read + Seek>(
        reader: &mut R,
        header: &BlockHeader,
        channels: &[Channel],
    ) -> Result<Block> {
        let data_size = header.size as usize;
        let mut data = Vec::with_capacity(data_size);
        reader.take(data_size as u64).read_to_end(&mut data)?;

        // Calculate the size of each channel and use this to determine their offsets. Channel
        // data is aligned to a multiple of the alignment size.
        let mut block_channels = ArrayVec::new();
        let mut data_offset = 0;
        for (i, channel) in channels.iter().enumerate() {
            let format = Format::from(channel.format);
            let data_size = format.size_of(header.end_address as usize + 1);
            block_channels.push(BlockChannel {
                data_offset,
                data_size,
                initial_context: header.channel_contexts[i],
            });
            data_offset = align(data_offset + data_size);
        }

        Ok(Block {
            end_address: header.end_address,
            channels: block_channels,
            markers: header.markers.clone(),
            data,
        })
    }
}

impl Debug for Block {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Block")
            .field("end_address", &self.end_address)
            .field("channels", &self.channels)
            .field("markers", &self.markers)
            .finish()
    }
}

/// A channel within an HPS block.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct BlockChannel {
    /// The offset of the channel data within the block's data.
    pub data_offset: usize,
    /// The size (in bytes) of the channel data.
    pub data_size: usize,
    /// Initial decoder parameters.
    pub initial_context: Context,
}

impl BlockChannel {
    /// Creates an empty `BlockChannel`.
    pub fn new() -> Self {
        Self::default()
    }
}

/// Reads sample data from a single HPS channel.
pub struct ChannelReader<'a> {
    blocks: &'a [Block],
    channel: usize,
    format: GcFormat,
}

impl<'a> ReadSamples<'a> for ChannelReader<'a> {
    type Format = AnyFormat;
    fn read_samples(&mut self) -> Result<Option<Samples<'a, Self::Format>>> {
        if self.blocks.is_empty() {
            return Ok(None);
        }
        let block = &self.blocks[0];
        self.blocks = &self.blocks[1..];
        match self.format {
            GcFormat::Adpcm => Ok(Some(
                Samples::<GcAdpcm> {
                    context: block.channels[self.channel].initial_context,
                    start_address: 0,
                    end_address: block.end_address as usize,
                    channels: 1,
                    bytes: block.channel_data(self.channel).into(),
                }
                .into_any(),
            )),
            other => {
                // TODO
                error!("Sample format not supported yet: {:?}", other);
                Err(Error::UnsupportedFormat(self.format.into()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_read_header_mono() -> Result<()> {
        // This is bogus data, the point is just to test the I/O logic.
        #[rustfmt::skip]
        let bytes: &[u8] = &[
            0x20, 0x48, 0x41, 0x4C, 0x50, 0x53, 0x54, 0x00, // magic
            0x00, 0x00, 0xAC, 0x44, // sample_rate
            0x00, 0x00, 0x00, 0x01, // num_channels

            0x00, 0x00, // channels[0].looping
            0x00, 0x00, // channels[0].format
            0x00, 0x00, 0x00, 0x01, // channels[0].start_address
            0x00, 0x00, 0x00, 0x03, // channels[0].end_address
            0x00, 0x00, 0x00, 0x02, // channels[0].current_address

            0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04, // channels[0].coefficients
            0x00, 0x05, 0x00, 0x06, 0x00, 0x07, 0x00, 0x08,
            0x00, 0x09, 0x00, 0x0a, 0x00, 0x0b, 0x00, 0x0c,
            0x00, 0x0d, 0x00, 0x0e, 0x00, 0x0f, 0x00, 0x10,
            0x00, 0x04, // channels[0].gain
            0x00, 0x05, // channels[0].initial_context.predictor_and_scale
            0x00, 0x06, 0x00, 0x07, // channels[0].initial_context.last_samples

            // channels[1] is uninitialized
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];

        let mut reader = Cursor::new(bytes);
        let header = FileHeader::read_from(&mut reader)?;
        assert_eq!(reader.seek(SeekFrom::Current(0))?, FIRST_BLOCK_OFFSET as u64);

        assert_eq!(header.magic, *HPS_MAGIC);
        assert_eq!(header.sample_rate, 44100);
        assert_eq!(header.num_channels, 1);

        let ch = &header.channels[0];
        assert!(!ch.looping);
        assert_eq!(ch.format, GcFormat::Adpcm);
        assert_eq!(ch.start_address, 1);
        assert_eq!(ch.end_address, 3);
        assert_eq!(ch.current_address, 2);
        for i in 0..16 {
            assert_eq!(ch.coefficients[i], (i + 1) as i16);
        }
        assert_eq!(ch.gain, 4);
        assert_eq!(ch.initial_context.predictor_and_scale, 5);
        assert_eq!(ch.initial_context.last_samples, [6, 7]);

        Ok(())
    }

    #[test]
    fn test_read_header_stereo() -> Result<()> {
        // This is bogus data, the point is just to test the I/O logic.
        #[rustfmt::skip]
        let bytes: &[u8] = &[
            0x20, 0x48, 0x41, 0x4C, 0x50, 0x53, 0x54, 0x00, // magic
            0x00, 0x00, 0xAC, 0x44, // sample_rate
            0x00, 0x00, 0x00, 0x02, // num_channels

            0x00, 0x00, // channels[0].looping
            0x00, 0x00, // channels[0].format
            0x00, 0x00, 0x00, 0x01, // channels[0].start_address
            0x00, 0x00, 0x00, 0x03, // channels[0].end_address
            0x00, 0x00, 0x00, 0x02, // channels[0].current_address
            0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04, // channels[0].coefficients
            0x00, 0x05, 0x00, 0x06, 0x00, 0x07, 0x00, 0x08,
            0x00, 0x09, 0x00, 0x0a, 0x00, 0x0b, 0x00, 0x0c,
            0x00, 0x0d, 0x00, 0x0e, 0x00, 0x0f, 0x00, 0x10,
            0x00, 0x04, // channels[0].gain
            0x00, 0x05, // channels[0].initial_context.predictor_and_scale
            0x00, 0x06, 0x00, 0x07, // channels[0].initial_context.last_samples

            0x00, 0x01, // channels[1].looping
            0x00, 0x0a, // channels[1].format
            0x00, 0x00, 0x00, 0x08, // channels[1].start_address
            0x00, 0x00, 0x00, 0x0a, // channels[1].end_address
            0x00, 0x00, 0x00, 0x09, // channels[1].current_address
            0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04, // channels[1].coefficients
            0x00, 0x05, 0x00, 0x06, 0x00, 0x07, 0x00, 0x08,
            0x00, 0x09, 0x00, 0x0a, 0x00, 0x0b, 0x00, 0x0c,
            0x00, 0x0d, 0x00, 0x0e, 0x00, 0x0f, 0x00, 0x10,
            0x00, 0x0b, // channels[1].gain
            0x00, 0x0c, // channels[1].initial_context.predictor_and_scale
            0x00, 0x0d, 0x00, 0x0e, // channels[1].initial_context.last_samples
        ];

        let mut reader = Cursor::new(bytes);
        let header = FileHeader::read_from(&mut reader)?;
        assert_eq!(reader.seek(SeekFrom::Current(0))?, FIRST_BLOCK_OFFSET as u64);

        assert_eq!(header.magic, *HPS_MAGIC);
        assert_eq!(header.sample_rate, 44100);
        assert_eq!(header.num_channels, 2);

        let ch0 = &header.channels[0];
        assert!(!ch0.looping);
        assert_eq!(ch0.format, GcFormat::Adpcm);
        assert_eq!(ch0.start_address, 1);
        assert_eq!(ch0.end_address, 3);
        assert_eq!(ch0.current_address, 2);
        for i in 0..16 {
            assert_eq!(ch0.coefficients[i], (i + 1) as i16);
        }
        assert_eq!(ch0.gain, 4);
        assert_eq!(ch0.initial_context.predictor_and_scale, 5);
        assert_eq!(ch0.initial_context.last_samples, [6, 7]);

        let ch1 = &header.channels[1];
        assert!(ch1.looping);
        assert_eq!(ch1.format, GcFormat::Pcm16);
        assert_eq!(ch1.start_address, 8);
        assert_eq!(ch1.end_address, 10);
        assert_eq!(ch1.current_address, 9);
        for i in 0..16 {
            assert_eq!(ch1.coefficients[i], (i + 1) as i16);
        }
        assert_eq!(ch1.gain, 11);
        assert_eq!(ch1.initial_context.predictor_and_scale, 12);
        assert_eq!(ch1.initial_context.last_samples, [13, 14]);

        Ok(())
    }

    #[test]
    fn test_read_block_header() -> Result<()> {
        // This is bogus data, the point is just to test the I/O logic.
        #[rustfmt::skip]
        let bytes: &[u8] = &[
            0x00, 0x00, 0x00, 0x01, // size
            0x00, 0x00, 0x00, 0x02, // end_address
            0x00, 0x00, 0x00, 0x03, // next_offset

            0x00, 0x04, // channel_contexts[0].predictor_and_scale
            0x00, 0x05, 0x00, 0x06, // channel_contexts[1].last_samples
            0x00, 0x00, // padding

            0x00, 0x07, // channel_contexts[1].predictor_and_scale
            0x00, 0x08, 0x00, 0x09, // channel_contexts[1].last_samples
            0x00, 0x00, // padding

            0x00, // num_markers
            0x00, 0x00, 0x00, // padding
        ];

        let mut reader = Cursor::new(bytes);
        let header = BlockHeader::read_from(&mut reader)?;
        assert_eq!(reader.seek(SeekFrom::Current(0))?, 0x20);

        assert_eq!(header.size, 1);
        assert_eq!(header.end_address, 2);
        assert_eq!(header.next_offset, 3);
        assert_eq!(header.channel_contexts[0].predictor_and_scale, 4);
        assert_eq!(header.channel_contexts[0].last_samples, [5, 6]);
        assert_eq!(header.channel_contexts[1].predictor_and_scale, 7);
        assert_eq!(header.channel_contexts[1].last_samples, [8, 9]);
        assert_eq!(header.markers.len(), 0);

        Ok(())
    }

    #[test]
    fn test_read_block_header_with_markers() -> Result<()> {
        // This is bogus data, the point is just to test the I/O logic.
        #[rustfmt::skip]
        let bytes: &[u8] = &[
            0x00, 0x00, 0x00, 0x01, // size
            0x00, 0x00, 0x00, 0x02, // end_address
            0x00, 0x00, 0x00, 0x03, // next_offset

            0x00, 0x04, // channel_contexts[0].predictor_and_scale
            0x00, 0x05, 0x00, 0x06, // channel_contexts[1].last_samples
            0x00, 0x00, // padding

            0x00, 0x07, // channel_contexts[1].predictor_and_scale
            0x00, 0x08, 0x00, 0x09, // channel_contexts[1].last_samples
            0x00, 0x00, // padding

            0x02, // num_markers
            0x00, 0x00, 0x00, // padding

            0x00, 0x00, 0x00, 0x0a, // markers[0].sample_index
            0x00, 0x00, 0x00, 0x0b, // markers[0].id
            0x00, 0x00, 0x00, 0x0c, // markers[1].sample_index
            0x00, 0x00, 0x00, 0x0d, // markers[1].id
            0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, // padding marker
            0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, // padding marker
        ];

        let mut reader = Cursor::new(bytes);
        let header = BlockHeader::read_from(&mut reader)?;
        assert_eq!(reader.seek(SeekFrom::Current(0))?, 0x40);

        assert_eq!(header.size, 1);
        assert_eq!(header.end_address, 2);
        assert_eq!(header.next_offset, 3);
        assert_eq!(header.channel_contexts[0].predictor_and_scale, 4);
        assert_eq!(header.channel_contexts[0].last_samples, [5, 6]);
        assert_eq!(header.channel_contexts[1].predictor_and_scale, 7);
        assert_eq!(header.channel_contexts[1].last_samples, [8, 9]);
        assert_eq!(
            header.markers,
            [Marker { sample_index: 10, id: 11 }, Marker { sample_index: 12, id: 13 }]
        );

        Ok(())
    }

    #[test]
    fn test_read_block() -> Result<()> {
        let bytes = vec![0u8; 0x40];
        let header = BlockHeader {
            size: 0x40,
            end_address: 0x1f,
            next_offset: 0,
            channel_contexts: [Default::default(); 2],
            markers: vec![],
        };
        let channel = Channel {
            looping: true,
            format: GcFormat::Adpcm,
            start_address: 0x02,
            end_address: 0x1f,
            current_address: 0x02,
            coefficients: [0; 16],
            gain: 0,
            initial_context: Default::default(),
        };
        let channels = [channel, channel];
        let mut reader = Cursor::new(bytes);
        let block = Block::read_from(&mut reader, &header, &channels)?;
        assert_eq!(reader.seek(SeekFrom::Current(0))?, 0x40);

        assert_eq!(block.end_address, 0x1f);
        assert_eq!(block.data.len(), 0x40);

        assert_eq!(block.channels[0].data_offset, 0);
        assert_eq!(block.channels[0].data_size, 0x10);
        assert_eq!(block.channels[1].data_offset, 0x20);
        assert_eq!(block.channels[1].data_size, 0x10);

        Ok(())
    }
}