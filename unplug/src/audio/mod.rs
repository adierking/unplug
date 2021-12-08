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

/// A marked section of samples in an audio stream.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cue {
    /// The index of the sample frame where the cue starts.
    pub start: u64,
    /// The cue's duration in sample frames. For simple markers, this can be zero.
    pub duration: u64,
    /// The cue's name.
    pub name: Arc<str>,
}

impl Cue {
    /// Creates a new `Cue` which does not have a duration.
    pub fn new(name: impl Into<Arc<str>>, start: u64) -> Self {
        Self::with_duration(name, start, 0)
    }

    /// Creates a new `Cue` with a duration.
    pub fn with_duration(name: impl Into<Arc<str>>, start: u64, duration: u64) -> Self {
        Self { name: name.into(), start, duration }
    }
}

impl Default for Cue {
    fn default() -> Self {
        Self { start: 0, duration: 0, name: "".into() }
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
