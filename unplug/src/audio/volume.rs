use super::format::pcm::Scalable;
use super::format::{PcmFormat, StaticFormat};
use super::{Error, Result, SampleFilter};
use float_cmp::approx_eq;
use std::marker::PhantomData;

/// Trait for a format which allows the scaling of sample amplitudes.
pub trait ScaleAmplitude: StaticFormat {
    /// Scales the amplitude of `sample` by multiplying it by `factor`. For logarithmic volume
    /// control, `factor` should be computed from a relative volume level using `volume()`.
    fn scale_amplitude(sample: Self::Data, factor: f64) -> Self::Data;

    /// Helper method which scales the amplitudes of a slice of samples.
    fn scale_amplitudes(samples: &mut [Self::Data], factor: f64) {
        for sample in samples {
            *sample = Self::scale_amplitude(*sample, factor);
        }
    }
}

impl<F: PcmFormat> ScaleAmplitude for F
where
    F::Data: Scalable,
{
    fn scale_amplitude(sample: Self::Data, factor: f64) -> Self::Data {
        (sample.scale::<f64>() * factor).scale()
    }
}

/// Calculates the amplitude scale factor for the relative volume level `volume` (1.0 = no change).
/// A volume of `0.5` is a gain of -30dB.
pub fn factor(volume: f64) -> f64 {
    // This code inspired by <https://www.dr-lex.be/info-stuff/volumecontrols.html>
    const A: f64 = 0.001;
    const B: f64 = 6.907755278982137; // ln(1000)
    let factor = A * f64::exp(B * volume);
    if volume >= 0.1 {
        factor
    } else {
        // Linear backoff to achieve true 0 dB instead of 30 dB
        factor * volume * 10.0
    }
}

/// A filter which scales the volumes of samples.
pub struct Volume<F: ScaleAmplitude> {
    volume: f64,
    factor: Option<f64>,
    _marker: PhantomData<F>,
}

impl<F: ScaleAmplitude> Volume<F> {
    /// Creates a new `VolumeFilter` which scales sample volumes by `volume` (1.0 = no change).
    pub fn new(volume: f64) -> Self {
        Self { volume, factor: None, _marker: PhantomData }
    }

    /// Retrieves the filter's current volume scale.
    pub fn volume(&self) -> f64 {
        self.volume
    }

    /// Sets the filter's volume scale to `volume`.
    pub fn set_volume(&mut self, volume: f64) {
        if !approx_eq!(f64, self.volume, volume, ulps = 2) {
            self.factor = None;
        }
        self.volume = volume;
    }
}

impl<F: ScaleAmplitude> SampleFilter for Volume<F> {
    type Format = F;
    fn apply(&mut self, samples: &mut [F::Data], _channels: usize, len: usize) -> Result<()> {
        if self.factor.is_none() {
            let volume = self.volume;
            self.factor = if !volume.is_finite() || volume < 0.0 {
                return Err(Error::InvalidVolume(volume));
            } else if approx_eq!(f64, volume, 1.0, ulps = 2) {
                Some(1.0)
            } else if approx_eq!(f64, volume, 0.0, ulps = 2) {
                Some(0.0)
            } else {
                Some(factor(volume))
            };
        }
        F::scale_amplitudes(&mut samples[..len], self.factor.unwrap());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::format::{PcmF32Le, PcmS16Le};
    use crate::audio::transport::WavReader;
    use crate::audio::{ReadSamples, Samples};
    use crate::test::{assert_samples_close, open_test_wav, TEST_WAV_VOL};
    use float_cmp::assert_approx_eq;
    use std::io::Cursor;

    #[test]
    fn test_scale_amplitude() {
        assert_eq!(PcmS16Le::scale_amplitude(0, 2.0), 0);
        assert_eq!(PcmS16Le::scale_amplitude(64, 2.0), 128);
        assert_eq!(PcmS16Le::scale_amplitude(128, 0.5), 64);
        assert_eq!(PcmS16Le::scale_amplitude(16384, 2.0), 32767);
        assert_eq!(PcmS16Le::scale_amplitude(-16384, 2.0), -32768);
        assert_eq!(PcmS16Le::scale_amplitude(30000, 2.0), 32767);
        assert_eq!(PcmS16Le::scale_amplitude(-30000, 2.0), -32768);
        assert_approx_eq!(f32, PcmF32Le::scale_amplitude(0.5, 2.0), 1.0, epsilon = 0.0001);
        assert_approx_eq!(f32, PcmF32Le::scale_amplitude(1.0, 2.0), 2.0, epsilon = 0.0001);
        assert_approx_eq!(f32, PcmF32Le::scale_amplitude(1.0, 0.5), 0.5, epsilon = 0.0001);
    }

    #[test]
    fn test_volume_factor() {
        assert_approx_eq!(f64, factor(0.0), 0.0, epsilon = 0.0001);
        assert_approx_eq!(f64, factor(0.1), 0.002, epsilon = 0.0001);
        assert_approx_eq!(f64, factor(0.5), 0.0316, epsilon = 0.0001);
        assert_approx_eq!(f64, factor(0.9), 0.5012, epsilon = 0.0001);
        assert_approx_eq!(f64, factor(1.0), 1.0, epsilon = 0.0001);
    }

    #[test]
    fn test_volume() -> Result<()> {
        let original = Samples::from_pcm(open_test_wav(), 2, 44100);
        let expected = WavReader::new(Cursor::new(TEST_WAV_VOL), "test")?.read_all_samples()?;
        let actual = original.into_reader("test").filter(Volume::new(0.5)).read_all_samples()?;
        assert_samples_close(&actual, &expected, 1);
        Ok(())
    }
}
