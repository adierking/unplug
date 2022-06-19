use super::{BlockHeader, Channel, CuePoint, FileHeader};
use crate::audio::format::adpcm::{self, EncoderBuilder, GcAdpcm};
use crate::audio::format::dsp::{AudioAddress, DspFormat};
use crate::audio::format::{PcmS16Le, StaticFormat};
use crate::audio::{Cue, Error, ProgressHint, ReadSamples, Result, Samples};
use crate::common::io::pad;
use crate::common::{align, WriteSeek, WriteTo};
use arrayvec::ArrayVec;
use std::io::SeekFrom;
use std::num::NonZeroU32;
use tracing::{instrument, warn};

type ProgressCallback<'a> = Box<dyn FnMut(Option<ProgressHint>) + 'a>;

/// A channel within a program stream block.
struct BlockChannel {
    /// Initial playback parameters.
    initial_context: adpcm::FrameContext,
    /// The channel's audio data.
    data: Vec<u8>,
}

impl BlockChannel {
    /// Creates a `BlockChannel` with sample data from `samples`.
    fn from_samples(samples: Samples<'_, GcAdpcm>) -> Result<Self> {
        Ok(Self { initial_context: samples.params.context, data: samples.data.into_owned() })
    }
}

/// A block within a program stream.
struct Block {
    /// The end address of the audio data in the block.
    end_address: u32,
    /// The data for each channel in the block.
    channels: ArrayVec<BlockChannel, 2>,
    /// The cue points in the block.
    cues: Vec<CuePoint>,
}

impl Block {
    /// Creates a `Block` from mono ADPCM sample data.
    fn from_mono(samples: Samples<'_, GcAdpcm>) -> Result<Self> {
        if samples.channels != 1 {
            return Err(Error::StreamNotMono);
        }
        let size = GcAdpcm::address_to_byte_up(samples.len);
        if size > super::MONO_BLOCK_SIZE {
            return Err(Error::BlockTooLarge(size, super::MONO_BLOCK_SIZE));
        }
        let mut channels = ArrayVec::new();
        let end_address = (samples.len - 1) as u32;
        channels.push(BlockChannel::from_samples(samples)?);
        Ok(Self { end_address, channels, cues: vec![] })
    }

    /// Creates a `Block` from stereo ADPCM sample data.
    fn from_stereo(left: Samples<'_, GcAdpcm>, right: Samples<'_, GcAdpcm>) -> Result<Self> {
        if left.channels != 1 || right.channels != 1 {
            return Err(Error::StreamNotMono);
        }
        if left.rate != right.rate {
            return Err(Error::InconsistentSampleRate);
        }
        if left.len != right.len {
            return Err(Error::DifferentChannelSizes);
        }
        let size = GcAdpcm::address_to_byte_up(left.len);
        if size > super::STEREO_BLOCK_SIZE {
            return Err(Error::BlockTooLarge(size, super::STEREO_BLOCK_SIZE));
        }
        let mut channels = ArrayVec::new();
        let end_address = (left.len - 1) as u32;
        channels.push(BlockChannel::from_samples(left)?);
        channels.push(BlockChannel::from_samples(right)?);
        Ok(Self { end_address, channels, cues: vec![] })
    }

    /// Writes the block header and data to `writer`. If `next_offset` is not `None`, it will be
    /// used as the block's `next_offset` instead of the offset after this block. Returns the total
    /// size of the written data in bytes.
    #[instrument(level = "trace", skip_all)]
    fn write_to(
        &self,
        writer: &mut dyn WriteSeek,
        offset: u32,
        next_offset: Option<u32>,
    ) -> Result<u32> {
        if !(1..=2).contains(&self.channels.len()) {
            return Err(Error::UnsupportedChannels);
        }
        let mut channel_contexts = [adpcm::FrameContext::default(); 2];
        let mut data_size = 0;
        for (i, channel) in self.channels.iter().enumerate() {
            channel_contexts[i] = channel.initial_context;
            data_size += align(channel.data.len(), super::DATA_ALIGN) as u32;
        }

        let mut header = BlockHeader {
            size: data_size,
            end_address: self.end_address,
            next_offset: 0,
            channel_contexts,
            cues: self.cues.clone(),
        };
        let total_size = (header.file_size() as u32) + data_size;
        header.next_offset = next_offset.unwrap_or(offset + total_size);
        header.write_to(writer)?;

        for channel in &self.channels {
            writer.write_all(&channel.data)?;
            pad(&mut *writer, super::DATA_ALIGN as u64, 0)?;
        }
        Ok(total_size)
    }
}

/// Builds a program stream by reading and encoding PCM sample data. The encoding actually takes
/// place in two steps: a "preparation" step where the audio samples are gathered and analyzed, and
/// then the actual writing step which is driven by the `HpsWriter` that `prepare()` returns.
pub struct PcmHpsWriter<'r, 's> {
    reader: Box<dyn ReadSamples<'s, Format = PcmS16Le> + 'r>,
    on_progress: Option<ProgressCallback<'r>>,
}

impl<'r, 's> PcmHpsWriter<'r, 's> {
    /// Creates a new `PcmHpsBuilder` which reads samples from `reader`.
    pub fn new(reader: impl ReadSamples<'s, Format = PcmS16Le> + 'r) -> Self {
        Self { reader: Box::from(reader), on_progress: None }
    }

    /// Sets a callback to run for progress updates. If the total amount of work is unknown, the
    /// callback will still be invoked with a `None` hint. This callback is only for the preparation
    /// step and a new callback must be set on the builder returned by `prepare()` to monitor
    /// encoding progress.
    #[must_use]
    pub fn on_progress(mut self, callback: impl FnMut(Option<ProgressHint>) + 'r) -> Self {
        self.on_progress = Some(Box::from(callback));
        self
    }

    /// Gathers samples and analyzes the waveform to prepare for encoding. The actual encoding is
    /// done separately with the returned `HpsWriter`.
    pub fn prepare(self) -> Result<HpsWriter<'s, 's>> {
        let mut builder = EncoderBuilder::new(self.reader)
            .mono_block_size(super::MONO_BLOCK_SIZE)
            .stereo_block_size(super::STEREO_BLOCK_SIZE);
        if let Some(on_progress) = self.on_progress {
            builder = builder.on_progress(on_progress);
        }
        match builder.build()? {
            (left, Some(right)) => Ok(HpsWriter::with_stereo(left, right)),
            (mono, None) => Ok(HpsWriter::with_mono(mono)),
        }
    }
}

type AdpcmReader<'r, 's> = Box<dyn ReadSamples<'s, Format = GcAdpcm> + 'r>;

/// Audio looping strategies.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Looping {
    /// Always disable looping, even if the source audio has a loop point.
    Disabled,
    /// Always enable looping. If the source audio does not have a loop point, one will be set at
    /// the beginning.
    Enabled,
    /// Use the loop settings from the source audio (default).
    Auto,
}

/// Writes out a program stream from ADPCM sample data. Use `PcmHpsBuilder` to create a writer for
/// PCM samples.
pub struct HpsWriter<'r, 's> {
    left: AdpcmReader<'r, 's>,
    right: Option<AdpcmReader<'r, 's>>,
    on_progress: Option<ProgressCallback<'r>>,
    looping: Looping,
    left_channel: Channel,
    right_channel: Channel,
    offset: u32,
    sample: u64,
    loop_offset: u32,
    cues: Vec<Cue>,
    next_cue_index: usize,
    next_cue_id: u32,
    rate: Option<NonZeroU32>,
}

impl<'r, 's> HpsWriter<'r, 's> {
    /// Creates a new `HpsWriter` which reads mono samples from `reader`.
    pub fn with_mono(reader: impl ReadSamples<'s, Format = GcAdpcm> + 'r) -> Self {
        Self::new_impl(Box::from(reader), None)
    }

    /// Creates a new `HpsWriter` which reads stereo samples from `left` and `right`.
    pub fn with_stereo(
        left: impl ReadSamples<'s, Format = GcAdpcm> + 'r,
        right: impl ReadSamples<'s, Format = GcAdpcm> + 'r,
    ) -> Self {
        Self::new_impl(Box::from(left), Some(Box::from(right)))
    }

    fn new_impl(left: AdpcmReader<'r, 's>, right: Option<AdpcmReader<'r, 's>>) -> Self {
        Self {
            left,
            right,
            on_progress: None,
            looping: Looping::Auto,
            left_channel: Channel::default(),
            right_channel: Channel::default(),
            offset: super::FIRST_BLOCK_OFFSET,
            sample: 0,
            loop_offset: super::END_BLOCK_OFFSET,
            cues: vec![],
            next_cue_index: 0,
            next_cue_id: 1,
            rate: None,
        }
    }

    /// Sets a callback to run for progress updates. If the total amount of work is unknown, the
    /// callback will still be invoked with a `None` hint.
    #[must_use]
    pub fn on_progress(mut self, callback: impl FnMut(Option<ProgressHint>) + 'r) -> Self {
        self.on_progress = Some(Box::from(callback));
        self
    }

    /// Sets the audio's looping strategy. Defaults to `Auto`.
    #[must_use]
    pub fn looping(mut self, looping: Looping) -> Self {
        self.looping = looping;
        self
    }

    /// Finishes building the program stream and writes it out to `writer`.
    pub fn write_to(self, mut writer: impl WriteSeek) -> Result<()> {
        self.write_to_impl(&mut writer)
    }

    /// Reads all samples and builds the final program stream.
    fn write_to_impl(mut self, writer: &mut dyn WriteSeek) -> Result<()> {
        self.update_progress();
        self.collect_cues();

        // Write a placeholder header because we can't know how to fill it out until the blocks are
        // all processed
        let start_offset = writer.seek(SeekFrom::Current(0))?;
        FileHeader::new().write_to(writer)?;

        pad(&mut *writer, super::FIRST_BLOCK_OFFSET as u64, 0)?;
        self.write_blocks(writer)?;

        // Now go back and fill in the header
        let end_offset = writer.seek(SeekFrom::Current(0))?;
        writer.seek(SeekFrom::Start(start_offset))?;
        let channels = [self.left_channel, self.right_channel];
        let header = FileHeader {
            sample_rate: self.rate.unwrap().get(),
            num_channels: if self.right.is_some() { 2 } else { 1 },
            channels,
            ..Default::default()
        };
        header.write_to(writer)?;
        writer.seek(SeekFrom::Start(end_offset))?;
        self.update_progress();
        Ok(())
    }

    /// Processes each block in the stream and writes them out to `writer`.
    fn write_blocks(&mut self, writer: &mut dyn WriteSeek) -> Result<()> {
        let mut last_block: Option<Block> = None;
        loop {
            let mut right = self.right.take();
            let block = match &mut right {
                Some(right) => self.next_block_stereo(right)?,
                None => self.next_block_mono()?,
            };
            self.right = right;
            let block = match block {
                Some(block) => block,
                None => break,
            };

            // Blocks aren't written out until we've read the next block of samples, because
            // otherwise we don't know when the end of the stream is. The last block has to be
            // written specially in order to set the next offset correctly based on the looping
            // settings.
            if let Some(mut last) = last_block {
                self.write_block(&mut last, writer, false)?;
            }

            last_block = Some(block);
            self.update_progress();
        }
        match last_block {
            Some(mut last) => self.write_block(&mut last, writer, true)?,
            None => return Err(Error::EmptyStream),
        }
        Ok(())
    }

    fn write_block(
        &mut self,
        block: &mut Block,
        writer: &mut dyn WriteSeek,
        is_last: bool,
    ) -> Result<()> {
        let end_sample = if is_last {
            u64::MAX // Write all remaining cues
        } else {
            let num_samples = super::num_samples(block.end_address, DspFormat::Adpcm);
            self.sample + num_samples
        };
        self.assign_cues(block, self.offset, self.sample, end_sample);
        let next_offset = if is_last { Some(self.loop_offset) } else { None };
        self.offset += block.write_to(writer, self.offset, next_offset)?;
        self.sample += super::num_samples(block.end_address, DspFormat::Adpcm);
        Ok(())
    }

    /// Processes the next block of sample data to write when there is only one channel.
    fn next_block_mono(&mut self) -> Result<Option<Block>> {
        let samples = match self.left.read_samples()? {
            Some(samples) => samples,
            None => return Ok(None),
        };

        if let Some(rate) = self.rate {
            if samples.rate != rate.get() {
                return Err(Error::InconsistentSampleRate);
            }
        } else if samples.rate > 0 {
            self.rate = NonZeroU32::new(samples.rate);
        } else {
            return Err(Error::InvalidSampleRate(samples.rate));
        }

        let adpcm = samples.params;
        let block = Block::from_mono(samples)?;
        Self::expand_channel(&mut self.left_channel, &block, &adpcm)?;
        Ok(Some(block))
    }

    /// Processes the next block of sample data to write when there are two channels.
    fn next_block_stereo(&mut self, right: &mut AdpcmReader<'r, 's>) -> Result<Option<Block>> {
        let (left, right) = match (self.left.read_samples()?, right.read_samples()?) {
            (Some(l), Some(r)) => (l, r),
            (None, None) => return Ok(None),
            _ => return Err(Error::DifferentChannelSizes),
        };

        if left.rate != right.rate {
            return Err(Error::InconsistentSampleRate);
        } else if let Some(rate) = self.rate {
            if left.rate != rate.get() {
                return Err(Error::InconsistentSampleRate);
            }
        } else if left.rate > 0 {
            self.rate = NonZeroU32::new(left.rate);
        } else {
            return Err(Error::InvalidSampleRate(left.rate));
        }

        let left_adpcm = left.params;
        let right_adpcm = right.params;
        let block = Block::from_stereo(left, right)?;
        Self::expand_channel(&mut self.left_channel, &block, &left_adpcm)?;
        Self::expand_channel(&mut self.right_channel, &block, &right_adpcm)?;
        Ok(Some(block))
    }

    /// Updates `channel` to include `block`.
    fn expand_channel(channel: &mut Channel, block: &Block, state: &adpcm::Info) -> Result<()> {
        if channel.is_initialized() {
            if state.coefficients != channel.adpcm.coefficients {
                return Err(Error::DifferentCoefficients);
            }
            channel.address.end_address += block.end_address + 1;
        } else {
            *channel = Channel {
                address: AudioAddress {
                    looping: true,
                    format: DspFormat::Adpcm,
                    loop_address: 0x2,
                    end_address: block.end_address,
                    current_address: 0x2,
                },
                adpcm: *state,
            };
        }
        Ok(())
    }

    /// Merges cue points from the inner readers and populates the `self.cues` list.
    fn collect_cues(&mut self) {
        self.cues = match &self.right {
            Some(right) => self.left.cues().chain(right.cues()).collect(),
            None => self.left.cues().collect(),
        };
        self.cues.sort_unstable();
        self.cues.dedup();
        if self.looping == Looping::Enabled && !self.cues.iter().any(|c| c.is_loop()) {
            warn!("Setting loop point at the start because none was defined");
            self.cues.insert(0, Cue::new_loop("loop", 0));
        }
    }

    /// Assigns the cue points from `start_sample` up until `end_sample` to `block`. `offset` is the
    /// file offset of the block used for loop point handling.
    fn assign_cues(&mut self, block: &mut Block, offset: u32, start_sample: u64, end_sample: u64) {
        while self.next_cue_index < self.cues.len() {
            let cue = &self.cues[self.next_cue_index];
            if cue.start >= end_sample {
                break;
            }
            if cue.is_loop() {
                if self.looping == Looping::Disabled {
                    warn!("Discarding loop point \"{}\" because looping is disabled", cue.name);
                } else if self.loop_offset == super::END_BLOCK_OFFSET {
                    if cue.start != start_sample {
                        warn!(
                            "Loop point \"{}\" is not block-aligned - rounding down to sample {}",
                            cue.name, start_sample
                        );
                    }
                    self.loop_offset = offset;
                } else {
                    warn!("Discarding extra loop point \"{}\"", cue.name);
                }
            } else {
                let start = cue.start - start_sample;
                let point = CuePoint { sample_index: start as i32, id: self.next_cue_id };
                block.cues.push(point);
                self.next_cue_id += 1;
            }
            self.next_cue_index += 1;
        }
    }

    fn update_progress(&mut self) {
        if let Some(on_progress) = &mut self.on_progress {
            on_progress(self.left.progress())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::sample::{ReadSampleList, ReadSamples, Samples};
    use crate::audio::transport::hps::HpsReader;
    use crate::audio::Cue;
    use crate::test;
    use std::io::{Cursor, Seek};

    fn write_and_read_hps<'r, 's>(writer: HpsWriter<'r, 's>) -> Result<HpsReader<'static>> {
        let mut cursor = Cursor::new(vec![]);
        writer.write_to(&mut cursor)?;
        cursor.seek(SeekFrom::Start(0))?;
        HpsReader::new(cursor, "test")
    }

    #[test]
    fn test_hps_from_pcm_mono() -> Result<()> {
        let data = test::open_test_wav();
        let samples = Samples::<PcmS16Le>::from_pcm(data, 2, 44100);
        let cues = vec![Cue::new_loop("loop", 0), Cue::new("1", 88200)];
        let reader = ReadSampleList::with_cues(vec![samples], cues.clone(), "test");
        let splitter = reader.split_channels();

        let hps = write_and_read_hps(PcmHpsWriter::new(splitter.left()).prepare()?)?;
        assert_eq!(hps.sample_rate(), 44100);
        assert_eq!(hps.channels(), 1);
        assert_eq!(hps.cues().collect::<Vec<_>>(), cues);

        let left = hps.channel_header(0);
        assert_eq!(left.address.end_address, 0x30af8);
        assert_eq!(left.adpcm.coefficients, test::TEST_WAV_LEFT_COEFFICIENTS);
        assert_eq!(left.adpcm.context.predictor_and_scale, test::TEST_WAV_LEFT_DSP[0] as u16);

        const EXPECTED_END_ADDRESSES: &[u32] = &[0x1ffff, 0x10af8];
        const EXPECTED_LAST_SAMPLES: &[[i16; 2]] = &[[0, 0], [1236, 1218]];

        let blocks = hps.blocks().collect::<Vec<_>>();
        assert_eq!(blocks.len(), 2);
        for (i, block) in blocks.into_iter().enumerate() {
            assert_eq!(block.end_address, EXPECTED_END_ADDRESSES[i]);
            let left = &block.channel_contexts[0];
            assert_eq!(left.last_samples, EXPECTED_LAST_SAMPLES[i]);
        }

        let decoded = hps.channel_reader(0).cast::<GcAdpcm>().read_all_samples()?;
        assert_eq!(decoded.data, test::TEST_WAV_LEFT_DSP);
        Ok(())
    }

    #[test]
    fn test_hps_from_pcm_stereo() -> Result<()> {
        let data = test::open_test_wav();
        let samples = Samples::<PcmS16Le>::from_pcm(data, 2, 44100);
        let cues = vec![Cue::new_loop("loop", 0), Cue::new("1", 88200)];
        let reader = ReadSampleList::with_cues(vec![samples], cues.clone(), "test");

        let hps = write_and_read_hps(PcmHpsWriter::new(reader).prepare()?)?;
        assert_eq!(hps.sample_rate(), 44100);
        assert_eq!(hps.channels(), 2);
        assert_eq!(hps.cues().collect::<Vec<_>>(), cues);

        let left = hps.channel_header(0);
        assert_eq!(left.address.end_address, 0x30af8);
        assert_eq!(left.adpcm.coefficients, test::TEST_WAV_LEFT_COEFFICIENTS);
        assert_eq!(left.adpcm.context.predictor_and_scale, test::TEST_WAV_LEFT_DSP[0] as u16);

        let right = hps.channel_header(1);
        assert_eq!(right.address.end_address, 0x30af8);
        assert_eq!(right.adpcm.coefficients, test::TEST_WAV_RIGHT_COEFFICIENTS);
        assert_eq!(right.adpcm.context.predictor_and_scale, test::TEST_WAV_RIGHT_DSP[0] as u16);

        const EXPECTED_END_ADDRESSES: &[u32] = &[0xffff, 0xffff, 0xffff, 0xaf8];
        const EXPECTED_LAST_SAMPLES_LEFT: &[[i16; 2]] =
            &[[0, 0], [-5232, -5240], [1236, 1218], [33, 42]];
        const EXPECTED_LAST_SAMPLES_RIGHT: &[[i16; 2]] =
            &[[0, 0], [730, 618], [1751, 1697], [-9, -3]];

        let blocks = hps.blocks().collect::<Vec<_>>();
        assert_eq!(blocks.len(), 4);
        for (i, block) in blocks.into_iter().enumerate() {
            assert_eq!(block.end_address, EXPECTED_END_ADDRESSES[i]);
            let left = &block.channel_contexts[0];
            assert_eq!(left.last_samples, EXPECTED_LAST_SAMPLES_LEFT[i]);
            let right = &block.channel_contexts[1];
            assert_eq!(right.last_samples, EXPECTED_LAST_SAMPLES_RIGHT[i]);
        }

        let left = hps.channel_reader(0).cast::<GcAdpcm>().read_all_samples()?;
        let right = hps.channel_reader(1).cast::<GcAdpcm>().read_all_samples()?;
        assert_eq!(left.data, test::TEST_WAV_LEFT_DSP);
        assert_eq!(right.data, test::TEST_WAV_RIGHT_DSP);
        Ok(())
    }

    #[test]
    fn test_assign_cues() -> Result<()> {
        let mut blocks = vec![];
        for _ in 0..4 {
            blocks.push(Samples::<GcAdpcm> {
                channels: 1,
                rate: 44100,
                len: 0x10,
                data: vec![0; 8].into(),
                params: adpcm::Info::default(),
            });
        }
        let cues = vec![Cue::new("b", 28), Cue::new("a", 0), Cue::new("c", 29), Cue::new("d", 56)];
        let reader = ReadSampleList::with_cues(blocks, cues, "test");

        let hps = write_and_read_hps(HpsWriter::with_mono(reader))?;
        let blocks = hps.blocks().collect::<Vec<_>>();
        assert_eq!(blocks.len(), 4);
        assert_eq!(blocks[0].cues, &[CuePoint { sample_index: 0, id: 1 }]);
        assert_eq!(
            blocks[2].cues,
            &[CuePoint { sample_index: 0, id: 2 }, CuePoint { sample_index: 1, id: 3 }]
        );
        assert_eq!(blocks[3].cues, &[CuePoint { sample_index: 14, id: 4 }]);
        Ok(())
    }

    #[test]
    fn test_looping() -> Result<()> {
        fn make_reader(cues: Vec<Cue>) -> ReadSampleList<'static, GcAdpcm> {
            let mut blocks = vec![];
            for _ in 0..4 {
                blocks.push(Samples::<GcAdpcm> {
                    channels: 1,
                    rate: 44100,
                    len: 0x10,
                    data: vec![0; 8].into(),
                    params: adpcm::Info::default(),
                });
            }
            ReadSampleList::with_cues(blocks, cues, "test")
        }

        let cues = vec![Cue::new_loop("loop", 14)];
        let hps = write_and_read_hps(HpsWriter::with_mono(make_reader(cues)))?;
        assert_eq!(hps.loop_start(), Some(1));

        let cues = vec![Cue::new_loop("loop", 41)];
        let hps = write_and_read_hps(HpsWriter::with_mono(make_reader(cues)))?;
        assert_eq!(hps.loop_start(), Some(2));

        let cues = vec![Cue::new_loop("loop", 56)];
        let hps = write_and_read_hps(HpsWriter::with_mono(make_reader(cues)))?;
        assert_eq!(hps.loop_start(), Some(3));

        let cues = vec![Cue::new_loop("loop", 0)];
        let hps =
            write_and_read_hps(HpsWriter::with_mono(make_reader(cues)).looping(Looping::Disabled))?;
        assert_eq!(hps.loop_start(), None);

        let cues = vec![];
        let hps =
            write_and_read_hps(HpsWriter::with_mono(make_reader(cues)).looping(Looping::Enabled))?;
        assert_eq!(hps.loop_start(), Some(0));
        Ok(())
    }
}
