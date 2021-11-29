mod reader;
mod writer;

pub use reader::*;
pub use writer::*;

use crate::common::{ReadFrom, WriteTo};
use byteorder::{ReadBytesExt, WriteBytesExt, LE};
use std::io::{self, Read, Write};

const fn fourcc(s: &[u8]) -> u32 {
    (s[0] as u32) | ((s[1] as u32) << 8) | ((s[2] as u32) << 16) | ((s[3] as u32) << 24)
}

const ID_DATA: u32 = fourcc(b"data");
const ID_FMT: u32 = fourcc(b"fmt ");
const ID_INFO: u32 = fourcc(b"INFO");
const ID_ISFT: u32 = fourcc(b"ISFT");
const ID_LIST: u32 = fourcc(b"LIST");
const ID_RIFF: u32 = fourcc(b"RIFF");
const ID_WAVE: u32 = fourcc(b"WAVE");

const RIFF_ALIGN: u64 = 2;
const CHUNK_HEADER_SIZE: u64 = 8;

const WAVE_FORMAT_PCM: u16 = 0x1;

/// A RIFF chunk header.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
struct ChunkHeader {
    /// The FOURCC chunk type identifier.
    id: u32,
    /// The size of the chunk data, excluding this header.
    size: u32,
}

impl<R: Read + ?Sized> ReadFrom<R> for ChunkHeader {
    type Error = io::Error;
    fn read_from(reader: &mut R) -> io::Result<Self> {
        Ok(Self { id: reader.read_u32::<LE>()?, size: reader.read_u32::<LE>()? })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for ChunkHeader {
    type Error = io::Error;
    fn write_to(&self, writer: &mut W) -> io::Result<()> {
        writer.write_u32::<LE>(self.id)?;
        writer.write_u32::<LE>(self.size)?;
        Ok(())
    }
}

/// WAVE `fmt ` chunk data.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
struct FormatChunk {
    /// The WAVE format category.
    format_tag: u16,
    /// The number of channels stored in the file.
    channels: u16,
    /// The sample rate.
    samples_per_sec: u32,
    /// The average number of bytes per second that will be transferred.
    avg_bytes_per_sec: u32,
    /// The size of a complete sample, including data for all channels.
    block_align: u16,
    /// The number of bits per channel per sample.
    bits_per_sample: u16,
}

impl<R: Read + ?Sized> ReadFrom<R> for FormatChunk {
    type Error = io::Error;
    fn read_from(reader: &mut R) -> io::Result<Self> {
        Ok(Self {
            format_tag: reader.read_u16::<LE>()?,
            channels: reader.read_u16::<LE>()?,
            samples_per_sec: reader.read_u32::<LE>()?,
            avg_bytes_per_sec: reader.read_u32::<LE>()?,
            block_align: reader.read_u16::<LE>()?,
            bits_per_sample: reader.read_u16::<LE>()?,
        })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for FormatChunk {
    type Error = io::Error;
    fn write_to(&self, writer: &mut W) -> io::Result<()> {
        writer.write_u16::<LE>(self.format_tag)?;
        writer.write_u16::<LE>(self.channels)?;
        writer.write_u32::<LE>(self.samples_per_sec)?;
        writer.write_u32::<LE>(self.avg_bytes_per_sec)?;
        writer.write_u16::<LE>(self.block_align)?;
        writer.write_u16::<LE>(self.bits_per_sample)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_write_and_read;

    #[test]
    fn test_write_and_read_chunk_header() {
        assert_write_and_read!(ChunkHeader { id: ID_DATA, size: 1234 });
    }

    #[test]
    fn test_write_and_read_format_chunk() {
        assert_write_and_read!(FormatChunk {
            format_tag: 1,
            channels: 2,
            samples_per_sec: 3,
            avg_bytes_per_sec: 4,
            block_align: 5,
            bits_per_sample: 6,
        });
    }
}
