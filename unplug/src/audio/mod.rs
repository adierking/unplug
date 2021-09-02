pub mod adpcm;
pub mod format;
pub mod hps;
pub mod sample;

mod wav;

pub use format::{Format, FormatTag, GcFormat};
pub use sample::{ReadSamples, Samples};
pub use wav::WavBuilder;

use std::io;
use thiserror::Error;

/// The result type for audio operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for audio operations.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("invalid channel count: {0}")]
    InvalidChannelCount(u32),

    #[error("invalid HPS magic")]
    InvalidHpsMagic,

    #[error("audio stream is not mono")]
    StreamNotMono,

    #[error("unrecognized sample format: {0}")]
    UnrecognizedSampleFormat(u16),

    #[error("unsupported stream format: {0:?}")]
    UnsupportedFormat(Format),

    #[error(transparent)]
    Io(Box<io::Error>),
}

from_error_boxed!(Error::Io, io::Error);
