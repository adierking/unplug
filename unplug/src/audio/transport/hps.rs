mod reader;
mod writer;

pub use reader::*;
pub use writer::*;

use crate::audio::format::adpcm::{self, GcAdpcm};
use crate::audio::format::dsp::{AudioAddress, DspFormat};
use crate::audio::{Error, Result};
use crate::common::{align, ReadFrom, WriteTo};
use byteorder::{ReadBytesExt, WriteBytesExt, BE};
use std::fmt::Debug;
use std::io::{Read, Write};
use unplug_proc::{ReadFrom, WriteTo};

/// The magic string at the beginning of an HPS file. (HALPST = HAL Program STream)
const HPS_MAGIC: [u8; 8] = *b" HALPST\0";

/// The offset of the first block in an HPS file.
const FIRST_BLOCK_OFFSET: u32 = 0x80;
/// The block offset indicating the end of the stream.
const END_BLOCK_OFFSET: u32 = u32::MAX;

/// The alignment of data in an HPS file.
const DATA_ALIGN: usize = 0x20;

/// The ADPCM encoder block size for mono audio data.
const MONO_BLOCK_SIZE: usize = 0x10000;
/// The ADPCM encoder block size for stereo audio data.
const STEREO_BLOCK_SIZE: usize = MONO_BLOCK_SIZE / 2;

/// HPS file header.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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

impl FileHeader {
    fn new() -> Self {
        Self { magic: HPS_MAGIC, sample_rate: 0, num_channels: 0, channels: Default::default() }
    }
}

impl Default for FileHeader {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: Read + ?Sized> ReadFrom<R> for FileHeader {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let mut header = Self::default();
        reader.read_exact(&mut header.magic)?;
        if header.magic != HPS_MAGIC {
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

impl<W: Write + ?Sized> WriteTo<W> for FileHeader {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_all(&self.magic)?;
        writer.write_u32::<BE>(self.sample_rate)?;
        writer.write_u32::<BE>(self.num_channels)?;
        Channel::write_all_to(writer, &self.channels)?;
        Ok(())
    }
}

/// Audio channel header.
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, ReadFrom, WriteTo)]
#[read_from(error = Error)]
#[write_to(error = Error)]
pub struct Channel {
    pub address: AudioAddress,
    pub adpcm: adpcm::Info,
}

impl Channel {
    fn is_initialized(&self) -> bool {
        self.address.current_address > 0 && self.address.end_address > 0
    }
}

/// Size of a cue in bytes.
const CUE_SIZE: usize = 0x8;

/// A cue point in an audio stream which allows the game to trigger events when it is reached.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct CuePoint {
    /// Index of the sample where the cue point is at.
    pub sample_index: i32,
    /// User-assigned ID value for this cue.
    pub id: u32,
}

impl Default for CuePoint {
    fn default() -> Self {
        Self { sample_index: -1, id: 0 }
    }
}

impl<R: Read + ?Sized> ReadFrom<R> for CuePoint {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Ok(Self { sample_index: reader.read_i32::<BE>()?, id: reader.read_u32::<BE>()? })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for CuePoint {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_i32::<BE>(self.sample_index)?;
        writer.write_u32::<BE>(self.id)?;
        Ok(())
    }
}

/// Size of a block header without cues in bytes.
const BLOCK_HEADER_SIZE: usize = 0x20;

/// Calculates the number of samples in a block according to `format`.
fn num_samples(end_address: u32, format: DspFormat) -> u64 {
    let len = end_address as usize + 1;
    match format {
        DspFormat::Adpcm => GcAdpcm::address_to_sample(len) as u64,
        DspFormat::Pcm16 | DspFormat::Pcm8 => len as u64,
    }
}

/// Audio block header.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BlockHeader {
    /// Total size (in bytes) of the audio data in the block.
    pub size: u32,
    /// The end address of the audio data.
    pub end_address: u32,
    /// The offset of the next block to play.
    pub next_offset: u32,
    /// Initial playback parameters for each channel.
    pub channel_contexts: [adpcm::FrameContext; 2],
    /// Cue points in this block.
    pub cues: Vec<CuePoint>,
}

impl BlockHeader {
    fn file_size(&self) -> u64 {
        (BLOCK_HEADER_SIZE + align(self.cues.len() * CUE_SIZE, DATA_ALIGN)) as u64
    }
}

impl<R: Read + ?Sized> ReadFrom<R> for BlockHeader {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let mut header = Self {
            size: reader.read_u32::<BE>()?,
            end_address: reader.read_u32::<BE>()?,
            next_offset: reader.read_u32::<BE>()?,
            ..Default::default()
        };

        for context in &mut header.channel_contexts {
            *context = adpcm::FrameContext::read_from(reader)?;
            let _padding = reader.read_u16::<BE>()?;
        }

        let num_cues = reader.read_u8()?;
        let _padding = reader.read_u24::<BE>()?;

        // Cue point data follows the header if it's present
        for _ in 0..num_cues {
            header.cues.push(CuePoint::read_from(reader)?);
        }

        // In order to preserve alignment, data is padded with extra cues
        let aligned = align(num_cues, DATA_ALIGN / CUE_SIZE);
        for _ in (num_cues as usize)..aligned {
            CuePoint::read_from(reader)?;
        }

        Ok(header)
    }
}

impl<W: Write + ?Sized> WriteTo<W> for BlockHeader {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_u32::<BE>(self.size)?;
        writer.write_u32::<BE>(self.end_address)?;
        writer.write_u32::<BE>(self.next_offset)?;
        for context in &self.channel_contexts {
            context.write_to(writer)?;
            writer.write_u16::<BE>(0)?; // padding
        }
        writer.write_u8(self.cues.len() as u8)?;
        writer.write_u24::<BE>(0)?; // padding
        for cue in &self.cues {
            cue.write_to(writer)?;
        }
        let aligned = align(self.cues.len(), DATA_ALIGN / CUE_SIZE);
        for _ in self.cues.len()..aligned {
            // Writing default cues gives us parity with the official files
            CuePoint::default().write_to(writer)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_write_and_read;
    use std::convert::TryInto;
    use std::io::{Cursor, Seek, SeekFrom};

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

        assert_eq!(header.magic, HPS_MAGIC);
        assert_eq!(header.sample_rate, 44100);
        assert_eq!(header.num_channels, 1);

        let ch = &header.channels[0];
        assert!(!ch.address.looping);
        assert_eq!(ch.address.format, DspFormat::Adpcm);
        assert_eq!(ch.address.loop_address, 1);
        assert_eq!(ch.address.end_address, 3);
        assert_eq!(ch.address.current_address, 2);
        for i in 0..16 {
            assert_eq!(ch.adpcm.coefficients[i], (i + 1) as i16);
        }
        assert_eq!(ch.adpcm.gain, 4);
        assert_eq!(ch.adpcm.context.predictor_and_scale, 5);
        assert_eq!(ch.adpcm.context.last_samples, [6, 7]);

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

        assert_eq!(header.magic, HPS_MAGIC);
        assert_eq!(header.sample_rate, 44100);
        assert_eq!(header.num_channels, 2);

        let ch0 = &header.channels[0];
        assert!(!ch0.address.looping);
        assert_eq!(ch0.address.format, DspFormat::Adpcm);
        assert_eq!(ch0.address.loop_address, 1);
        assert_eq!(ch0.address.end_address, 3);
        assert_eq!(ch0.address.current_address, 2);
        for i in 0..16 {
            assert_eq!(ch0.adpcm.coefficients[i], (i + 1) as i16);
        }
        assert_eq!(ch0.adpcm.gain, 4);
        assert_eq!(ch0.adpcm.context.predictor_and_scale, 5);
        assert_eq!(ch0.adpcm.context.last_samples, [6, 7]);

        let ch1 = &header.channels[1];
        assert!(ch1.address.looping);
        assert_eq!(ch1.address.format, DspFormat::Pcm16);
        assert_eq!(ch1.address.loop_address, 8);
        assert_eq!(ch1.address.end_address, 10);
        assert_eq!(ch1.address.current_address, 9);
        for i in 0..16 {
            assert_eq!(ch1.adpcm.coefficients[i], (i + 1) as i16);
        }
        assert_eq!(ch1.adpcm.gain, 11);
        assert_eq!(ch1.adpcm.context.predictor_and_scale, 12);
        assert_eq!(ch1.adpcm.context.last_samples, [13, 14]);

        Ok(())
    }

    #[test]
    fn test_write_and_read_header() {
        assert_write_and_read!(FileHeader {
            magic: HPS_MAGIC,
            sample_rate: 44100,
            num_channels: 1,
            channels: [
                Channel {
                    address: AudioAddress {
                        looping: true,
                        format: DspFormat::Adpcm,
                        loop_address: 1,
                        end_address: 2,
                        current_address: 3,
                    },
                    adpcm: adpcm::Info {
                        coefficients: (1..=16).collect::<Vec<_>>().try_into().unwrap(),
                        gain: 4,
                        context: adpcm::FrameContext {
                            predictor_and_scale: 5,
                            last_samples: [6, 7],
                        },
                    },
                },
                Default::default(),
            ]
        });
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

            0x00, // num_cues
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
        assert_eq!(header.cues.len(), 0);

        Ok(())
    }

    #[test]
    fn test_read_block_header_with_cues() -> Result<()> {
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

            0x02, // num_cues
            0x00, 0x00, 0x00, // padding

            0x00, 0x00, 0x00, 0x0a, // cues[0].sample_index
            0x00, 0x00, 0x00, 0x0b, // cues[0].id
            0x00, 0x00, 0x00, 0x0c, // cues[1].sample_index
            0x00, 0x00, 0x00, 0x0d, // cues[1].id
            0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, // padding cue
            0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, // padding cue
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
            header.cues,
            [CuePoint { sample_index: 10, id: 11 }, CuePoint { sample_index: 12, id: 13 }]
        );

        Ok(())
    }

    #[test]
    fn test_write_and_read_block_header() {
        assert_write_and_read!(BlockHeader {
            size: 1,
            end_address: 2,
            next_offset: 3,
            channel_contexts: [
                adpcm::FrameContext { predictor_and_scale: 4, last_samples: [5, 6] },
                adpcm::FrameContext { predictor_and_scale: 7, last_samples: [8, 9] },
            ],
            cues: vec![
                CuePoint { sample_index: 10, id: 11 },
                CuePoint { sample_index: 12, id: 13 },
            ],
        });
    }
}
