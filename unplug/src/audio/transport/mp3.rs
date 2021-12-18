use crate::audio::format::{PcmS16Le, StaticFormat};
use crate::audio::{Error, Format, ProgressHint, ReadSamples, Result, Samples, SourceTag};
use crate::common::{ReadSeek, Region};
use byteorder::{ReadBytesExt, BE};
use minimp3::{Decoder, Error as Mp3Error, Frame};
use std::io::{Read, Seek, SeekFrom};
use std::iter;
use tracing::{debug, error, instrument, trace, warn};

/// Reads audio samples from MP3 data.
pub struct Mp3Reader<'r> {
    decoder: Decoder<Box<dyn ReadSeek + 'r>>,
    info: Info,
    channels: usize,
    sample_rate: u32,
    first_frame: Option<Frame>,
    frame_index: u64,
    tag: SourceTag,
}

impl<'r> Mp3Reader<'r> {
    /// Creates a new `Mp3Reader` which reads MP3 data from `reader`. The first frame will be
    /// decoded to get stream metadata. `tag` is a string or tag to identify the stream for
    /// debugging purposes.
    pub fn new(reader: impl ReadSeek + 'r, tag: impl Into<SourceTag>) -> Result<Self> {
        Self::new_impl(Box::from(reader), tag.into())
    }

    #[instrument(level = "trace", skip_all)]
    fn new_impl(mut reader: Box<dyn ReadSeek + 'r>, tag: SourceTag) -> Result<Self> {
        let start_offset = reader.seek(SeekFrom::Current(0))?;
        let mut info = Info::default();
        if let Err(e) = analyze_mp3(&mut reader, &mut info) {
            warn!("Could not fully analyze MP3 file: {:#}", e);
        }
        debug!("MP3 info: {:?}", info);

        // HACK: minimp3 might reject the last frame if it reads the file footer. If we know where
        // the footer is, limit what gets read.
        if info.audio_size > 0 {
            reader = Box::from(Region::new(reader, info.audio_offset, info.audio_size));
        } else {
            reader.seek(SeekFrom::Start(start_offset))?;
        }

        let mut decoder = Decoder::new(reader);
        // We have to get stream metadata from the first frame
        let frame = match Self::read_frame(&mut decoder)? {
            Some(frame) => frame,
            None => return Err(Error::EmptyStream),
        };
        let layer = frame.layer;
        let channels = frame.channels;
        let sample_rate = frame.sample_rate as u32;
        debug!(
            "Opened MPEG Layer {} stream {:?}: {} Hz, {} channel(s)",
            layer, tag, sample_rate, channels
        );

        // *Discard* the first frame if it's the Xing frame
        let first_frame = if info.has_xing_frame { None } else { Some(frame) };
        Ok(Self { decoder, info, channels, sample_rate, first_frame, frame_index: 0, tag })
    }

    /// Gets the number of channels in the stream.
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Gets the audio sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Decodes the next frame, or returns `Ok(None)` on EOF.
    fn next_frame(&mut self) -> Result<Option<Frame>> {
        match self.first_frame.take() {
            Some(frame) => Ok(Some(frame)),
            None => Self::read_frame(&mut self.decoder),
        }
    }

    /// Reads a frame from `decoder`, or returns `Ok(None)` on EOF.
    #[instrument(level = "trace", skip_all)]
    fn read_frame(decoder: &mut Decoder<Box<dyn ReadSeek + 'r>>) -> Result<Option<Frame>> {
        match decoder.next_frame() {
            Ok(frame) => Ok(Some(frame)),
            Err(Mp3Error::Eof) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

impl ReadSamples<'static> for Mp3Reader<'_> {
    type Format = PcmS16Le;

    #[instrument(level = "trace", name = "Mp3Reader", skip_all)]
    fn read_samples(&mut self) -> Result<Option<Samples<'static, Self::Format>>> {
        let frame = match self.next_frame()? {
            Some(frame) => frame,
            None => return Ok(None),
        };
        self.frame_index += 1;

        if frame.sample_rate as u32 != self.sample_rate {
            error!(
                "Inconsistent MP3 sample rate: got {} but the first frame had {}",
                frame.sample_rate, self.sample_rate
            );
            return Err(Error::InconsistentSampleRate);
        }
        if frame.channels != self.channels {
            error!(
                "Inconsistent MP3 channel count: got {} but the first frame had {}",
                frame.channels, self.channels
            );
            return Err(Error::InconsistentChannels);
        }

        // Use the frame index to determine whether this is the first/last frame and strip padding
        // as necessary
        let mut data = frame.data;
        if self.frame_index == 1 {
            let delay = self.info.delay_samples as usize * frame.channels;
            if delay > 0 && delay <= data.len() {
                trace!("Frame {}: trim {} samples", self.frame_index, self.info.delay_samples);
                data.drain(0..delay);
            }
        }
        if self.frame_index == self.info.num_frames as u64 {
            let pad = self.info.pad_samples as usize * frame.channels;
            if pad > 0 && pad <= data.len() {
                trace!("Frame {}: trim {} samples", self.frame_index, self.info.pad_samples);
                data.truncate(data.len() - pad);
            }
        }

        Ok(Some(Samples::<PcmS16Le>::from_pcm(data, frame.channels, frame.sample_rate as u32)))
    }

    fn format(&self) -> Format {
        Self::Format::FORMAT
    }

    fn tag(&self) -> &SourceTag {
        &self.tag
    }

    fn progress(&self) -> Option<ProgressHint> {
        ProgressHint::new(self.frame_index, self.info.num_frames as u64)
    }

    fn data_remaining(&self) -> Option<u64> {
        // TODO: Can we get this from the frame count?
        None
    }

    fn cues(&self) -> Box<dyn Iterator<Item = crate::audio::Cue> + '_> {
        Box::from(iter::empty())
    }
}

/// MP3 encoding and file structure info.
#[derive(Debug, Default)]
struct Info {
    /// Offset of the first frame in the file.
    audio_offset: u64,
    /// Size of the audio data in the file, or 0 if unknown.
    audio_size: u64,
    /// true if the first frame is the Xing frame and should be ignored.
    has_xing_frame: bool,
    /// Number of audio frames past the Xing frame.
    num_frames: u32,
    /// The number of samples added at the beginning of the stream.
    delay_samples: u16,
    /// The number of samples added at the end of the stream.
    pad_samples: u16,
}

const TAG_ID3: u32 = 0x494433; // 'ID3'
const TAG_INFO: u32 = 0x496e666f; // 'Info'
const TAG_TAG: u32 = 0x544147; // 'TAG'
const TAG_XING: u32 = 0x58696e67; // 'Xing'

/// Size of the buffer to look for the first MPEG frame within
const FRAME_SEARCH_SIZE: u64 = 0x1000;
/// Size of an ID3v1 footer
const ID3V1_SIZE: u64 = 0x80;

/// Mask to use to check for the frame sync bits
const SYNC_MASK: u16 = 0xffe0;
/// MPEG version 1 header bits
const VERSION_MPEG_1: u32 = 0b11;
/// Mono audio header bits
const MODE_MONO: u32 = 0b11;

const XING_FLAG_FRAMES: u32 = 0x1;
const XING_FLAG_BYTES: u32 = 0x2;
const XING_FLAG_TOC: u32 = 0x4;
const XING_FLAG_QUALITY: u32 = 0x8;
const XING_TOC_SIZE: u64 = 100;

/// Offset of the encoder delay after the end of the info header
const ENCODER_DELAY_OFFSET: u64 = 0x15;
/// Fixed decoder delay used to compensate the encoder delays
const DECODER_DELAY: u16 = 529;

/// Reads a synchsafe 32-bit integer from `reader`.
fn read_synchsafe_u32(reader: &mut impl Read) -> Result<u32> {
    let mut b = [0u8; 4];
    reader.read_exact(&mut b)?;
    Ok((((b[0] & 0x7f) as u32) << 21)
        | (((b[1] & 0x7f) as u32) << 14)
        | (((b[2] & 0x7f) as u32) << 7)
        | ((b[3] & 0x7f) as u32))
}

/// If an ID3v2 header is at the current position in the stream, skips past it.
fn detect_id3_header(reader: &mut (impl Read + Seek)) -> Result<Option<u64>> {
    if reader.read_u24::<BE>()? != TAG_ID3 {
        return Ok(None);
    }
    let _version_and_flags = reader.read_u24::<BE>()?;
    let size = read_synchsafe_u32(reader)?;
    trace!("Found ID3v2 header with size {:#x}", size);
    Ok(Some(reader.seek(SeekFrom::Current(size as i64))?))
}

/// Attempts to detect an ID3v1 footer. Returns its offset if present.
fn detect_footer(reader: &mut (impl Read + Seek)) -> Result<Option<u64>> {
    let offset = reader.seek(SeekFrom::End(-(ID3V1_SIZE as i64)))?;
    let tag = reader.read_u24::<BE>()?;
    if tag == TAG_TAG {
        trace!("Found ID3v1 footer at {:#x}", offset);
        Ok(Some(offset))
    } else {
        Ok(None)
    }
}

/// Locates the offset of the first MPEG frame in `reader`.
#[instrument(level = "trace", skip(reader))]
fn find_first_frame(reader: &mut impl Read, base_offset: u64) -> Result<Option<u64>> {
    // Search only the next 4 KB in case this isn't a valid MP3 file. This is kinda hacky, but the
    // worst case scenario here is that we just don't get to remove the audio padding. Oh well.
    let mut bytes = vec![];
    reader.take(FRAME_SEARCH_SIZE).read_to_end(&mut bytes)?;
    let mut sync = 0;
    for (i, &b) in bytes.iter().enumerate() {
        sync = (sync << 8) | (b as u16);
        if sync & SYNC_MASK == SYNC_MASK {
            let frame_offset = (i - 1) as u64;
            return Ok(Some(base_offset + frame_offset));
        }
    }
    Ok(None)
}

/// Analyzes an MP3 file to determine its structure and encoding parameters.
#[instrument(level = "trace", skip_all)]
fn analyze_mp3(reader: &mut (impl Read + Seek), info: &mut Info) -> Result<()> {
    // The ID3 header can be arbitrarily large, so we have to skip past it first
    info.audio_offset = reader.seek(SeekFrom::Current(0))?;
    match detect_id3_header(reader) {
        Ok(Some(offset)) => info.audio_offset = offset,
        Ok(None) => (),
        Err(e) => warn!("Failed to detect ID3v2 header: {:#}", e),
    }

    // Detect any ID3v1 footer at the end of the file. Although the Xing tag might give us the exact
    // size of the audio data, we have to be able to handle files without it.
    let footer_offset = match detect_footer(reader) {
        Ok(Some(offset)) => offset,
        Ok(None) => reader.seek(SeekFrom::End(0))?,
        Err(e) => {
            warn!("Failed to detect ID3v1 footer: {:#}", e);
            reader.seek(SeekFrom::End(0))?
        }
    };
    info.audio_size = footer_offset - info.audio_offset;

    trace!("Searching for frames starting at {:#x}", info.audio_offset);
    reader.seek(SeekFrom::Start(info.audio_offset))?;
    let frame_offset = match find_first_frame(reader, info.audio_offset)? {
        Some(o) => o,
        None => return Ok(()),
    };
    info.audio_size -= frame_offset - info.audio_offset;
    info.audio_offset = frame_offset;
    trace!("Found first MPEG frame at {:#x}", frame_offset);

    // The VBR info header is after the frame's side info, whose size varies depending on the format
    // version, layer, and number of channels.
    reader.seek(SeekFrom::Start(frame_offset))?;
    let header = reader.read_u32::<BE>()?;
    let version = (header >> 19) & 0x3;
    let layer = 4 - ((header >> 17) & 0x3);
    let mode = (header >> 4) & 0x3;
    let side_info_size = match (layer, version, mode) {
        (3, VERSION_MPEG_1, MODE_MONO) => 17,
        (3, VERSION_MPEG_1, _) => 32,
        (3, _, MODE_MONO) => 9,
        (3, _, _) => 17,
        (_, _, _) => 0,
    };

    let tag_offset = frame_offset + 4 + side_info_size;
    reader.seek(SeekFrom::Start(tag_offset))?;
    let tag = reader.read_u32::<BE>()?;
    if tag != TAG_XING && tag != TAG_INFO {
        return Ok(());
    }
    info.has_xing_frame = true;
    trace!("Found info tag at {:#x}", tag_offset);

    // Skip past the first part of the VBR header, where the flags field indicates what is present
    let flags = reader.read_u32::<BE>()?;
    if flags & XING_FLAG_FRAMES != 0 {
        info.num_frames = reader.read_u32::<BE>()?;
    }
    if flags & XING_FLAG_BYTES != 0 {
        info.audio_size = info.audio_size.min(reader.read_u32::<BE>()? as u64);
    }
    if flags & XING_FLAG_TOC != 0 {
        reader.seek(SeekFrom::Current(XING_TOC_SIZE as i64))?;
    }
    if flags & XING_FLAG_QUALITY != 0 {
        let _quality = reader.read_u32::<BE>()?;
    }

    // Read and unpack the 12-bit encoder delays
    reader.seek(SeekFrom::Current(ENCODER_DELAY_OFFSET as i64))?;
    let delay = reader.read_u24::<BE>()?;
    info.delay_samples = ((delay >> 12) & 0xfff) as u16;
    info.pad_samples = (delay & 0xfff) as u16;
    // libmp3lame checks these values against 3000 to see if they're valid. *shrug*
    if info.delay_samples < 3000 && info.pad_samples < 3000 {
        // Correct the delays to compensate for the fixed decoder delay
        info.delay_samples += DECODER_DELAY;
        info.pad_samples -= info.pad_samples.min(DECODER_DELAY);
    } else {
        info.delay_samples = 0;
        info.pad_samples = 0;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::transport::WavReader;
    use crate::test::{assert_samples_close, TEST_MP3, TEST_MP3_WAV};
    use std::io::Cursor;

    #[test]
    fn test_read_mp3() -> Result<()> {
        let mut mp3 = Mp3Reader::new(Cursor::new(TEST_MP3), "TEST_MP3")?;
        assert_eq!(mp3.sample_rate(), 44100);
        assert_eq!(mp3.channels(), 2);
        let samples = mp3.read_all_samples()?;
        let reference =
            WavReader::new(Cursor::new(TEST_MP3_WAV), "TEST_MP3_WAV")?.read_all_samples()?;
        // Compare with a tolerance of +/- 1 (minimp3 vs ffmpeg)
        assert_samples_close(&samples, &reference, 1);
        Ok(())
    }
}
