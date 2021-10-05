use super::*;
use crate::audio::format::PcmS16Le;
use crate::audio::{Error, ReadSamples, Result, Samples};
use crate::common::{align, ReadFrom, ReadSeek, Region};
use log::{error, trace};
use std::collections::HashMap;
use std::io::{self, Read, Seek, SeekFrom};

/// RIFF data reader which can recursively read chunks.
struct RiffReader<'a> {
    /// The inner reader.
    reader: Box<dyn ReadSeek + 'a>,
    /// The offset that the next I/O operation should happen at, if any.
    /// Usually this will be the offset of the next chunk.
    next_offset: Option<u64>,
    /// The offset of the chunk header within the parent chunk.
    header_offset: u64,
    /// This chunk's ID.
    id: u32,
    /// The RIFF data's form ID.
    form: u32,
}

impl<'a> RiffReader<'a> {
    /// Opens a RIFF form, reading the header from `reader`.
    fn open_form(mut reader: impl ReadSeek + 'a) -> Result<Self> {
        let header_offset = reader.seek(SeekFrom::Current(0))?;
        let header = ChunkHeader::read_from(&mut reader)?;
        let id = header.id;
        if id != ID_RIFF {
            return Err(Error::InvalidRiff);
        }
        let form = reader.read_u32::<LE>()?;
        Ok(Self { reader: Box::from(reader), next_offset: None, header_offset, id, form })
    }

    /// Opens a RIFF chunk.
    fn open_chunk(reader: impl ReadSeek + 'a, header_offset: u64, id: u32, form: u32) -> Self {
        Self { reader: Box::from(reader), next_offset: None, header_offset, id, form }
    }

    /// Reads the next available chunk. If this is called immediately after another call to
    /// `next_chunk()`, it will automatically seek to the following chunk, otherwise it will read
    /// the chunk at the current offset.
    fn next_chunk(&mut self) -> Result<Option<RiffReader<'_>>> {
        let chunk = match ChunkHeader::read_from(self) {
            Ok(c) => c,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        // Set the next I/O operation to happen at the start of the next chunk. We can't just seek
        // here because `Region` would conflict with it. The caller can do whatever with the reader
        // we return and the next call to `next_chunk()` will always read from the right place.
        let data_offset = self.seek(SeekFrom::Current(0))?;
        let header_offset = data_offset - CHUNK_HEADER_SIZE;
        let end_offset = data_offset + chunk.size as u64;
        self.next_offset = Some(align(end_offset, RIFF_ALIGN));

        let region = Region::new(&mut self.reader, data_offset, chunk.size as u64);
        Ok(Some(RiffReader::open_chunk(region, header_offset, chunk.id, self.form)))
    }
}

impl Read for RiffReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // If an offset is queued, seek there first. This lets `next_chunk()` auto-seek to the end
        // of the current chunk if no intermediate I/O operations happen.
        if let Some(next_offset) = self.next_offset {
            self.reader.seek(SeekFrom::Start(next_offset))?;
            self.next_offset = None;
        }
        self.reader.read(buf)
    }
}

impl Seek for RiffReader<'_> {
    fn seek(&mut self, mut pos: SeekFrom) -> io::Result<u64> {
        // Make the seek relative to `next_offset` if necessary
        if let Some(next_offset) = self.next_offset {
            if let SeekFrom::Current(offset) = &mut pos {
                *offset += next_offset as i64;
            }
        }
        let new_offset = self.reader.seek(pos)?;
        self.next_offset = None;
        Ok(new_offset)
    }
}

/// Reads PCMS16LE WAV data from a stream.
pub struct WavReader<'a> {
    /// The reader open on the RIFF form.
    riff: RiffReader<'a>,
    /// An id -> offset mapping for each chunk in the form.
    chunk_offsets: HashMap<u32, u64>,
    /// true if the data chunk has not been read yet.
    data_available: bool,
    /// The number of channels in the audio data.
    pub channels: usize,
    /// The audio's sample rate.
    pub sample_rate: u32,
}

impl<'a> WavReader<'a> {
    /// Opens the WAV data provided by `reader` and reads its header.
    pub fn open(reader: impl ReadSeek + 'a) -> Result<Self> {
        Self::open_impl(Box::from(reader))
    }

    fn open_impl(reader: Box<dyn ReadSeek + 'a>) -> Result<Self> {
        let riff = RiffReader::open_form(reader)?;
        if riff.form != ID_WAVE {
            return Err(Error::InvalidWav);
        }
        let mut wav = Self {
            riff,
            chunk_offsets: HashMap::new(),
            data_available: true,
            channels: 0,
            sample_rate: 0,
        };
        wav.read_chunks()?;
        wav.read_format()?;
        Ok(wav)
    }

    /// Iterates over the chunks in the WAV data and builds the internal chunk offset map.
    fn read_chunks(&mut self) -> Result<()> {
        while let Some(chunk) = self.riff.next_chunk()? {
            if self.chunk_offsets.insert(chunk.id, chunk.header_offset).is_some() {
                error!("Duplicate WAV chunk {:#x}", chunk.id);
                return Err(Error::InvalidWav);
            }
        }
        Ok(())
    }

    /// Reads and validates format parameters from the fmt chunk.
    fn read_format(&mut self) -> Result<()> {
        let format = {
            let mut chunk = match self.open_chunk(ID_FMT)? {
                Some(c) => c,
                None => {
                    error!("WAV data is missing a fmt chunk");
                    return Err(Error::InvalidWav);
                }
            };
            FormatChunk::read_from(&mut chunk)?
        };
        trace!("fmt chunk: {:?}", format);
        if format.format_tag != WAVE_FORMAT_PCM {
            error!(
                "Unsupported WAV audio format {:#x}. PCMS16LE data is required.",
                format.format_tag
            );
            return Err(Error::InvalidWav);
        }
        if format.bits_per_sample != 16 {
            error!("WAV data must have 16 bits per sample");
            return Err(Error::InvalidWav);
        }
        if format.block_align != format.channels * 2 {
            error!("WAV data has unexpected block alignment: {:#x}", format.block_align);
            return Err(Error::InvalidWav);
        }
        self.channels = format.channels as usize;
        self.sample_rate = format.samples_per_sec;
        Ok(())
    }

    /// Opens a reader on chunk `id` if it is present.
    fn open_chunk(&mut self, id: u32) -> Result<Option<RiffReader<'_>>> {
        let offset = match self.chunk_offsets.get(&id) {
            Some(&o) => o,
            None => return Ok(None),
        };
        self.riff.seek(SeekFrom::Start(offset))?;
        match self.riff.next_chunk()? {
            Some(c) if c.id == id => Ok(Some(c)),
            _ => {
                error!("Failed to open chunk {:#x}", id);
                Err(Error::InvalidWav)
            }
        }
    }
}

impl ReadSamples<'static> for WavReader<'_> {
    type Format = PcmS16Le;
    fn read_samples(&mut self) -> Result<Option<Samples<'static, Self::Format>>> {
        if !self.data_available {
            return Ok(None);
        }
        let mut data = vec![];
        if let Some(mut chunk) = self.open_chunk(ID_DATA)? {
            chunk.read_to_end(&mut data)?;
        }
        self.data_available = false;
        if data.is_empty() {
            Ok(None)
        } else {
            Ok(Some(Samples::<Self::Format> {
                params: (),
                end_address: data.len() / 2 - 1,
                channels: self.channels,
                bytes: data.into(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::{open_test_wav, TEST_WAV};
    use crate::audio::format::{PcmS16Le, StaticFormat};
    use std::io::Cursor;

    #[test]
    fn test_read_wav() -> Result<()> {
        let expected = open_test_wav().into_inner();

        let mut wav = WavReader::open(Cursor::new(TEST_WAV))?;
        assert_eq!(wav.sample_rate, 44100);
        assert_eq!(wav.channels, 2);

        let samples = wav.coalesce_samples()?;
        assert_eq!(samples.end_address, expected.len() / 2 - 1);
        assert_eq!(samples.channels, 2);
        let end = PcmS16Le::size_of(samples.end_address + 1);
        assert!(&samples.bytes[..end] == expected);
        Ok(())
    }
}
