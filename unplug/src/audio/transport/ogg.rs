use crate::audio::format::{PcmS16Le, StaticFormat};
use crate::audio::{Format, ProgressHint, ReadSamples, Result, Samples, SourceTag};
use crate::common::ReadSeek;
use lewton::inside_ogg::OggStreamReader;
use std::io::{Read, Seek};
use tracing::{debug, instrument};

/// Reads audio samples from Ogg Vorbis data.
pub struct OggReader<'r> {
    reader: OggStreamReader<Box<dyn ReadSeek + 'r>>,
    tag: SourceTag,
}

impl<'r> OggReader<'r> {
    /// Creates a new `OggReader` which reads Ogg Vorbis data from `reader`. `tag` is a string or
    /// tag to identify the stream for debugging purposes.
    pub fn new(reader: (impl Read + Seek + 'r), tag: impl Into<SourceTag>) -> Result<Self> {
        Self::new_impl(Box::from(reader), tag.into())
    }

    #[instrument(level = "trace", skip_all)]
    fn new_impl(reader: Box<dyn ReadSeek + 'r>, tag: SourceTag) -> Result<Self> {
        let reader = Self { reader: OggStreamReader::new(reader)?, tag };
        debug!(
            "Opened Ogg Vorbis stream {:?}: {} Hz, {} channels",
            reader.tag(),
            reader.sample_rate(),
            reader.channels()
        );
        Ok(reader)
    }

    /// Gets the number of channels in the stream.
    pub fn channels(&self) -> usize {
        self.reader.ident_hdr.audio_channels as usize
    }

    /// Gets the audio sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.reader.ident_hdr.audio_sample_rate
    }
}

impl ReadSamples<'static> for OggReader<'_> {
    type Format = PcmS16Le;

    #[instrument(level = "trace", name = "OggReader", skip_all)]
    fn read_samples(&mut self) -> Result<Option<Samples<'static, Self::Format>>> {
        loop {
            match self.reader.read_dec_packet_itl()? {
                Some(packet) => {
                    if !packet.is_empty() {
                        return Ok(Some(Samples::from_pcm(
                            packet,
                            self.channels(),
                            self.sample_rate(),
                        )));
                    }
                }
                None => return Ok(None),
            }
        }
    }

    fn format(&self) -> Format {
        Self::Format::FORMAT
    }

    fn tag(&self) -> &SourceTag {
        &self.tag
    }

    fn progress_hint(&self) -> Option<ProgressHint> {
        // There doesn't seem to be a way to get this
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::transport::WavReader;
    use crate::test::{assert_samples_close, TEST_OGG, TEST_OGG_WAV};
    use std::io::Cursor;

    #[test]
    fn test_read_ogg() -> Result<()> {
        let mut ogg = OggReader::new(Cursor::new(TEST_OGG), "TEST_OGG")?;
        assert_eq!(ogg.sample_rate(), 44100);
        assert_eq!(ogg.channels(), 2);
        let samples = ogg.read_all_samples()?;
        let reference =
            WavReader::new(Cursor::new(TEST_OGG_WAV), "TEST_OGG_WAV")?.read_all_samples()?;
        // Compare with a tolerance of +/- 10 (lewton vs Audacity)
        assert_samples_close(&samples, &reference, 10);
        Ok(())
    }
}
