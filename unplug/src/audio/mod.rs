pub mod format;
pub mod metadata;
pub mod sample;
pub mod transport;

pub use format::{Format, FormatTag};
pub use sample::{ReadSamples, Samples};

use std::io;
use thiserror::Error;

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

    #[error("invalid WAV data")]
    InvalidWav,

    #[error("samples are not aligned on a frame boundary")]
    NotFrameAligned,

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

    #[error(transparent)]
    Flac(Box<claxon::Error>),

    #[error(transparent)]
    Io(Box<io::Error>),

    #[error(transparent)]
    Mp3(Box<minimp3::Error>),
}

from_error_boxed!(Error::Flac, claxon::Error);
from_error_boxed!(Error::Io, io::Error);
from_error_boxed!(Error::Mp3, minimp3::Error);
