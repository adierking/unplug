use crate::audio::format::{AnyFormat, Cast, PcmFormat, PcmS16Le, PcmS24Le, PcmS32Le, PcmS8};
use crate::audio::{Error, Format, ReadSamples, Result, Samples};
use claxon::{self};
use log::debug;
use std::convert::TryFrom;
use std::io::Read;
use std::mem;

/// Reads audio samples from FLAC data.
pub struct FlacReader<'r> {
    /// The inner FLAC stream.
    flac: claxon::FlacReader<Box<dyn Read + 'r>>,
    /// The buffer to store decoded samples in.
    buffer: Vec<i32>,
    /// The corresponding PCM format.
    format: Format,
    /// The number of channels in the stream.
    channels: usize,
    /// The audio sample rate.
    sample_rate: u32,
}

impl<'r> FlacReader<'r> {
    /// Creates a new `FlacReader` which reads FLAC data from `reader`.
    pub fn new(reader: impl Read + 'r) -> Result<Self> {
        Self::new_impl(Box::from(reader))
    }

    fn new_impl(reader: Box<dyn Read + 'r>) -> Result<Self> {
        let flac = claxon::FlacReader::new(reader)?;
        let info = flac.streaminfo();
        let channels = info.channels as usize;
        let sample_rate = info.sample_rate;
        let format = match info.bits_per_sample {
            // read_samples() must match this!
            8 => Format::PcmS8,
            16 => Format::PcmS16Le,
            24 => Format::PcmS24Le,
            32 => Format::PcmS32Le,
            other => return Err(Error::UnsupportedBitDepth(other)),
        };
        let buffer_size = info.max_block_size as usize * channels;
        debug!(
            "Opened FLAC stream: {} Hz, {}-bit, {} channel(s)",
            sample_rate, info.bits_per_sample, channels
        );
        Ok(Self { flac, buffer: vec![0; buffer_size], format, channels, sample_rate })
    }

    /// Gets the format of the sample data that will be read. This will always be a PCM format.
    pub fn format(&self) -> Format {
        self.format
    }

    /// Gets the number of channels in the stream.
    pub fn channels(&self) -> usize {
        self.channels
    }

    /// Gets the audio sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Converts and interleaves the samples across audio channels.
    fn build_samples<F>(num_samples: usize, channels: Vec<&[i32]>) -> Samples<'static, AnyFormat>
    where
        F: PcmFormat + Cast<AnyFormat>,
        F::Data: TryFrom<i32>,
    {
        let mut samples = Vec::with_capacity(num_samples);
        for i in 0..(num_samples / channels.len()) {
            for channel in &channels {
                samples.push(F::Data::try_from(channel[i]).ok().expect("bad sample size"));
            }
        }
        Samples::<F>::from_pcm(samples, channels.len()).cast()
    }
}

impl ReadSamples<'static> for FlacReader<'_> {
    type Format = AnyFormat;

    fn read_samples(&mut self) -> Result<Option<Samples<'static, Self::Format>>> {
        // Calling blocks() here will pick up where the last call left off
        let mut reader = self.flac.blocks();
        let block = match reader.read_next_or_eof(mem::take(&mut self.buffer))? {
            Some(b) => b,
            None => return Ok(None),
        };

        // The channels are all separate chunks of data which need to be interleaved
        let num_samples = block.len() as usize;
        let channels = (0..self.channels).map(|c| block.channel(c as u32)).collect::<Vec<_>>();
        let samples = match self.format {
            Format::PcmS8 => Self::build_samples::<PcmS8>(num_samples, channels),
            Format::PcmS16Le => Self::build_samples::<PcmS16Le>(num_samples, channels),
            Format::PcmS24Le => Self::build_samples::<PcmS24Le>(num_samples, channels),
            Format::PcmS32Le => Self::build_samples::<PcmS32Le>(num_samples, channels),
            other => panic!("unhandled format: {:?}", other),
        };
        self.buffer = block.into_buffer();
        Ok(Some(samples))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::{open_test_wav, TEST_FLAC};

    #[test]
    fn test_read_flac() -> Result<()> {
        let flac = FlacReader::new(TEST_FLAC)?;
        assert_eq!(flac.format(), Format::PcmS16Le);
        assert_eq!(flac.channels(), 2);
        assert_eq!(flac.sample_rate(), 44100);
        let mut converter = flac.convert::<PcmS16Le>();
        let samples = converter.read_all_samples()?;
        assert_eq!(samples.data, open_test_wav());
        Ok(())
    }
}
