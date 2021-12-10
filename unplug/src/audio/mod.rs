pub mod format;
pub mod metadata;
pub mod resample;
pub mod sample;
pub mod transport;

pub use format::{Format, FormatTag};
pub use sample::{ReadSamples, Samples, SourceChannel, SourceTag};

use lewton::VorbisError;
use std::io;
use std::num::NonZeroU64;
use std::sync::Arc;
use thiserror::Error;

/// Represents the current progress of an audio processing operation. Units are arbitrary and this
/// should only be used for reporting progress back to the user.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ProgressHint {
    /// The amount of work completed so far.
    pub current: u64,
    /// The total amount of work to complete.
    pub total: NonZeroU64,
}

impl ProgressHint {
    /// Creates an `Option<ProgressHint>` from `current` and `total`. If `total` is 0, this will
    /// return `None` to prevent potential divide-by-zero errors.
    pub fn new(current: u64, total: u64) -> Option<ProgressHint> {
        NonZeroU64::new(total).map(|total| ProgressHint { current, total })
    }
}

/// A marked point or range in an audio stream.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cue {
    /// The index of the sample frame where the cue starts.
    pub start: u64,
    /// The cue's type and type-specific data.
    pub kind: CueKind,
    /// The cue's name.
    pub name: Arc<str>,
}

/// Describes how a cue is used.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CueKind {
    /// The cue is a single point in the audio stream.
    Point,
    /// The cue indicates the start of the looped section.
    Loop,
    /// The cue covers a range of samples.
    Range(NonZeroU64),
}

impl Cue {
    /// Creates a new `Cue` which is a simple point.
    pub fn new(name: impl Into<Arc<str>>, start: u64) -> Self {
        Self { start, kind: CueKind::Point, name: name.into() }
    }

    /// Creates a new `Cue` which defines a loop point.
    pub fn new_loop(name: impl Into<Arc<str>>, start: u64) -> Self {
        Self { start, kind: CueKind::Loop, name: name.into() }
    }

    /// Creates a new range `Cue` with a duration. ***Panics*** if the duration is zero.
    pub fn new_range(name: impl Into<Arc<str>>, start: u64, duration: u64) -> Self {
        let duration = NonZeroU64::new(duration).expect("zero-duration range cue");
        Self { start, kind: CueKind::Range(duration), name: name.into() }
    }

    /// Returns the duration of the cue. This will be 0 for non-range cues.
    pub fn duration(&self) -> u64 {
        match self.kind {
            CueKind::Point | CueKind::Loop => 0,
            CueKind::Range(duration) => duration.get(),
        }
    }

    /// Returns true if this cue is a simple point. This does not include loop points.
    pub fn is_simple(&self) -> bool {
        matches!(self.kind, CueKind::Point)
    }

    /// Returns true if this cue is a loop point.
    pub fn is_loop(&self) -> bool {
        matches!(self.kind, CueKind::Loop)
    }

    /// Returns true if this cue is a range.
    pub fn is_range(&self) -> bool {
        matches!(self.kind, CueKind::Range(_))
    }
}

impl Default for Cue {
    fn default() -> Self {
        Self { start: 0, kind: CueKind::Point, name: "".into() }
    }
}

/// The result type for audio operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for audio operations.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("sample block too large: {0:#x} > {1:#x}")]
    BlockTooLarge(usize, usize),

    #[error("channels have different sizes")]
    DifferentChannelSizes,

    #[error("cannot concatenate samples with different coefficients")]
    DifferentCoefficients,

    #[error("audio stream is empty")]
    EmptyStream,

    #[error("audio stream does not have a consistent channel count")]
    InconsistentChannels,

    #[error("audio stream does not have a consistent sample rate")]
    InconsistentSampleRate,

    #[error("invalid BRSAR data")]
    InvalidBrsar,

    #[error("invalid channel count: {0}")]
    InvalidChannelCount(u32),

    #[error("invalid HPS magic")]
    InvalidHpsMagic,

    #[error("invalid RIFF data")]
    InvalidRiff,

    #[error("invalid RWAV data")]
    InvalidRwav,

    #[error("invalid sample rate: {0}")]
    InvalidSampleRate(u32),

    #[error("invalid WAV data")]
    InvalidWav,

    #[error("samples are not aligned on a frame boundary")]
    NotFrameAligned,

    #[error("libsamplerate error {0}: {1}")]
    ResampleInternal(i32, String),

    #[error("audio stream is not mono")]
    StreamNotMono,

    #[error("audio stream is not stereo")]
    StreamNotStereo,

    #[error("unrecognized event command: {0}")]
    UnrecognizedEventCommand(u8),

    #[error("unrecognized sample format: {0}")]
    UnrecognizedSampleFormat(u16),

    #[error("unsupported bit depth: {0}")]
    UnsupportedBitDepth(u32),

    #[error("audio stream is not mono or stereo")]
    UnsupportedChannels,

    #[error("unsupported stream format: {0:?}")]
    UnsupportedFormat(Format),

    #[error("unsupported sample rate conversion: {0} -> {1}")]
    UnsupportedRateConversion(u32, u32),

    #[error(transparent)]
    Flac(Box<claxon::Error>),

    #[error(transparent)]
    Io(Box<io::Error>),

    #[error(transparent)]
    Mp3(Box<minimp3::Error>),

    #[error(transparent)]
    Vorbis(Box<VorbisError>),
}

from_error_boxed!(Error::Flac, claxon::Error);
from_error_boxed!(Error::Io, io::Error);
from_error_boxed!(Error::Mp3, minimp3::Error);
from_error_boxed!(Error::Vorbis, VorbisError);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cue_ordering() {
        // Cues MUST be ordered by start, then kind, then name
        let mut cues = vec![
            Cue::new("b", 1),
            Cue::new("a", 0),
            Cue::new_loop("e", 1),
            Cue::new_range("f", 1, 1),
            Cue::new_range("d", 1, 2),
            Cue::new("c", 1),
            Cue::new("g", 2),
        ];
        cues.sort_unstable();
        assert_eq!(
            cues,
            &[
                Cue::new("a", 0),
                Cue::new("b", 1),
                Cue::new("c", 1),
                Cue::new_loop("e", 1),
                Cue::new_range("f", 1, 1),
                Cue::new_range("d", 1, 2),
                Cue::new("g", 2),
            ]
        );
    }
}
