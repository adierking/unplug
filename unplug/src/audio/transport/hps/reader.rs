use super::{BlockHeader, Channel, FileHeader};
use crate::audio::cue::{Cue, LOOP_PREFIX};
use crate::audio::format::adpcm::{self, GcAdpcm};
use crate::audio::format::dsp::DspFormat;
use crate::audio::format::{AnyFormat, Format, PcmS16Be, PcmS16Le, PcmS8, ReadWriteBytes};
use crate::audio::{ProgressHint, ReadSamples, Result, Samples, SourceChannel, SourceTag};
use crate::common::{align, ReadFrom, ReadSeek};
use arrayvec::ArrayVec;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex};
use tracing::{debug, instrument, trace, warn};

/// Convenience type for an opaque decoder.
type HpsDecoder<'r, 's> = Box<dyn ReadSamples<'s, Format = PcmS16Le> + 'r>;

#[derive(Debug, Clone, PartialEq, Eq)]
struct BlockInfo {
    header: BlockHeader,
    data_offset: u64,
}

struct HpsState {
    /// The stream's sample rate in Hz (e.g. 44100).
    sample_rate: u32,
    /// Information about each channel in the stream.
    channels: ArrayVec<Channel, 2>,
    /// The index of the block to loop back to when the end is reached.
    loop_start: Option<usize>,
    /// The blocks making up the stream data.
    blocks: Vec<BlockInfo>,
}

/// Reads HAL program stream audio from a .hps file.
pub struct HpsReader<'r> {
    reader: Arc<Mutex<Box<dyn ReadSeek + 'r>>>,
    state: Arc<HpsState>,
    /// The audio source tag for debugging purposes.
    tag: SourceTag,
}

impl<'r> HpsReader<'r> {
    /// Opens a program stream read from `reader`. `tag` is a string or tag to identify the stream
    /// for debugging purposes.
    pub fn new(reader: impl ReadSeek + 'r, tag: impl Into<SourceTag>) -> Result<Self> {
        Self::new_impl(Box::from(reader), tag.into())
    }

    #[instrument(level = "trace", skip_all)]
    fn new_impl(mut reader: Box<dyn ReadSeek + 'r>, tag: SourceTag) -> Result<Self> {
        let header = FileHeader::read_from(&mut reader)?;
        let channels: ArrayVec<Channel, 2> =
            header.channels.iter().take(header.num_channels as usize).copied().collect();

        let mut blocks = vec![];
        let mut blocks_by_offset = HashMap::new();
        let mut loop_start = None;
        let mut current_offset = super::FIRST_BLOCK_OFFSET;
        loop {
            reader.seek(SeekFrom::Start(current_offset as u64))?;
            let block_header = BlockHeader::read_from(&mut reader)?;
            let next_offset = block_header.next_offset;
            trace!("Block {:#x}: {:?}", current_offset, block_header);
            let data_offset = (current_offset as u64) + block_header.file_size();
            blocks_by_offset.insert(current_offset, blocks.len());
            blocks.push(BlockInfo { header: block_header, data_offset });

            // Advance to the offset in the block header, unless it's the end or we've already
            // visited the next block.
            if next_offset == super::END_BLOCK_OFFSET {
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
            "Loaded program stream {:?}: {} Hz, {}, {} blocks",
            tag,
            header.sample_rate,
            if channels.len() == 2 { "stereo" } else { "mono" },
            blocks.len(),
        );
        Ok(Self {
            reader: Arc::new(Mutex::new(reader)),
            state: Arc::new(HpsState {
                sample_rate: header.sample_rate,
                channels,
                loop_start,
                blocks,
            }),
            tag,
        })
    }

    /// Returns the audio sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.state.sample_rate
    }

    /// Returns the number of channels.
    pub fn channels(&self) -> usize {
        self.state.channels.len()
    }

    /// Returns the header for the channel at index `channel`.
    pub fn channel_header(&self, channel: usize) -> Channel {
        self.state.channels[channel]
    }

    /// Returns the index of the block where looping starts at.
    pub fn loop_start(&self) -> Option<usize> {
        self.state.loop_start
    }

    /// Returns the audio source tag.
    pub fn tag(&self) -> &SourceTag {
        &self.tag
    }

    /// Creates a `ChannelReader` over a channel in the stream.
    /// ***Panics*** if the channel index is out-of-bounds.
    pub fn channel_reader(&self, channel: usize) -> ChannelReader<'r> {
        assert!(channel < self.state.channels.len(), "invalid channel index");
        let tag = match (self.state.channels.len(), channel) {
            (2, 0) => self.tag.clone().for_channel(SourceChannel::Left),
            (2, 1) => self.tag.clone().for_channel(SourceChannel::Right),
            _ => self.tag.clone(),
        };
        ChannelReader {
            reader: Arc::clone(&self.reader),
            state: Arc::clone(&self.state),
            channel,
            pos: 0,
            tag,
        }
    }

    /// Creates a decoder which decodes the samples in `channel` into PCM16 format.
    /// ***Panics*** if the channel index is out-of-bounds.
    pub fn channel_decoder(&self, channel: usize) -> HpsDecoder<'r, 'static> {
        let reader = self.channel_reader(channel);
        match self.state.channels[channel].address.format {
            DspFormat::Adpcm => Box::new(adpcm::Decoder::new(reader.cast())),
            DspFormat::Pcm8 | DspFormat::Pcm16 => reader.convert(),
        }
    }

    /// Creates a decoder which decodes all channels into PCM16 format and joins them.
    pub fn decoder(&self) -> HpsDecoder<'r, 'static> {
        if self.state.channels.len() == 1 {
            self.channel_decoder(0)
        } else {
            let left = self.channel_decoder(0);
            let right = self.channel_decoder(1);
            Box::new(left.with_right_channel(right))
        }
    }

    /// Returns an iterator over the headers of the blocks in the stream.
    pub fn blocks(&self) -> impl Iterator<Item = &BlockHeader> {
        self.state.blocks.iter().map(|b| &b.header)
    }

    /// Returns an iterator over the cues in the stream.
    pub fn cues(&self) -> CueIterator {
        CueIterator::new(Arc::clone(&self.state))
    }
}

/// Reads sample data from a single program stream channel.
pub struct ChannelReader<'r> {
    reader: Arc<Mutex<Box<dyn ReadSeek + 'r>>>,
    state: Arc<HpsState>,
    channel: usize,
    pos: usize,
    tag: SourceTag,
}

impl ReadSamples<'static> for ChannelReader<'_> {
    type Format = AnyFormat;

    #[instrument(level = "trace", name = "ChannelReader", skip_all)]
    fn read_samples(&mut self) -> Result<Option<Samples<'static, Self::Format>>> {
        if self.pos >= self.state.blocks.len() {
            return Ok(None);
        }
        let block = &self.state.blocks[self.pos];
        let header = &block.header;
        self.pos += 1;

        let format = self.state.channels[self.channel].address.format;
        let data_size = Format::from(format).address_to_byte_up(header.end_address as usize + 1);
        let data_offset =
            block.data_offset + (align(data_size, super::DATA_ALIGN) * self.channel) as u64;
        let data = {
            let mut data = vec![0; data_size];
            let mut reader = self.reader.lock().unwrap();
            reader.seek(SeekFrom::Start(data_offset))?;
            reader.read_exact(&mut data)?;
            data
        };

        let rate = self.state.sample_rate;
        let len = header.end_address as usize + 1;
        let adpcm = &self.state.channels[self.channel].adpcm;
        match format {
            DspFormat::Adpcm => Ok(Some(
                Samples::<GcAdpcm> {
                    channels: 1,
                    rate,
                    len,
                    data: data.into(),
                    params: adpcm::Info {
                        coefficients: adpcm.coefficients,
                        gain: adpcm.gain,
                        context: header.channel_contexts[self.channel],
                    },
                }
                .cast(),
            )),

            // Chibi-Robo's engine doesn't actually play program streams with non-ADPCM samples
            // correctly, but the format *technically* should support it and there's even some code
            // referencing other formats...
            DspFormat::Pcm16 => {
                let samples = PcmS16Be::read_bytes(&data[..(len * 2)])?;
                Ok(Some(Samples::<PcmS16Be>::from_pcm(samples, 1, rate).cast()))
            }
            DspFormat::Pcm8 => {
                let samples = PcmS8::read_bytes(&data[..len])?;
                Ok(Some(Samples::<PcmS8>::from_pcm(samples, 1, rate).cast()))
            }
        }
    }

    fn format(&self) -> Format {
        self.state.channels[self.channel].address.format.into()
    }

    fn tag(&self) -> &SourceTag {
        &self.tag
    }

    fn progress(&self) -> Option<ProgressHint> {
        ProgressHint::new(self.pos as u64, self.state.blocks.len() as u64)
    }

    fn data_remaining(&self) -> Option<u64> {
        Some(self.state.blocks[self.pos..].iter().map(|b| b.header.end_address as u64 + 1).sum())
    }

    fn cues(&self) -> Box<dyn Iterator<Item = Cue> + '_> {
        Box::from(CueIterator::new(Arc::clone(&self.state)))
    }
}

/// An iterator over the cues in a program stream.
pub struct CueIterator {
    state: Arc<HpsState>,
    /// The index of the block to loop back to at the end of the stream.
    loop_block: Option<usize>,
    /// The index of the current block.
    block_index: usize,
    /// The index of the current cue within the current block.
    cue_index: usize,
    /// The number of samples before the current block.
    sample_base: u64,
}

impl CueIterator {
    fn new(state: Arc<HpsState>) -> Self {
        let loop_block = state.loop_start;
        Self { state, loop_block, block_index: 0, cue_index: 0, sample_base: 0 }
    }
}

impl Iterator for CueIterator {
    type Item = Cue;
    fn next(&mut self) -> Option<Self::Item> {
        let format = self.state.channels[0].address.format;
        loop {
            if self.block_index >= self.state.blocks.len() {
                return None;
            }
            if self.loop_block == Some(self.block_index) {
                self.loop_block = None;
                return Some(Cue::new_loop(LOOP_PREFIX, self.sample_base));
            }
            let block = &self.state.blocks[self.block_index].header;
            if self.cue_index < block.cues.len() {
                let cue = &block.cues[self.cue_index];
                self.cue_index += 1;
                let name = format!("{}", cue.id);
                let start = self.sample_base + (cue.sample_index as u64);
                return Some(Cue::new(name, start));
            } else {
                self.block_index += 1;
                self.cue_index = 0;
                self.sample_base += super::num_samples(block.end_address, format);
            }
        }
    }
}
