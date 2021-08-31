pub mod hps;

use crate::common::ReadFrom;
use byteorder::{ReadBytesExt, BE};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::convert::TryFrom;
use std::io::{self, Read};
use thiserror::Error;

/// The result type for audio operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for audio operations.
#[derive(Error, Debug)]
#[non_exhaustive]
#[allow(variant_size_differences)]
pub enum Error {
    #[error("unrecognized sample format: {0}")]
    UnrecognizedSampleFormat(u16),

    #[error("invalid HPS magic")]
    InvalidHpsMagic,

    #[error("invalid channel count: {0}")]
    InvalidChannelCount(u32),

    #[error(transparent)]
    Io(Box<io::Error>),
}

from_error_boxed!(Error::Io, io::Error);

/// GameCube audio sample formats.
#[derive(Debug, Copy, Clone, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
#[repr(u16)]
pub enum SampleFormat {
    /// GameCube ADPCM
    Adpcm = 0,
    /// 16-bit PCM
    Pcm16 = 10,
    /// 8-bit PCM
    Pcm8 = 25,
}

impl SampleFormat {
    /// Calculates the size (in bytes) of audio data which ends at `end_address`.
    pub fn calculate_size(&self, end_address: u32) -> u32 {
        match *self {
            Self::Adpcm => end_address / 2 + 1,
            Self::Pcm16 => (end_address + 1) * 2,
            Self::Pcm8 => end_address + 1,
        }
    }
}

impl Default for SampleFormat {
    fn default() -> Self {
        Self::Adpcm
    }
}

impl<R: Read> ReadFrom<R> for SampleFormat {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let id = reader.read_u16::<BE>()?;
        match Self::try_from(id) {
            Ok(format) => Ok(format),
            Err(_) => Err(Error::UnrecognizedSampleFormat(id)),
        }
    }
}
