use super::*;
use crate::audio::format::{PcmFormat, PcmS16Le, ReadWriteBytes};
use crate::audio::sample::{PeekSamples, ReadSamples};
use crate::audio::{Error, ProgressHint, Result};
use crate::common::WriteTo;
use byteorder::{WriteBytesExt, LE};
use std::borrow::Cow;
use std::io::{self, Seek, SeekFrom, Write};
use tracing::level_filters::STATIC_MAX_LEVEL;
use tracing::{debug, instrument, Level};

const DEFAULT_SOFTWARE_NAME: &str = concat!("unplug v", env!("CARGO_PKG_VERSION"));

/// Windows UTF-8 code page.
const CP_UTF8: u16 = 65001;

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
pub struct WavWriter<'r, 's: 'r> {
    samples: PeekSamples<'s, Box<dyn ReadSamples<'s, Format = PcmS16Le> + 'r>>,
    channels: usize,
    sample_rate: u32,
    software_name: Cow<'static, str>,
    on_progress: Option<Box<dyn FnMut(Option<ProgressHint>) + 'r>>,
}

impl<'r, 's: 'r> WavWriter<'r, 's> {
    /// Creates a new `WavWriter` which reads samples from `reader`.
    pub fn new(reader: impl ReadSamples<'s, Format = PcmS16Le> + 'r) -> Self {
        Self::new_impl(Box::from(reader))
    }

    fn new_impl(reader: Box<dyn ReadSamples<'s, Format = PcmS16Le> + 'r>) -> Self {
        Self {
            samples: reader.peekable(),
            channels: 0,
            sample_rate: 0,
            software_name: DEFAULT_SOFTWARE_NAME.into(),
            on_progress: None,
        }
    }

    /// Sets the software name to write to the `INFO` chunk.
    pub fn software_name(&mut self, name: impl Into<String>) -> &mut Self {
        self.software_name = Cow::Owned(name.into());
        self
    }

    /// Sets a callback to run for progress updates. If the total amount of work is unknown, the
    /// callback will still be invoked with a `None` hint.
    pub fn on_progress(&mut self, callback: impl FnMut(Option<ProgressHint>) + 'r) -> &mut Self {
        self.on_progress = Some(Box::from(callback));
        self
    }

    /// Prepares the final WAV file and writes it to `writer`.
    #[instrument(level = "trace", skip_all)]
    pub fn write_to(&mut self, writer: (impl Write + Seek)) -> Result<()> {
        self.update_progress();
        self.peek_audio_info()?;
        let mut riff = RiffWriter::new(writer);
        riff.open_form(ID_WAVE)?;
        self.write_format(&mut riff)?;
        self.write_cues(&mut riff)?;
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

    /// Writes the `cue ` and `adtl` chunks with cue information.
    fn write_cues(&self, riff: &mut RiffWriter<impl Write + Seek>) -> Result<()> {
        let mut cues = self.samples.cues().collect::<Vec<_>>();
        if cues.is_empty() {
            return Ok(());
        }
        cues.sort_unstable();
        cues.dedup();

        let mut chunk = CueChunk::default();
        let mut labels: Vec<LabelChunk> = vec![];
        let mut ltxts: Vec<LabelTextChunk> = vec![];
        for (i, cue) in cues.into_iter().enumerate() {
            // Each cue gets an entry in the cue chunk and a label in the labl list.
            let id = (i + 1) as u32;
            chunk.points.push(CuePoint {
                name: id,
                position: cue.start as u32,
                chunk_id: ID_DATA,
                chunk_start: 0,
                block_start: 0,
                sample_offset: cue.start as u32,
            });
            let text = CString::new(&*cue.name).unwrap();
            // Cues with a duration need an entry in ltxt to store the duration.
            if cue.duration > 0 {
                ltxts.push(LabelTextChunk {
                    name: id,
                    sample_length: cue.duration as u32,
                    purpose: ID_RGN,
                    country: 0,
                    language: 0,
                    dialect: 0,
                    code_page: CP_UTF8,
                    // What's the difference between this and the text in labl? Who knows. Just
                    // duplicate the label name in case some program out there wants it.
                    text: text.clone(),
                });
            }
            labels.push(LabelChunk { name: id, text });
        }

        riff.open_chunk(ID_CUE)?;
        chunk.write_to(riff)?;
        riff.close_chunk(ID_CUE)?;

        riff.open_chunk(ID_LIST)?;
        riff.write_u32::<LE>(ID_ADTL)?;
        for label in labels {
            riff.open_chunk(ID_LABL)?;
            label.write_to(riff)?;
            riff.close_chunk(ID_LABL)?;
        }
        for ltxt in ltxts {
            riff.open_chunk(ID_LTXT)?;
            ltxt.write_to(riff)?;
            riff.close_chunk(ID_LTXT)?;
        }
        riff.close_chunk(ID_LIST)?;
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
    #[instrument(level = "trace", skip_all)]
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
            self.update_progress();
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

    fn update_progress(&mut self) {
        if let Some(callback) = &mut self.on_progress {
            callback(self.samples.progress())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::sample::ReadSampleList;
    use crate::audio::{Cue, Samples};
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
        let mut cursor = Cursor::new(vec![]);
        WavWriter::new(samples.into_reader("test")).software_name("test").write_to(&mut cursor)?;
        let bytes = cursor.into_inner();
        assert_eq!(bytes, EXPECTED_WAV);
        Ok(())
    }

    #[test]
    fn test_write_wav_with_progress() -> Result<()> {
        let samples1 = Samples::<PcmS16Le>::from_pcm((0..2).collect::<Vec<_>>(), 2, 44100);
        let samples2 = Samples::<PcmS16Le>::from_pcm((2..4).collect::<Vec<_>>(), 2, 44100);
        let samples3 = Samples::<PcmS16Le>::from_pcm((4..6).collect::<Vec<_>>(), 2, 44100);
        let samples4 = Samples::<PcmS16Le>::from_pcm((6..8).collect::<Vec<_>>(), 2, 44100);
        let samples = ReadSampleList::new(vec![samples1, samples2, samples3, samples4], "test");

        let mut progress_updates = vec![];
        let mut cursor = Cursor::new(vec![]);
        WavWriter::new(samples)
            .software_name("test")
            .on_progress(|p| progress_updates.push(p))
            .write_to(&mut cursor)?;

        let bytes = cursor.into_inner();
        assert_eq!(bytes, EXPECTED_WAV);

        assert_eq!(progress_updates.len(), 5);
        for (i, progress) in progress_updates.into_iter().enumerate() {
            let progress = progress.expect("unknown progress");
            assert_eq!(progress.current, i as u64);
            assert_eq!(progress.total.get(), 4);
        }
        Ok(())
    }

    #[test]
    fn test_write_and_read_cues() -> Result<()> {
        let samples = Samples::<PcmS16Le>::from_pcm((0..8).collect::<Vec<_>>(), 2, 44100);
        let cues =
            vec![Cue::new("start", 0), Cue::with_duration("range", 1, 2), Cue::new("end", 3)];

        let samples = ReadSampleList::with_cues(vec![samples], cues.clone(), "test");
        let mut cursor = Cursor::new(vec![]);
        WavWriter::new(samples).write_to(&mut cursor)?;

        cursor.seek(SeekFrom::Start(0))?;
        let wav = WavReader::new(cursor, "test")?;
        let actual = wav.cues().collect::<Vec<_>>();
        assert_eq!(actual, cues);
        Ok(())
    }
}
