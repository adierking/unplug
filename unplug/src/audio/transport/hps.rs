use crate::audio::format::adpcm::{self, EncoderBuilder, GcAdpcm};
use crate::audio::format::dsp::{AudioAddress, DspFormat};
use crate::audio::format::{
    AnyFormat, Format, PcmS16Be, PcmS16Le, PcmS8, ReadWriteBytes, StaticFormat,
};
use crate::audio::{
    self, Error, ProgressHint, ReadSamples, Result, Samples, SourceChannel, SourceTag,
};
use crate::common::io::pad;
use crate::common::{align, ReadFrom, ReadSeek, Region, WriteTo};
use arrayvec::ArrayVec;
use byteorder::{ReadBytesExt, WriteBytesExt, BE};
use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt::{self, Debug};
use std::io::{self, Read, Seek, SeekFrom, Write};
use tracing::{debug, instrument, trace};
use unplug_proc::{ReadFrom, WriteTo};

/// The magic string at the beginning of an HPS file.
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

/// Convenience type for an opaque decoder.
type HpsDecoder<'r, 's> = Box<dyn ReadSamples<'s, Format = PcmS16Le> + 'r>;

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

/// Size of a block header in bytes.
const BLOCK_HEADER_SIZE: usize = 0x20;

/// Audio block header.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct BlockHeader {
    /// Total size (in bytes) of the audio data in the block.
    size: u32,
    /// The end address of the audio data.
    end_address: u32,
    /// The offset of the next block to play.
    next_offset: u32,
    /// Initial playback parameters for each channel.
    channel_contexts: [adpcm::FrameContext; 2],
    /// Cue points in this block.
    cues: Vec<CuePoint>,
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

type ProgressCallback<'a> = Box<dyn FnMut(Option<ProgressHint>) + 'a>;

/// Builds an HPS stream by reading and encoding PCM sample data. The encoding actually takes place
/// in two steps: a "preparation" step where the audio samples are gathered and analyzed, and then
/// the actual encoding step which is driven by the `AdpcmHpsBuilder` that `prepare()` returns.
pub struct PcmHpsBuilder<'r, 's> {
    reader: Box<dyn ReadSamples<'s, Format = PcmS16Le> + 'r>,
    on_progress: Option<ProgressCallback<'r>>,
}

impl<'r, 's> PcmHpsBuilder<'r, 's> {
    /// Creates a new `PcmHpsBuilder` which reads samples from `reader`.
    pub fn new(reader: impl ReadSamples<'s, Format = PcmS16Le> + 'r) -> Self {
        Self { reader: Box::from(reader), on_progress: None }
    }

    /// Sets a callback to run for progress updates. If the total amount of work is unknown, the
    /// callback will still be invoked with a `None` hint. This callback is only for the preparation
    /// step and a new callback must be set on the builder returned by `prepare()` to monitor
    /// encoding progress.
    pub fn on_progress(mut self, callback: impl FnMut(Option<ProgressHint>) + 'r) -> Self {
        self.on_progress = Some(Box::from(callback));
        self
    }

    /// Gathers samples and analyzes the waveform to prepare for encoding. The actual encoding is
    /// done separately with the returned `AdpcmHpsBuilder`.
    pub fn prepare(self) -> Result<AdpcmHpsBuilder<'s, 's>> {
        let mut builder = EncoderBuilder::new(self.reader)
            .mono_block_size(MONO_BLOCK_SIZE)
            .stereo_block_size(STEREO_BLOCK_SIZE);
        if let Some(on_progress) = self.on_progress {
            builder = builder.on_progress(on_progress);
        }
        match builder.build()? {
            (left, Some(right)) => Ok(AdpcmHpsBuilder::with_stereo(left, right)),
            (mono, None) => Ok(AdpcmHpsBuilder::with_mono(mono)),
        }
    }
}

type AdpcmReader<'r, 's> = Box<dyn ReadSamples<'s, Format = GcAdpcm> + 'r>;

/// Builds an HPS stream by reading ADPCM sample data.
pub struct AdpcmHpsBuilder<'r, 's> {
    left: AdpcmReader<'r, 's>,
    right: Option<AdpcmReader<'r, 's>>,
    on_progress: Option<ProgressCallback<'r>>,
    looping: bool,
    left_channel: Channel,
    right_channel: Channel,
    blocks: Vec<Block>,
    cues: Vec<audio::Cue>,
    rate: u32,
}

impl<'r, 's> AdpcmHpsBuilder<'r, 's> {
    /// Creates a new `AdpcmHpsBuilder` which reads mono samples from `reader`.
    pub fn with_mono(reader: impl ReadSamples<'s, Format = GcAdpcm> + 'r) -> Self {
        Self::new_impl(Box::from(reader), None)
    }

    /// Creates a new `AdpcmHpsBuilder` which reads stereo samples from `left` and `right`.
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
            looping: true,
            left_channel: Channel::default(),
            right_channel: Channel::default(),
            blocks: vec![],
            cues: vec![],
            rate: 0,
        }
    }

    /// Sets a callback to run for progress updates. If the total amount of work is unknown, the
    /// callback will still be invoked with a `None` hint.
    pub fn on_progress(mut self, callback: impl FnMut(Option<ProgressHint>) + 'r) -> Self {
        self.on_progress = Some(Box::from(callback));
        self
    }

    /// Sets whether the HPS should loop (enabled by default).
    pub fn looping(mut self, enabled: bool) -> Self {
        self.looping = enabled;
        self
    }

    /// Reads all samples and builds the final HPS stream.
    pub fn build(mut self) -> Result<HpsStream> {
        self.update_progress();
        self.collect_cues();
        match self.right.take() {
            Some(right) => self.build_stereo(right),
            None => self.build_mono(),
        }
    }

    #[instrument(level = "trace", skip_all)]
    fn build_mono(mut self) -> Result<HpsStream> {
        while let Some(samples) = self.left.read_samples()? {
            if samples.len == 0 {
                continue;
            }

            if self.blocks.is_empty() {
                self.rate = samples.rate;
            } else if samples.rate != self.rate {
                return Err(Error::InconsistentSampleRate);
            }

            let adpcm = samples.params;
            let block = Block::from_mono(samples)?;
            Self::expand_channel(&mut self.left_channel, &block, &adpcm)?;
            self.blocks.push(block);
            self.update_progress();
        }
        let tag = self.left.tag().clone();
        self.finish(tag)
    }

    #[instrument(level = "trace", skip_all)]
    fn build_stereo(mut self, mut right: AdpcmReader<'r, 's>) -> Result<HpsStream> {
        loop {
            let (left, right) = match (self.left.read_samples()?, right.read_samples()?) {
                (Some(l), Some(r)) => (l, r),
                (None, None) => break,
                _ => return Err(Error::DifferentChannelSizes),
            };

            if left.len != right.len {
                return Err(Error::DifferentChannelSizes);
            } else if left.len == 0 {
                continue;
            }

            if self.blocks.is_empty() {
                self.rate = left.rate;
            }
            if left.rate != self.rate || right.rate != self.rate {
                return Err(Error::InconsistentSampleRate);
            }

            let left_adpcm = left.params;
            let right_adpcm = right.params;
            let block = Block::from_stereo(left, right)?;
            Self::expand_channel(&mut self.left_channel, &block, &left_adpcm)?;
            Self::expand_channel(&mut self.right_channel, &block, &right_adpcm)?;
            self.blocks.push(block);
            self.update_progress();
        }
        let tag = self.left.tag().join(right.tag());
        self.finish(tag)
    }

    fn expand_channel(channel: &mut Channel, block: &Block, state: &adpcm::Info) -> Result<()> {
        if channel.is_initialized() {
            if state.coefficients != channel.adpcm.coefficients {
                return Err(Error::DifferentCoefficients);
            }
            channel.address.end_address += block.end_address + 1;
        } else {
            *channel = Channel {
                address: AudioAddress {
                    looping: true, // TODO
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
    }

    /// Assigns the cue points from `self.cues` to their corresponding blocks.
    fn assign_cues(&mut self) {
        let mut cue_index = 0;
        let mut block_index = 0;
        let mut sample_base = 0;
        let num_cues = self.cues.len();
        let num_blocks = self.blocks.len();
        while cue_index < num_cues && block_index < num_blocks {
            let block = &mut self.blocks[block_index];
            let num_samples = block.num_samples(DspFormat::Adpcm);
            let end_sample = sample_base + (num_samples as u64);
            while cue_index < num_cues {
                let cue = &self.cues[cue_index];
                if cue.start >= end_sample && block_index < num_blocks - 1 {
                    break;
                }
                let start = cue.start - sample_base;
                let point = CuePoint { sample_index: start as i32, id: (cue_index + 1) as u32 };
                block.cues.push(point);
                cue_index += 1;
            }
            block_index += 1;
            sample_base = end_sample;
        }
    }

    fn finish(mut self, tag: SourceTag) -> Result<HpsStream> {
        if self.blocks.is_empty() {
            return Err(Error::EmptyStream);
        }
        if self.rate == 0 {
            return Err(Error::InvalidSampleRate(self.rate));
        }
        self.assign_cues();
        let mut channels = ArrayVec::new();
        channels.push(self.left_channel);
        if self.right_channel.is_initialized() {
            channels.push(self.right_channel);
        }
        let loop_start = if self.looping { Some(0) } else { None }; // TODO
        Ok(HpsStream { sample_rate: self.rate, channels, loop_start, blocks: self.blocks, tag })
    }

    fn update_progress(&mut self) {
        if let Some(on_progress) = &mut self.on_progress {
            on_progress(self.left.progress())
        }
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
    /// The audio source tag for debugging purposes.
    tag: SourceTag,
}

impl HpsStream {
    /// Opens an HPS stream read from `reader`. `tag` is a string or tag to identify the stream for
    /// debugging purposes.
    pub fn open(reader: &mut dyn ReadSeek, tag: impl Into<SourceTag>) -> Result<Self> {
        Self::open_impl(reader, tag.into())
    }

    #[instrument(level = "trace", skip_all)]
    fn open_impl(reader: &mut dyn ReadSeek, tag: SourceTag) -> Result<Self> {
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
            "Loaded HPS stream {:?}: {} Hz, {}, {} blocks",
            tag,
            header.sample_rate,
            if channels.len() == 2 { "stereo" } else { "mono" },
            blocks.len(),
        );
        Ok(Self { sample_rate: header.sample_rate, channels, loop_start, blocks, tag })
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
            blocks: &self.blocks,
            pos: 0,
            channel,
            format: self.channels[channel].address.format,
            sample_rate: self.sample_rate,
            adpcm: &self.channels[channel].adpcm,
            tag,
        }
    }

    /// Creates a decoder which decodes all channels into PCM16 format and joins them.
    pub fn decoder(&self) -> HpsDecoder<'_, '_> {
        if self.channels.len() == 1 {
            self.channel_decoder(0)
        } else {
            let left = self.channel_decoder(0);
            let right = self.channel_decoder(1);
            Box::new(left.with_right_channel(right))
        }
    }

    /// Creates a decoder which decodes the samples in `channel` into PCM16 format.
    /// ***Panics*** if the channel index is out-of-bounds.
    pub fn channel_decoder(&self, channel: usize) -> HpsDecoder<'_, '_> {
        let reader = self.reader(channel);
        match self.channels[channel].address.format {
            DspFormat::Adpcm => Box::new(adpcm::Decoder::new(reader.cast())),
            DspFormat::Pcm8 | DspFormat::Pcm16 => reader.convert(),
        }
    }

    /// Returns an iterator over the cues in the stream.
    pub fn cues(&self) -> CueIterator<'_> {
        CueIterator::new(&self.blocks, self.channels[0].address.format)
    }

    /// Creates a new `HpsStream` by encoding mono/stereo PCMS16LE sample data to ADPCM format. For
    /// more control over the stream creation, use `PcmHpsBuilder` instead.
    #[instrument(level = "trace", skip_all)]
    pub fn from_pcm(reader: &mut dyn ReadSamples<'_, Format = PcmS16Le>) -> Result<Self> {
        PcmHpsBuilder::new(reader).prepare()?.build()
    }

    /// Creates a new `HpsStream` from mono ADPCM sample data. For more control over the stream
    /// creation, use `AdpcmHpsBuilder` instead.
    #[instrument(level = "trace", skip_all)]
    pub fn from_adpcm_mono<'r, 's>(
        reader: &'r mut dyn ReadSamples<'s, Format = GcAdpcm>,
    ) -> Result<Self> {
        AdpcmHpsBuilder::with_mono(reader).build()
    }

    /// Creates a new `HpsStream` from stereo ADPCM sample data. For more control over the stream
    /// creation, use `AdpcmHpsBuilder` instead.
    #[instrument(level = "trace", skip_all)]
    pub fn from_adpcm_stereo<'r, 's>(
        left: &'r mut dyn ReadSamples<'s, Format = GcAdpcm>,
        right: &'r mut dyn ReadSamples<'s, Format = GcAdpcm>,
    ) -> Result<Self> {
        AdpcmHpsBuilder::with_stereo(left, right).build()
    }
}

impl<W: Write + Seek + ?Sized> WriteTo<W> for HpsStream {
    type Error = Error;

    #[instrument(level = "trace", skip_all)]
    fn write_to(&self, writer: &mut W) -> Result<()> {
        if !(1..=2).contains(&self.channels.len()) {
            return Err(Error::UnsupportedChannels);
        }

        let mut channels = self.channels.to_vec();
        channels.resize_with(2, Default::default);
        let header = FileHeader {
            magic: HPS_MAGIC,
            sample_rate: self.sample_rate,
            num_channels: self.channels.len() as u32,
            channels: channels.try_into().unwrap(),
        };
        header.write_to(writer)?;
        pad(&mut *writer, FIRST_BLOCK_OFFSET as u64, 0)?;

        let mut loop_offset = u32::MAX;
        for (i, block) in self.blocks.iter().enumerate() {
            if self.loop_start == Some(i) {
                loop_offset = writer.seek(SeekFrom::Current(0))? as u32;
            }
            if i == self.blocks.len() - 1 {
                block.write_to(writer, Some(loop_offset))?;
            } else {
                block.write_to(writer, None)?;
            }
        }

        Ok(())
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
    /// The cue points in the block.
    pub cues: Vec<CuePoint>,
}

impl Block {
    /// Creates an empty `Block`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Calculates the number of samples in the block according to `format`.
    pub fn num_samples(&self, format: DspFormat) -> usize {
        let len = (self.end_address as usize) + 1;
        match format {
            DspFormat::Adpcm => {
                let bytes = (len + 1) / 2;
                let num_frames = (bytes + adpcm::BYTES_PER_FRAME - 1) / adpcm::BYTES_PER_FRAME;
                len - len.min(num_frames * 2)
            }
            DspFormat::Pcm16 | DspFormat::Pcm8 => len,
        }
    }

    /// Creates a `Block` from mono ADPCM sample data.
    fn from_mono(samples: Samples<'_, GcAdpcm>) -> Result<Self> {
        if samples.channels != 1 {
            return Err(Error::StreamNotMono);
        }
        let size = GcAdpcm::address_to_byte_up(samples.len);
        if size > MONO_BLOCK_SIZE {
            return Err(Error::BlockTooLarge(size, MONO_BLOCK_SIZE));
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
        if size > STEREO_BLOCK_SIZE {
            return Err(Error::BlockTooLarge(size, STEREO_BLOCK_SIZE));
        }
        let mut channels = ArrayVec::new();
        let end_address = (left.len - 1) as u32;
        channels.push(BlockChannel::from_samples(left)?);
        channels.push(BlockChannel::from_samples(right)?);
        Ok(Self { end_address, channels, cues: vec![] })
    }

    /// Reads block data from `reader` using information from `header` and `channels`.
    #[instrument(level = "trace", skip_all)]
    fn read_from<R: Read + Seek + ?Sized>(
        reader: &mut R,
        header: &BlockHeader,
        channels: &[Channel],
    ) -> Result<Block> {
        // Constrain the reader to the block data
        let start_offset = reader.seek(SeekFrom::Current(0))?;
        let mut data_reader = Region::new(reader, start_offset, header.size as u64);

        // The left channel data is stored followed by the right data after alignment
        let mut block_channels = ArrayVec::new();
        let mut data_offset = 0;
        for (i, channel) in channels.iter().enumerate() {
            let mut data = vec![];
            let format = Format::from(channel.address.format);
            let data_size = format.address_to_byte_up(header.end_address as usize + 1) as u64;
            data_reader.seek(SeekFrom::Start(data_offset))?;
            (&mut data_reader).take(data_size).read_to_end(&mut data)?;
            if data.len() < data_size as usize {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
            }
            data_offset = align(data_offset + data_size, DATA_ALIGN as u64);
            block_channels.push(BlockChannel { initial_context: header.channel_contexts[i], data });
        }

        Ok(Block {
            end_address: header.end_address,
            channels: block_channels,
            cues: header.cues.clone(),
        })
    }

    /// Writes the block header and data to `writer`. If `next_offset` is not `None`, it will be
    /// used as the block's `next_offset` instead of the offset after this block.
    #[instrument(level = "trace", skip_all)]
    fn write_to<W: Write + Seek + ?Sized>(
        &self,
        writer: &mut W,
        next_offset: Option<u32>,
    ) -> Result<()> {
        if !(1..=2).contains(&self.channels.len()) {
            return Err(Error::UnsupportedChannels);
        }
        let mut channel_contexts = [adpcm::FrameContext::default(); 2];
        let mut data_size = 0;
        for (i, channel) in self.channels.iter().enumerate() {
            channel_contexts[i] = channel.initial_context;
            data_size += align(channel.data.len(), DATA_ALIGN);
        }

        let next_offset = if let Some(offset) = next_offset {
            offset
        } else {
            let current_offset = writer.seek(SeekFrom::Current(0))?;
            let total_size =
                BLOCK_HEADER_SIZE + align(CUE_SIZE * self.cues.len(), DATA_ALIGN) + data_size;
            (current_offset + total_size as u64) as u32
        };

        let header = BlockHeader {
            size: data_size as u32,
            end_address: self.end_address,
            next_offset,
            channel_contexts,
            cues: self.cues.clone(),
        };
        header.write_to(writer)?;

        for channel in &self.channels {
            writer.write_all(&channel.data)?;
            pad(&mut *writer, DATA_ALIGN as u64, 0)?;
        }
        Ok(())
    }
}

impl Debug for Block {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Block")
            .field("end_address", &self.end_address)
            .field("channels", &self.channels)
            .field("cues", &self.cues)
            .finish()
    }
}

/// A channel within an HPS block.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct BlockChannel {
    /// Initial playback parameters.
    pub initial_context: adpcm::FrameContext,
    /// The channel's audio data.
    pub data: Vec<u8>,
}

impl BlockChannel {
    /// Creates an empty `BlockChannel`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a `BlockChannel` with sample data from `samples`.
    fn from_samples(samples: Samples<'_, GcAdpcm>) -> Result<Self> {
        Ok(Self { initial_context: samples.params.context, data: samples.data.into_owned() })
    }
}

/// Reads sample data from a single HPS channel.
pub struct ChannelReader<'a> {
    blocks: &'a [Block],
    pos: usize,
    channel: usize,
    format: DspFormat,
    sample_rate: u32,
    adpcm: &'a adpcm::Info,
    tag: SourceTag,
}

impl<'a> ReadSamples<'a> for ChannelReader<'a> {
    type Format = AnyFormat;

    #[instrument(level = "trace", name = "ChannelReader", skip_all)]
    fn read_samples(&mut self) -> Result<Option<Samples<'a, Self::Format>>> {
        if self.pos >= self.blocks.len() {
            return Ok(None);
        }
        let block = &self.blocks[self.pos];
        self.pos += 1;

        let len = block.end_address as usize + 1;
        let data = &block.channels[self.channel].data;
        match self.format {
            DspFormat::Adpcm => Ok(Some(
                Samples::<GcAdpcm> {
                    channels: 1,
                    rate: self.sample_rate,
                    len,
                    data: data.into(),
                    params: adpcm::Info {
                        coefficients: self.adpcm.coefficients,
                        gain: self.adpcm.gain,
                        context: block.channels[self.channel].initial_context,
                    },
                }
                .cast(),
            )),

            // Chibi-Robo's engine doesn't actually play HPS files with non-ADPCM samples correctly,
            // but the format *technically* should support it and there's even some code referencing
            // other formats...
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
        ProgressHint::new(self.pos as u64, self.blocks.len() as u64)
    }

    fn data_remaining(&self) -> Option<u64> {
        Some(self.blocks[self.pos..].iter().map(|b| b.end_address as u64 + 1).sum())
    }

    fn cues(&self) -> Box<dyn Iterator<Item = audio::Cue> + '_> {
        Box::from(CueIterator::new(self.blocks, self.format))
    }
}

/// An iterator over the cues in an HPS stream.
pub struct CueIterator<'a> {
    /// The blocks to iterate over and obtain cue points from.
    blocks: &'a [Block],
    /// The format of the data in each block.
    format: DspFormat,
    /// The index of the current block.
    block_index: usize,
    /// The index of the current cue within the current block.
    cue_index: usize,
    /// The number of samples before the current block.
    sample_base: u64,
}

impl<'a> CueIterator<'a> {
    fn new(blocks: &'a [Block], format: DspFormat) -> Self {
        Self { blocks, format, block_index: 0, cue_index: 0, sample_base: 0 }
    }
}

impl Iterator for CueIterator<'_> {
    type Item = audio::Cue;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.block_index >= self.blocks.len() {
                return None;
            }
            let block = &self.blocks[self.block_index];
            if self.cue_index < block.cues.len() {
                let cue = &block.cues[self.cue_index];
                self.cue_index += 1;
                let name = format!("{}", cue.id);
                let start = self.sample_base + (cue.sample_index as u64);
                return Some(audio::Cue::new(name, start));
            } else {
                self.block_index += 1;
                self.cue_index = 0;
                self.sample_base += block.num_samples(self.format) as u64;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::sample::ReadSampleList;
    use crate::audio::Cue;
    use crate::{assert_write_and_read, test};
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

    #[test]
    fn test_read_block() -> Result<()> {
        let bytes = vec![0u8; 0x40];
        let header = BlockHeader {
            size: 0x40,
            end_address: 0x1f,
            next_offset: 0,
            channel_contexts: [Default::default(); 2],
            cues: vec![],
        };
        let channel = Channel {
            address: AudioAddress {
                looping: true,
                format: DspFormat::Adpcm,
                loop_address: 0x02,
                end_address: 0x1f,
                current_address: 0x02,
            },
            adpcm: adpcm::Info { coefficients: [0; 16], gain: 0, context: Default::default() },
        };
        let channels = [channel, channel];
        let mut reader = Cursor::new(bytes);
        let block = Block::read_from(&mut reader, &header, &channels)?;

        assert_eq!(block.end_address, 0x1f);
        assert_eq!(block.channels[0].data.len(), 0x10);
        assert_eq!(block.channels[1].data.len(), 0x10);

        Ok(())
    }

    #[test]
    fn test_hps_from_pcm_mono() -> Result<()> {
        let data = test::open_test_wav();
        let samples = Samples::<PcmS16Le>::from_pcm(data, 2, 44100);

        let splitter = samples.into_reader("test").split_channels();
        let hps = HpsStream::from_pcm(&mut splitter.left())?;
        assert_eq!(hps.sample_rate, 44100);
        assert_eq!(hps.channels.len(), 1);
        assert_eq!(hps.loop_start, Some(0));
        assert_eq!(hps.blocks.len(), 2);

        let left = &hps.channels[0];
        assert_eq!(left.address.end_address, 0x30af8);
        assert_eq!(left.adpcm.coefficients, test::TEST_WAV_LEFT_COEFFICIENTS);
        assert_eq!(left.adpcm.context.predictor_and_scale, test::TEST_WAV_LEFT_DSP[0] as u16);

        const EXPECTED_BLOCK_LENGTHS: &[usize] = &[0x10000, 0x857d];
        const EXPECTED_END_ADDRESSES: &[u32] = &[0x1ffff, 0x10af8];
        const EXPECTED_LAST_SAMPLES: &[[i16; 2]] = &[[0, 0], [1236, 1218]];

        let mut offset = 0;
        for (i, block) in hps.blocks.iter().enumerate() {
            let end_offset = offset + block.channels[0].data.len();
            assert_eq!(block.end_address, EXPECTED_END_ADDRESSES[i]);

            let left = &block.channels[0];
            assert_eq!(left.initial_context.predictor_and_scale, left.data[0] as u16);
            assert_eq!(left.initial_context.last_samples, EXPECTED_LAST_SAMPLES[i]);
            assert_eq!(left.data, &test::TEST_WAV_LEFT_DSP[offset..end_offset]);
            assert_eq!(left.data.len(), EXPECTED_BLOCK_LENGTHS[i]);
            offset = end_offset;
        }
        Ok(())
    }

    #[test]
    fn test_hps_from_pcm_stereo() -> Result<()> {
        let data = test::open_test_wav();
        let samples = Samples::<PcmS16Le>::from_pcm(data, 2, 44100);

        let hps = HpsStream::from_pcm(&mut samples.into_reader("test"))?;
        assert_eq!(hps.sample_rate, 44100);
        assert_eq!(hps.channels.len(), 2);
        assert_eq!(hps.loop_start, Some(0));
        assert_eq!(hps.blocks.len(), 4);

        let left = &hps.channels[0];
        assert_eq!(left.address.end_address, 0x30af8);
        assert_eq!(left.adpcm.coefficients, test::TEST_WAV_LEFT_COEFFICIENTS);
        assert_eq!(left.adpcm.context.predictor_and_scale, test::TEST_WAV_LEFT_DSP[0] as u16);

        let right = &hps.channels[1];
        assert_eq!(right.address.end_address, 0x30af8);
        assert_eq!(right.adpcm.coefficients, test::TEST_WAV_RIGHT_COEFFICIENTS);
        assert_eq!(right.adpcm.context.predictor_and_scale, test::TEST_WAV_RIGHT_DSP[0] as u16);

        const EXPECTED_BLOCK_LENGTHS: &[usize] = &[0x8000, 0x8000, 0x8000, 0x57d];
        const EXPECTED_END_ADDRESSES: &[u32] = &[0xffff, 0xffff, 0xffff, 0xaf8];
        const EXPECTED_LAST_SAMPLES_LEFT: &[[i16; 2]] =
            &[[0, 0], [-5232, -5240], [1236, 1218], [33, 42]];
        const EXPECTED_LAST_SAMPLES_RIGHT: &[[i16; 2]] =
            &[[0, 0], [730, 618], [1751, 1697], [-9, -3]];

        let mut offset = 0;
        for (i, block) in hps.blocks.iter().enumerate() {
            let end_offset = offset + block.channels[0].data.len();
            assert_eq!(block.end_address, EXPECTED_END_ADDRESSES[i]);

            let left = &block.channels[0];
            assert_eq!(left.initial_context.predictor_and_scale, left.data[0] as u16);
            assert_eq!(left.initial_context.last_samples, EXPECTED_LAST_SAMPLES_LEFT[i]);
            assert_eq!(left.data, &test::TEST_WAV_LEFT_DSP[offset..end_offset]);
            assert_eq!(left.data.len(), EXPECTED_BLOCK_LENGTHS[i]);

            let right = &block.channels[1];
            assert_eq!(right.initial_context.predictor_and_scale, right.data[0] as u16);
            assert_eq!(right.initial_context.last_samples, EXPECTED_LAST_SAMPLES_RIGHT[i]);
            assert_eq!(right.data, &test::TEST_WAV_RIGHT_DSP[offset..end_offset]);
            assert_eq!(right.data.len(), EXPECTED_BLOCK_LENGTHS[i]);
            offset = end_offset;
        }
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

        let hps = AdpcmHpsBuilder::with_mono(reader).build()?;
        assert_eq!(hps.blocks.len(), 4);
        assert_eq!(hps.blocks[0].cues, &[CuePoint { sample_index: 0, id: 1 }]);
        assert_eq!(
            hps.blocks[2].cues,
            &[CuePoint { sample_index: 0, id: 2 }, CuePoint { sample_index: 1, id: 3 }]
        );
        assert_eq!(hps.blocks[3].cues, &[CuePoint { sample_index: 14, id: 4 }]);
        Ok(())
    }
}
