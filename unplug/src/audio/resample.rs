use super::format::pcm::{AnyPcm, ConvertPcm, PcmF32Le};
use super::format::Convert;
use super::{Error, ProgressHint, ReadSamples, Result, Samples};
use libsamplerate_sys::*;
use std::convert::TryInto;
use std::ffi::CStr;
use std::marker::PhantomData;
use std::mem;
use std::os::raw::{c_double, c_int, c_long};
use std::ptr;
use tracing::{instrument, trace, trace_span};

/// The maximum number of output frames to allocate.
const OUT_BUFFER_FRAMES: usize = 0x1000;

/// Converts a libsamplerate error code into an `Error` value.
#[allow(clippy::useless_conversion)]
fn make_error(code: c_int) -> Error {
    let description_ptr = unsafe { src_strerror(code) };
    let description = if !description_ptr.is_null() {
        unsafe { CStr::from_ptr(description_ptr).to_string_lossy().into_owned() }
    } else {
        String::new()
    };
    Error::ResampleInternal(code.into(), description)
}

/// Wraps a stream of PCM samples and resamples them at a different sample rate.
pub struct Resample<'r, 's, F: AnyPcm> {
    /// The inner stream to read samples from.
    inner: ConvertPcm<'r, 's, PcmF32Le>,
    /// The number of channels to resample.
    channels: usize,
    /// The rate of the last packet of samples that was converted.
    rate_in: u32,
    /// The rate to resample to.
    rate_out: u32,
    /// The buffer to hold input samples that libsamplerate could not process.
    buffer: Vec<f32>,
    /// True if the inner stream is at the end.
    eof: bool,
    /// The libsamplerate state. Null if conversion is either uninitialized or finished.
    state: *mut SRC_STATE,
    _marker: PhantomData<F>,
}

impl<'r, 's, F: AnyPcm> Resample<'r, 's, F> {
    /// Creates a new `Resample` which reads samples from `inner` and resamples them to `rate`.
    pub fn new(inner: impl ReadSamples<'s, Format = F> + 'r, rate: u32) -> Self {
        Self::new_impl(ConvertPcm::new(inner), rate)
    }

    fn new_impl(inner: ConvertPcm<'r, 's, PcmF32Le>, rate: u32) -> Self {
        Self {
            inner,
            channels: 0,
            rate_in: rate,
            rate_out: rate,
            buffer: vec![],
            eof: false,
            state: ptr::null_mut(),
            _marker: PhantomData,
        }
    }

    /// Initializes the libsamplerate state for an audio stream with `channels` channels.
    fn init_state(&mut self, channels: usize) -> Result<()> {
        assert!(self.state.is_null());
        let mut error = 0;
        self.state =
            unsafe { src_new(SRC_SINC_BEST_QUALITY as c_int, channels as c_int, &mut error) };
        if self.state.is_null() {
            return Err(make_error(error));
        }
        self.channels = channels;
        Ok(())
    }

    /// Destroys the libsamplerate state if it has been initialized.
    fn destroy_state(&mut self) {
        if !self.state.is_null() {
            unsafe { src_delete(self.state) };
            self.state = ptr::null_mut();
        }
    }

    /// Resamples `samples` to the target rate if it is not `None`, otherwise completes resampling
    /// and returns any additional samples.
    #[instrument(level = "trace", skip_all)]
    fn resample(
        &mut self,
        samples: Option<Samples<'s, PcmF32Le>>,
    ) -> Result<Samples<'s, PcmF32Le>> {
        let (samples, end_of_input) = match samples {
            Some(s) => (s, false),
            None => {
                // Process whatever's left in the buffer
                let data = mem::take(&mut self.buffer);
                (Samples::<PcmF32Le>::from_pcm(data, self.channels, self.rate_in), true)
            }
        };

        if self.state.is_null() {
            assert!(!self.eof);
            self.init_state(samples.channels)?;
        } else if samples.channels != self.channels {
            return Err(Error::InconsistentChannels);
        }

        let ratio = (self.rate_out as c_double) / (samples.rate as c_double);
        if unsafe { src_is_valid_ratio(ratio) } == 0 {
            return Err(Error::UnsupportedRateConversion(samples.rate, self.rate_out));
        }
        self.rate_in = samples.rate;

        // If our buffer is empty, we can just reference the sample data directly, otherwise we
        // have to copy it in
        let data_in = if self.buffer.is_empty() {
            &samples.data[..samples.len]
        } else {
            self.buffer.extend(&samples.data[..samples.len]);
            &self.buffer
        };

        let input_frames = data_in.len() / self.channels;
        let mut data_out = Vec::with_capacity(OUT_BUFFER_FRAMES * self.channels);
        let mut data = SRC_DATA {
            data_in: data_in.as_ptr(),
            data_out: data_out.as_mut_ptr(),
            input_frames: input_frames.try_into().unwrap(),
            output_frames: OUT_BUFFER_FRAMES as c_long,
            end_of_input: end_of_input.into(),
            src_ratio: ratio,
            ..Default::default()
        };
        let error =
            trace_span!("src_process").in_scope(|| unsafe { src_process(self.state, &mut data) });
        if error != 0 {
            return Err(make_error(error));
        }
        trace!("src_process() -> {:?}", data);

        let frames_consumed = data.input_frames_used as usize;
        let frames_produced = data.output_frames_gen as usize;
        let samples_consumed = frames_consumed * self.channels;
        let samples_produced = frames_produced * self.channels;
        assert!(samples_consumed <= data_in.len());
        assert!(samples_produced <= data_out.capacity());

        // Put any unprocessed samples into the buffer to be used next time
        self.buffer = data_in[samples_consumed..].into();

        // Keep going until libsamplerate doesn't return any more samples. Even with end_of_input
        // set it can continue to return samples for several more calls. This follows what the
        // timewarp-file.c libsamplerate example does.
        if end_of_input && frames_produced == 0 {
            self.destroy_state();
        }

        unsafe { data_out.set_len(samples_produced) };
        Ok(Samples::from_pcm(data_out, self.channels, self.rate_out))
    }

    fn read_samples_f32(&mut self) -> Result<Option<Samples<'s, PcmF32Le>>> {
        if self.rate_out == 0 {
            return Err(Error::InvalidSampleRate(self.rate_out));
        }
        let mut resampled = Samples::from_pcm(vec![], 0, 0);
        while resampled.len == 0 && !(self.eof && self.state.is_null()) {
            if self.eof {
                // We already know the inner stream reached the end
                resampled = self.resample(None)?;
            } else {
                let samples = self.inner.read_samples()?;
                self.eof = samples.is_none();
                resampled = self.resample(samples)?;
            }
        }
        Ok(if resampled.len > 0 { Some(resampled) } else { None })
    }
}

impl<'r, 's, F: AnyPcm> Drop for Resample<'r, 's, F> {
    fn drop(&mut self) {
        self.destroy_state();
    }
}

impl<'r, 's, F: AnyPcm> ReadSamples<'s> for Resample<'r, 's, F>
where
    PcmF32Le: Convert<F>,
{
    type Format = F;

    fn read_samples(&mut self) -> Result<Option<Samples<'s, Self::Format>>> {
        match self.read_samples_f32()? {
            Some(s) => Ok(Some(s.convert()?)),
            None => Ok(None),
        }
    }

    fn format(&self) -> super::Format {
        self.inner.format()
    }

    fn tag(&self) -> &crate::audio::SourceTag {
        self.inner.tag()
    }

    fn progress(&self) -> Option<ProgressHint> {
        // Ideally we could know how many blocks we plan to output, but variable sample rate streams
        // make this difficult, so we have to fall back on the progress of the inner stream
        self.inner.progress()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::format::PcmS16Le;
    use crate::audio::transport::FlacReader;
    use crate::test::{assert_samples_close, open_test_wav, TEST_FLAC};
    use std::io::Cursor;

    #[test]
    fn test_resample_flac_matches_wav() -> Result<()> {
        let flac = FlacReader::new(Cursor::new(TEST_FLAC), "test")?;
        let wav = Samples::<PcmS16Le>::from_pcm(open_test_wav(), 2, 44100).into_reader("test");
        let flac_resampler = Resample::new(flac, 48000);
        let wav_resampler = Resample::new(wav, 48000);
        let flac_resampled = flac_resampler.convert::<PcmF32Le>().read_all_samples()?;
        let wav_resampled = wav_resampler.convert::<PcmF32Le>().read_all_samples()?;
        assert_eq!(flac_resampled.rate, 48000);
        assert_eq!(wav_resampled.rate, 48000);
        assert_samples_close(&flac_resampled, &wav_resampled, 0.001);
        Ok(())
    }

    #[test]
    fn test_upsample_and_downsample() -> Result<()> {
        let initial = Samples::<PcmS16Le>::from_pcm(open_test_wav(), 2, 44100);
        let upsampler = Resample::new(initial.clone().into_reader("test"), 48000);
        let mut downsampler = Resample::new(upsampler, 44100);
        let mut resampled = downsampler.read_all_samples()?;
        // HACK: libsamplerate may output an extra frame as intended behavior. Discard it.
        if resampled.len >= initial.len && resampled.len <= initial.len + 2 {
            resampled.len = initial.len;
        }
        assert_samples_close(&resampled, &initial, 10);
        Ok(())
    }
}
