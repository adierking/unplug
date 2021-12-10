use super::*;
use crate::audio::format::{PcmS16Le, ReadWriteBytes, StaticFormat};
use crate::audio::{
    Cue, CueKind, Error, Format, ProgressHint, ReadSamples, Result, Samples, SourceTag,
};
use crate::common::{align, ReadFrom, ReadSeek, Region};
use std::collections::HashMap;
use std::io::{self, Read, Seek, SeekFrom};
use std::num::NonZeroU64;
use tracing::{error, instrument, trace, warn};

/// RIFF data reader which can recursively read chunks.
struct RiffReader<'a> {
    /// The inner reader.
    reader: Box<dyn ReadSeek + 'a>,
    /// The offset that the next I/O operation should happen at, if any.
    /// Usually this will be the offset of the next chunk.
    next_offset: Option<u64>,
    /// The offset of the chunk header within the parent chunk.
    header_offset: u64,
    /// The chunk's header.
    header: ChunkHeader,
    /// The RIFF data's form ID.
    form: u32,
}

impl<'a> RiffReader<'a> {
    /// Opens a RIFF form, reading the header from `reader`.
    fn open_form(mut reader: impl ReadSeek + 'a) -> Result<Self> {
        let header_offset = reader.seek(SeekFrom::Current(0))?;
        let header = ChunkHeader::read_from(&mut reader)?;
        if header.id != ID_RIFF {
            return Err(Error::InvalidRiff);
        }
        let form = reader.read_u32::<LE>()?;
        Ok(Self { reader: Box::from(reader), next_offset: None, header_offset, header, form })
    }

    /// Opens a RIFF chunk.
    fn open_chunk(
        reader: impl ReadSeek + 'a,
        header_offset: u64,
        header: ChunkHeader,
        form: u32,
    ) -> Self {
        Self { reader: Box::from(reader), next_offset: None, header_offset, header, form }
    }

    /// Reads the next available chunk. If this is called immediately after another call to
    /// `next_chunk()`, it will automatically seek to the following chunk, otherwise it will read
    /// the chunk at the current offset.
    fn next_chunk(&mut self) -> Result<Option<RiffReader<'_>>> {
        let header = match ChunkHeader::read_from(self) {
            Ok(c) => c,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        // Set the next I/O operation to happen at the start of the next chunk. We can't just seek
        // here because `Region` would conflict with it. The caller can do whatever with the reader
        // we return and the next call to `next_chunk()` will always read from the right place.
        let data_offset = self.seek(SeekFrom::Current(0))?;
        let header_offset = data_offset - CHUNK_HEADER_SIZE;
        let end_offset = data_offset + header.size as u64;
        self.next_offset = Some(align(end_offset, RIFF_ALIGN));

        let region = Region::new(&mut self.reader, data_offset, header.size as u64);
        Ok(Some(RiffReader::open_chunk(region, header_offset, header, self.form)))
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
    /// An id -> offsets mapping for each IFF list in the form.
    list_offsets: HashMap<u32, Vec<u64>>,
    /// The number of bytes which have not been read from the data chunk yet.
    data_remaining: u32,
    /// The audio source tag for debugging purposes.
    tag: SourceTag,
    /// Audio cue points.
    cues: Vec<Cue>,
    /// The number of channels in the audio data.
    pub channels: usize,
    /// The audio's sample rate.
    pub sample_rate: u32,
}

impl<'a> WavReader<'a> {
    /// Opens the WAV data provided by `reader` and reads its header. `tag` is a string or tag to
    /// identify the stream for debugging purposes.
    pub fn new(reader: impl ReadSeek + 'a, tag: impl Into<SourceTag>) -> Result<Self> {
        Self::new_impl(Box::from(reader), tag.into())
    }

    #[instrument(level = "trace", skip_all)]
    fn new_impl(reader: Box<dyn ReadSeek + 'a>, tag: SourceTag) -> Result<Self> {
        let riff = RiffReader::open_form(reader)?;
        if riff.form != ID_WAVE {
            return Err(Error::InvalidWav);
        }
        let mut wav = Self {
            riff,
            chunk_offsets: HashMap::new(),
            list_offsets: HashMap::new(),
            data_remaining: 0,
            tag,
            cues: vec![],
            channels: 0,
            sample_rate: 0,
        };
        wav.read_chunks()?;
        wav.read_format()?;
        wav.read_cues()?;
        Ok(wav)
    }

    /// Iterates over the chunks in the WAV data and builds the internal chunk offset map.
    fn read_chunks(&mut self) -> Result<()> {
        while let Some(mut chunk) = self.riff.next_chunk()? {
            if chunk.header.id == ID_LIST {
                // Special case for IFF lists - storing the offset of the LIST chunk isn't useful
                // when there can be different kinds of lists
                let list_type = chunk.read_u32::<LE>()?;
                self.list_offsets.entry(list_type).or_default().push(chunk.header_offset);
            } else if self.chunk_offsets.insert(chunk.header.id, chunk.header_offset).is_some() {
                error!("Duplicate WAV chunk {:#x}", chunk.header.id);
                return Err(Error::InvalidWav);
            }
            if chunk.header.id == ID_DATA {
                self.data_remaining = chunk.header.size;
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

    /// Reads cue point information from the cue chunk.
    fn read_cues(&mut self) -> Result<()> {
        let mut cues: HashMap<u32, Cue> = {
            let mut chunk = match self.open_chunk(ID_CUE)? {
                Some(c) => c,
                None => return Ok(()),
            };
            let raw_cues = CueChunk::read_from(&mut chunk)?;
            trace!("cue chunk: {:?}", raw_cues);
            raw_cues
                .points
                .into_iter()
                .filter(|c| c.chunk_id == ID_DATA)
                .map(|c| (c.name, Cue::new("", c.sample_offset as u64)))
                .collect()
        };

        // Cue names have to be pulled from the adtl list
        self.open_lists(ID_ADTL, |mut adtl| {
            while let Some(mut chunk) = adtl.next_chunk()? {
                match chunk.header.id {
                    ID_LABL => {
                        let label = LabelChunk::read_from(&mut chunk)?;
                        if let Some(cue) = cues.get_mut(&label.name) {
                            cue.name = label.text.to_string_lossy().into();
                        }
                    }
                    ID_LTXT => {
                        let label = LabelTextChunk::read_from(&mut chunk)?;
                        if let Some(cue) = cues.get_mut(&label.name) {
                            // ltxt is primarily useful for getting the cue duration
                            if let Some(duration) = NonZeroU64::new(label.sample_length.into()) {
                                cue.kind = CueKind::Range(duration);
                            }
                            // Give the name in labl precedence over this if there's more than one
                            // chunk for a cue
                            if cue.name.is_empty() {
                                cue.name = label.text.to_string_lossy().into();
                            }
                        }
                    }
                    _ => continue,
                };
            }
            Ok(())
        })?;

        // Make sure unnamed cues have some sort of name
        for (id, cue) in &mut cues {
            if cue.name.is_empty() {
                cue.name = format!("{}", id).into();
            }
        }

        // Sort and deduplicate in case any cues are duplicated for whatever reason
        self.cues = cues.into_values().collect();
        self.cues.sort_unstable();
        self.cues.dedup();
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
            Some(c) if c.header.id == id => Ok(Some(c)),
            _ => {
                error!("Failed to open chunk {:#x}", id);
                Err(Error::InvalidWav)
            }
        }
    }

    /// Loops through all IFF lists named `id` and passes a reader for each one to `callback`.
    fn open_lists<F>(&mut self, id: u32, mut callback: F) -> Result<()>
    where
        F: FnMut(RiffReader<'_>) -> Result<()>,
    {
        let offsets = match self.list_offsets.get(&id) {
            Some(o) => o,
            None => return Ok(()),
        };
        for &offset in offsets {
            self.riff.seek(SeekFrom::Start(offset))?;
            let mut list = match self.riff.next_chunk()? {
                Some(c) if c.header.id == ID_LIST => c,
                _ => {
                    error!("Failed to open chunk {:#x}", ID_LIST);
                    return Err(Error::InvalidWav);
                }
            };
            let list_type = list.read_u32::<LE>()?;
            if list_type != id {
                error!("Unexpected list type: {:#x}", list_type);
                return Err(Error::InvalidWav);
            }
            callback(list)?;
        }
        Ok(())
    }
}

impl ReadSamples<'static> for WavReader<'_> {
    type Format = PcmS16Le;

    #[instrument(level = "trace", name = "WavReader", skip_all)]
    fn read_samples(&mut self) -> Result<Option<Samples<'static, Self::Format>>> {
        if self.data_remaining == 0 {
            return Ok(None);
        }
        let mut samples = vec![];
        if let Some(chunk) = self.open_chunk(ID_DATA)? {
            samples = PcmS16Le::read_bytes(chunk)?;
        }
        self.data_remaining = 0;
        if samples.is_empty() {
            Ok(None)
        } else {
            Ok(Some(Samples::from_pcm(samples, self.channels, self.sample_rate)))
        }
    }

    fn format(&self) -> Format {
        Self::Format::FORMAT
    }

    fn tag(&self) -> &SourceTag {
        &self.tag
    }

    fn progress(&self) -> Option<ProgressHint> {
        // We just read everything at once, so...
        if self.data_remaining > 0 {
            ProgressHint::new(0, 1)
        } else {
            ProgressHint::new(1, 1)
        }
    }

    fn data_remaining(&self) -> Option<u64> {
        Some(self.data_remaining as u64 / 2) // Convert from bytes to samples
    }

    fn cues(&self) -> Box<dyn Iterator<Item = Cue> + '_> {
        Box::from(self.cues.iter().cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::{open_test_wav, TEST_WAV, TEST_WAV_CUES};
    use std::io::Cursor;

    #[test]
    fn test_read_wav() -> Result<()> {
        let expected = open_test_wav();

        let mut wav = WavReader::new(Cursor::new(TEST_WAV), "test")?;
        assert_eq!(wav.sample_rate, 44100);
        assert_eq!(wav.channels, 2);
        assert_eq!(wav.progress(), ProgressHint::new(0, 1));
        assert_eq!(wav.data_remaining(), Some(expected.len() as u64));

        let samples = wav.read_all_samples()?;
        assert_eq!(wav.progress(), ProgressHint::new(1, 1));
        assert_eq!(wav.data_remaining(), Some(0));
        assert_eq!(samples.channels, 2);
        assert_eq!(samples.rate, 44100);
        assert_eq!(samples.len, expected.len());
        assert!(samples.data[..samples.len] == expected);
        Ok(())
    }

    #[test]
    fn test_read_wav_cues() -> Result<()> {
        let wav = WavReader::new(Cursor::new(TEST_WAV_CUES), "test")?;
        let cues = wav.cues().collect::<Vec<_>>();
        assert_eq!(
            cues,
            &[Cue::new("start", 0), Cue::new_range("range", 44100, 88200), Cue::new("end", 174489)]
        );
        Ok(())
    }
}
