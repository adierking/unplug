use super::format::{PcmS16Le, RawFormat};
use super::{Error, ReadSamples, Result};
use crate::audio::format::StaticFormat;
use crate::common::WriteTo;
use byteorder::{WriteBytesExt, LE};
use log::{debug, log_enabled, Level};
use std::borrow::Cow;
use std::io::{self, Seek, SeekFrom, Write};

const fn fourcc(s: &[u8]) -> u32 {
    (s[0] as u32) | ((s[1] as u32) << 8) | ((s[2] as u32) << 16) | ((s[3] as u32) << 24)
}

const ID_DATA: u32 = fourcc(b"data");
const ID_FMT: u32 = fourcc(b"fmt ");
const ID_INFO: u32 = fourcc(b"INFO");
const ID_ISFT: u32 = fourcc(b"ISFT");
const ID_LIST: u32 = fourcc(b"LIST");
const ID_RIFF: u32 = fourcc(b"RIFF");
const ID_WAVE: u32 = fourcc(b"WAVE");

const WAVE_FORMAT_PCM: u16 = 0x1;

const DEFAULT_CHANNELS: usize = 1;
const DEFAULT_SAMPLE_RATE: u32 = 44100;
const DEFAULT_SOFTWARE_NAME: &str = concat!("unplug v", env!("CARGO_PKG_VERSION"));

/// A RIFF chunk header.
struct ChunkHeader {
    /// The FOURCC chunk type identifier.
    id: u32,
    /// The size of the chunk data, excluding this header.
    size: u32,
}

impl<W: Write> WriteTo<W> for ChunkHeader {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_u32::<LE>(self.id)?;
        writer.write_u32::<LE>(self.size)?;
        Ok(())
    }
}

/// WAVE `fmt ` chunk data.
struct FormatChunk {
    /// The WAVE format category.
    format_tag: u16,
    /// The number of channels stored in the file.
    channels: u16,
    /// The sample rate.
    samples_per_sec: u32,
    /// The average number of bytes per second that will be transferred.
    avg_bytes_per_sec: u32,
    /// The size of a complete sample, including data for all channels.
    block_align: u16,
    /// The number of bits per channel per sample.
    bits_per_sample: u16,
}

impl<W: Write> WriteTo<W> for FormatChunk {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        writer.write_u16::<LE>(self.format_tag)?;
        writer.write_u16::<LE>(self.channels)?;
        writer.write_u32::<LE>(self.samples_per_sec)?;
        writer.write_u32::<LE>(self.avg_bytes_per_sec)?;
        writer.write_u16::<LE>(self.block_align)?;
        writer.write_u16::<LE>(self.bits_per_sample)?;
        Ok(())
    }
}

/// Wraps a writer and provides methods for writing RIFF chunks.
struct RiffWriter<W: Write + Seek> {
    /// The inner writer.
    writer: W,
    /// A stack of (offset, ID) pairs for open chunks.
    open_chunks: Vec<(u64, u32)>,
}

impl<W: Write + Seek> RiffWriter<W> {
    /// Creates a new `RiffWriter` which wraps `writer`.
    fn new(writer: W) -> Self {
        Self { writer, open_chunks: vec![] }
    }

    /// Opens a RIFF form with type `form_type`.
    fn open_form(&mut self, form_type: u32) -> Result<()> {
        assert!(self.open_chunks.is_empty());
        self.open_chunk(ID_RIFF)?;
        self.writer.write_u32::<LE>(form_type)?;
        Ok(())
    }

    /// Closes the RIFF form.
    fn close_form(&mut self) -> Result<()> {
        self.close_chunk(ID_RIFF)?;
        assert!(self.open_chunks.is_empty());
        Ok(())
    }

    /// Opens a new chunk.
    fn open_chunk(&mut self, id: u32) -> Result<()> {
        // Save the offset of this header so we can go back and write the size in
        let offset = self.writer.seek(SeekFrom::Current(0))?;
        self.open_chunks.push((offset, id));

        // Write a header with the size set to 0 for now
        let header = ChunkHeader { id, size: 0 };
        header.write_to(&mut self.writer)?;
        Ok(())
    }

    /// Closes the current chunk. The ID must match.
    fn close_chunk(&mut self, id: u32) -> Result<()> {
        let (offset, actual_id) = self.open_chunks.pop().unwrap();
        assert_eq!(id, actual_id);

        let end_offset = self.writer.seek(SeekFrom::Current(0))?;
        let size = (end_offset - offset - 8) as u32;

        // Replace the filler chunk header we originally wrote
        self.writer.seek(SeekFrom::Start(offset))?;
        let header = ChunkHeader { id, size };
        header.write_to(&mut self.writer)?;

        self.writer.seek(SeekFrom::Start(end_offset))?;
        if end_offset % 2 != 0 {
            // The RIFF specification requires that chunks have a padding byte at the end if their
            // size is not word-aligned.
            self.writer.write_u8(0)?;
        }

        Ok(())
    }
}

impl<W: Write + Seek> Write for RiffWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.writer.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

/// Builds up a WAV file from sample data.
#[allow(single_use_lifetimes)]
pub struct WavBuilder<'a, 'b: 'a> {
    channels: usize,
    sample_rate: u32,
    software_name: Cow<'static, str>,
    samples: Option<Box<dyn ReadSamples<'b, Format = PcmS16Le> + 'a>>,
}

impl<'a, 'b: 'a> WavBuilder<'a, 'b> {
    /// Creates a new `WavBuilder` with default parameters.
    pub fn new() -> Self {
        Self {
            channels: DEFAULT_CHANNELS,
            sample_rate: DEFAULT_SAMPLE_RATE,
            software_name: DEFAULT_SOFTWARE_NAME.into(),
            samples: None,
        }
    }

    /// Sets the number of channels in the WAV file.
    pub fn channels(&mut self, channels: usize) -> &mut Self {
        self.channels = channels;
        self
    }

    /// Sets the sample rate of the WAV file.
    pub fn sample_rate(&mut self, sample_rate: u32) -> &mut Self {
        self.sample_rate = sample_rate;
        self
    }

    /// Sets the sample data for the WAV file.
    pub fn samples(
        &mut self,
        samples: Box<dyn ReadSamples<'b, Format = PcmS16Le> + 'a>,
    ) -> &mut Self {
        self.samples = Some(Box::new(samples));
        self
    }

    /// Sets the software name to write to the `INFO` chunk.
    pub fn software_name(&mut self, name: impl Into<String>) -> &mut Self {
        self.software_name = Cow::Owned(name.into());
        self
    }

    /// Finishes building and writes WAV data to `writer`.
    pub fn write_to(&mut self, writer: (impl Write + Seek)) -> Result<()> {
        let mut riff = RiffWriter::new(writer);
        riff.open_form(ID_WAVE)?;
        self.write_format(&mut riff)?;
        self.write_info(&mut riff)?;
        self.write_data(&mut riff)?;
        riff.close_form()?;
        Ok(())
    }

    /// Writes the `fmt ` chunk.
    fn write_format(&self, riff: &mut RiffWriter<impl Write + Seek>) -> Result<()> {
        riff.open_chunk(ID_FMT)?;
        let block_align = PcmS16Le::sample_to_byte(1, self.channels);
        let avg_bytes_per_sec = self.sample_rate * (block_align as u32);
        let format = FormatChunk {
            format_tag: WAVE_FORMAT_PCM,
            channels: self.channels as u16,
            samples_per_sec: self.sample_rate,
            avg_bytes_per_sec,
            block_align: block_align as u16,
            bits_per_sample: 16,
        };
        format.write_to(riff)?;
        riff.close_chunk(ID_FMT)?;
        Ok(())
    }

    /// Writes an `INFO` chunk with our program name.
    fn write_info(&self, riff: &mut RiffWriter<impl Write + Seek>) -> Result<()> {
        riff.open_chunk(ID_LIST)?;
        riff.write_u32::<LE>(ID_INFO)?;
        riff.open_chunk(ID_ISFT)?;
        riff.write_all(self.software_name.as_bytes())?;
        riff.write_u8(0)?;
        riff.close_chunk(ID_ISFT)?;
        riff.close_chunk(ID_LIST)?;
        Ok(())
    }

    /// Writes the `data` chunk.
    fn write_data(&mut self, riff: &mut RiffWriter<impl Write + Seek>) -> Result<()> {
        riff.open_chunk(ID_DATA)?;
        let mut num_samples = 0;
        if let Some(mut reader) = self.samples.take() {
            while let Some(samples) = reader.read_samples()? {
                let start = PcmS16Le::address_to_byte(samples.start_address);
                let end = PcmS16Le::address_to_byte(samples.end_address + 1);
                riff.write_all(&samples.bytes[start..end])?;
                num_samples += PcmS16Le::byte_to_sample(end - start, self.channels);
            }
        }
        riff.close_chunk(ID_DATA)?;
        if log_enabled!(Level::Debug) {
            let duration = (num_samples as f64) / (self.sample_rate as f64);
            let hour = (duration as usize) / 60 / 60;
            let min = (duration as usize) / 60 % 60;
            let sec = (duration as usize) % 60;
            let msec = (duration.fract() * 1000.0).round() as usize;
            debug!(
                "Wrote {} samples to WAV ({:>02}:{:>02}:{:>02}.{:>03})",
                num_samples, hour, min, sec, msec
            );
        }
        Ok(())
    }
}

impl Default for WavBuilder<'_, '_> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::Samples;
    use std::io::Cursor;

    #[rustfmt::skip]
    const EXPECTED_WAV: &[u8] = &[
        b'R', b'I', b'F', b'F', 0x4e, 0x00, 0x00, 0x00,
        b'W', b'A', b'V', b'E',

        b'f', b'm', b't', b' ', 0x10, 0x00, 0x00, 0x00,
        0x01, 0x00, // format_tag
        0x02, 0x00, // channels
        0x44, 0xac, 0x00, 0x00, // samples_per_sec
        0x10, 0xb1, 0x02, 0x00, // avg_bytes_per_sec
        0x04, 0x00, // block_align
        0x10, 0x00, // bits_per_sample

        b'L', b'I', b'S', b'T', 0x12, 0x00, 0x00, 0x00,
        b'I', b'N', b'F', b'O',
        b'I', b'S', b'F', b'T', 0x05, 0x00, 0x00, 0x00,
        b't', b'e', b's', b't', 0x00, 0x00, // software_name + padding

        b'd', b'a', b't', b'a', 0x10, 0x00, 0x00, 0x00,
        0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, // samples
        0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    ];

    #[test]
    fn test_write_wav() -> Result<()> {
        let bytes: Vec<u8> = (0..16).into_iter().collect();
        let samples = Samples::<'_, PcmS16Le> {
            context: (),
            start_address: 0,
            end_address: 0x7,
            channels: 2,
            bytes: Cow::Borrowed(&bytes),
        };
        let mut cursor = Cursor::new(Vec::<u8>::new());
        WavBuilder::new()
            .channels(2)
            .sample_rate(44100)
            .software_name("test")
            .samples(Box::new(samples.into_reader()))
            .write_to(&mut cursor)?;
        let bytes = cursor.into_inner();
        assert_eq!(bytes, EXPECTED_WAV);
        Ok(())
    }
}