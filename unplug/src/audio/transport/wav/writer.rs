use super::*;
use crate::audio::format::{PcmFormat, PcmS16Le, ReadWriteBytes};
use crate::audio::sample::{PeekSamples, ReadSamples};
use crate::audio::{Error, Result};
use crate::common::WriteTo;
use byteorder::{WriteBytesExt, LE};
use std::borrow::Cow;
use std::io::{self, Seek, SeekFrom, Write};
use tracing::level_filters::STATIC_MAX_LEVEL;
use tracing::{debug, Level};

const DEFAULT_SOFTWARE_NAME: &str = concat!("unplug v", env!("CARGO_PKG_VERSION"));

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
        if end_offset % RIFF_ALIGN != 0 {
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

/// Writes out a WAV file from sample data and other parameters.
pub struct WavWriter<'a, 'b: 'a> {
    software_name: Cow<'static, str>,
    samples: PeekSamples<'b, Box<dyn ReadSamples<'b, Format = PcmS16Le> + 'a>>,
    channels: usize,
    sample_rate: u32,
}

impl<'a, 'b: 'a> WavWriter<'a, 'b> {
    /// Creates a new `WavWriter` which reads samples from `reader`.
    pub fn new(reader: impl ReadSamples<'b, Format = PcmS16Le> + 'a) -> Self {
        Self::new_impl(Box::from(reader))
    }

    fn new_impl(reader: Box<dyn ReadSamples<'b, Format = PcmS16Le> + 'a>) -> Self {
        Self {
            software_name: DEFAULT_SOFTWARE_NAME.into(),
            samples: reader.peekable(),
            channels: 0,
            sample_rate: 0,
        }
    }

    /// Sets the software name to write to the `INFO` chunk.
    pub fn software_name(&mut self, name: impl Into<String>) -> &mut Self {
        self.software_name = Cow::Owned(name.into());
        self
    }

    /// Prepares the final WAV file and writes it to `writer`.
    pub fn write_to(&mut self, writer: (impl Write + Seek)) -> Result<()> {
        self.peek_audio_info()?;
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
        let block_align = PcmS16Le::sample_to_index(1, self.channels) * 2;
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
        while let Some(samples) = self.samples.read_samples()? {
            if samples.channels != self.channels {
                return Err(Error::InconsistentChannels);
            }
            if samples.rate != self.sample_rate {
                return Err(Error::InconsistentSampleRate);
            }
            PcmS16Le::write_bytes(&mut *riff, &samples.data[..samples.len])?;
            num_samples += PcmS16Le::index_to_sample(samples.len, self.channels);
        }

        if STATIC_MAX_LEVEL >= Level::DEBUG {
            let tag = self.samples.tag();
            let duration = (num_samples as f64) / (self.sample_rate as f64);
            let hour = (duration as usize) / 60 / 60;
            let min = (duration as usize) / 60 % 60;
            let sec = (duration as usize) % 60;
            let msec = (duration.fract() * 1000.0).round() as usize;
            debug!(
                "Wrote {} samples from {:?} to WAV ({:>02}:{:>02}:{:>02}.{:>03})",
                num_samples, tag, hour, min, sec, msec
            );
        }
        riff.close_chunk(ID_DATA)?;
        Ok(())
    }

    fn peek_audio_info(&mut self) -> Result<()> {
        match self.samples.peek_samples()? {
            Some(s) => {
                self.channels = s.channels;
                self.sample_rate = s.rate;
                Ok(())
            }
            None => Err(Error::EmptyStream),
        }
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
        0x00, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, // samples
        0x04, 0x00, 0x05, 0x00, 0x06, 0x00, 0x07, 0x00,
    ];

    #[test]
    fn test_write_wav() -> Result<()> {
        let samples = Samples::<PcmS16Le>::from_pcm((0..8).collect::<Vec<_>>(), 2, 44100);
        let mut cursor = Cursor::new(Vec::<u8>::new());
        WavWriter::new(samples.into_reader("test")).software_name("test").write_to(&mut cursor)?;
        let bytes = cursor.into_inner();
        assert_eq!(bytes, EXPECTED_WAV);
        Ok(())
    }
}
