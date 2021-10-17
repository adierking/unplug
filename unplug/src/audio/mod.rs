pub mod adpcm;
pub mod brsar;
pub mod dsp;
pub mod format;
pub mod hps;
pub mod pcm;
pub mod rwav;
pub mod sample;
pub mod sem;
pub mod ssm;
pub mod wav;

pub use brsar::Brsar;
pub use format::{Format, FormatTag};
pub use hps::HpsStream;
pub use pcm::ConvertPcm;
pub use sample::{ReadSamples, Samples};
pub use sem::EventBank;
pub use ssm::SoundBank;
pub use wav::{WavBuilder, WavReader};

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

    #[error("no samples are available")]
    NoSamplesAvailable,

    #[error("audio stream is not mono")]
    StreamNotMono,

    #[error("audio stream is not stereo")]
    StreamNotStereo,

    #[error("unrecognized event command: {0}")]
    UnrecognizedEventCommand(u8),

    #[error("unrecognized sample format: {0}")]
    UnrecognizedSampleFormat(u16),

    #[error("audio stream is not mono or stereo")]
    UnsupportedChannels,

    #[error("unsupported stream format: {0:?}")]
    UnsupportedFormat(Format),

    #[error(transparent)]
    Io(Box<io::Error>),
}

from_error_boxed!(Error::Io, io::Error);
