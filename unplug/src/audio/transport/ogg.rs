use crate::audio::format::PcmS16Le;
use crate::audio::{ReadSamples, Result, Samples};
use crate::common::ReadSeek;
use lewton::inside_ogg::OggStreamReader;
use log::debug;
use std::io::{Read, Seek};

/// Reads audio samples from Ogg Vorbis data.
pub struct OggReader<'r> {
    reader: OggStreamReader<Box<dyn ReadSeek + 'r>>,
}

impl<'r> OggReader<'r> {
    /// Creates a new `OggReader` which reads Ogg Vorbis data from `reader`.
    pub fn new(reader: (impl Read + Seek + 'r)) -> Result<Self> {
        Self::new_impl(Box::from(reader))
    }

    /// Gets the number of channels in the stream.
    pub fn channels(&self) -> usize {
        self.reader.ident_hdr.audio_channels as usize
    }

    /// Gets the audio sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.reader.ident_hdr.audio_sample_rate
    }

    fn new_impl(reader: Box<dyn ReadSeek + 'r>) -> Result<Self> {
        let reader = Self { reader: OggStreamReader::new(reader)? };
        debug!(
            "Opened Ogg Vorbis stream: {} Hz, {} channels",
            reader.sample_rate(),
            reader.channels()
        );
        Ok(reader)
    }
}

impl ReadSamples<'static> for OggReader<'_> {
    type Format = PcmS16Le;

    fn read_samples(&mut self) -> Result<Option<Samples<'static, Self::Format>>> {
        loop {
            match self.reader.read_dec_packet_itl()? {
                Some(packet) => {
                    if !packet.is_empty() {
                        return Ok(Some(Samples::from_pcm(packet, self.channels())));
                    }
                }
                None => return Ok(None),
            }
        }
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
        let mut ogg = OggReader::new(Cursor::new(TEST_OGG))?;
        assert_eq!(ogg.sample_rate(), 44100);
        assert_eq!(ogg.channels(), 2);
        let samples = ogg.read_all_samples()?;
        let reference = WavReader::open(Cursor::new(TEST_OGG_WAV))?.read_all_samples()?;
        // Compare with a tolerance of +/- 10 (lewton vs Audacity)
        assert_samples_close(&samples, &reference, 10);
        Ok(())
    }
}
