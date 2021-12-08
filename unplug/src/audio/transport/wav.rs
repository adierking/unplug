mod reader;
mod writer;

pub use reader::*;
pub use writer::*;

use crate::common::{ReadFrom, WriteTo};
use byteorder::{ReadBytesExt, WriteBytesExt, LE};
use std::ffi::CString;
use std::io::{self, Read, Write};

const fn fourcc(s: &[u8]) -> u32 {
    (s[0] as u32) | ((s[1] as u32) << 8) | ((s[2] as u32) << 16) | ((s[3] as u32) << 24)
}

const ID_ADTL: u32 = fourcc(b"adtl");
const ID_CUE: u32 = fourcc(b"cue ");
const ID_DATA: u32 = fourcc(b"data");
const ID_FMT: u32 = fourcc(b"fmt ");
const ID_INFO: u32 = fourcc(b"INFO");
const ID_ISFT: u32 = fourcc(b"ISFT");
const ID_LABL: u32 = fourcc(b"labl");
const ID_LIST: u32 = fourcc(b"LIST");
const ID_LTXT: u32 = fourcc(b"ltxt");
const ID_RGN: u32 = fourcc(b"rgn ");
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

/// WAVE `cue ` chunk data.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct CueChunk {
    points: Vec<CuePoint>,
}

impl<R: Read + ?Sized> ReadFrom<R> for CueChunk {
    type Error = io::Error;
    fn read_from(reader: &mut R) -> io::Result<Self> {
        let count = reader.read_u32::<LE>()?;
        let mut points = Vec::with_capacity(count as usize);
        for _ in 0..count {
            points.push(CuePoint::read_from(reader)?);
        }
        Ok(Self { points })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for CueChunk {
    type Error = io::Error;
    fn write_to(&self, writer: &mut W) -> io::Result<()> {
        writer.write_u32::<LE>(self.points.len() as u32)?;
        CuePoint::write_all_to(writer, &self.points)?;
        Ok(())
    }
}

/// WAVE cue point.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
struct CuePoint {
    name: u32,
    position: u32,
    chunk_id: u32,
    chunk_start: u32,
    block_start: u32,
    sample_offset: u32,
}

impl<R: Read + ?Sized> ReadFrom<R> for CuePoint {
    type Error = io::Error;
    fn read_from(reader: &mut R) -> io::Result<Self> {
        Ok(Self {
            name: reader.read_u32::<LE>()?,
            position: reader.read_u32::<LE>()?,
            chunk_id: reader.read_u32::<LE>()?,
            chunk_start: reader.read_u32::<LE>()?,
            block_start: reader.read_u32::<LE>()?,
            sample_offset: reader.read_u32::<LE>()?,
        })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for CuePoint {
    type Error = io::Error;
    fn write_to(&self, writer: &mut W) -> io::Result<()> {
        writer.write_u32::<LE>(self.name)?;
        writer.write_u32::<LE>(self.position)?;
        writer.write_u32::<LE>(self.chunk_id)?;
        writer.write_u32::<LE>(self.chunk_start)?;
        writer.write_u32::<LE>(self.block_start)?;
        writer.write_u32::<LE>(self.sample_offset)?;
        Ok(())
    }
}

/// WAVE `labl` chunk data.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct LabelChunk {
    name: u32,
    text: CString,
}

impl<R: Read + ?Sized> ReadFrom<R> for LabelChunk {
    type Error = io::Error;
    fn read_from(reader: &mut R) -> io::Result<Self> {
        Ok(Self { name: reader.read_u32::<LE>()?, text: CString::read_from(reader)? })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for LabelChunk {
    type Error = io::Error;
    fn write_to(&self, writer: &mut W) -> io::Result<()> {
        writer.write_u32::<LE>(self.name)?;
        self.text.write_to(writer)?;
        Ok(())
    }
}

/// WAVE `ltxt` chunk data.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct LabelTextChunk {
    name: u32,
    sample_length: u32,
    purpose: u32,
    country: u16,
    language: u16,
    dialect: u16,
    code_page: u16,
    text: CString,
}

impl<R: Read + ?Sized> ReadFrom<R> for LabelTextChunk {
    type Error = io::Error;
    fn read_from(reader: &mut R) -> io::Result<Self> {
        Ok(Self {
            name: reader.read_u32::<LE>()?,
            sample_length: reader.read_u32::<LE>()?,
            purpose: reader.read_u32::<LE>()?,
            country: reader.read_u16::<LE>()?,
            language: reader.read_u16::<LE>()?,
            dialect: reader.read_u16::<LE>()?,
            code_page: reader.read_u16::<LE>()?,
            // HACK: it seems that CueListTool doesn't write *anything* here...
            text: match CString::read_from(reader) {
                Ok(s) => s,
                Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => CString::default(),
                Err(e) => return Err(e),
            },
        })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for LabelTextChunk {
    type Error = io::Error;
    fn write_to(&self, writer: &mut W) -> io::Result<()> {
        writer.write_u32::<LE>(self.name)?;
        writer.write_u32::<LE>(self.sample_length)?;
        writer.write_u32::<LE>(self.purpose)?;
        writer.write_u16::<LE>(self.country)?;
        writer.write_u16::<LE>(self.language)?;
        writer.write_u16::<LE>(self.dialect)?;
        writer.write_u16::<LE>(self.code_page)?;
        self.text.write_to(writer)?;
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

    #[test]
    fn test_write_and_read_cue_chunk() {
        assert_write_and_read!(CueChunk {
            points: vec![
                CuePoint {
                    name: 1,
                    position: 2,
                    chunk_id: 3,
                    chunk_start: 4,
                    block_start: 5,
                    sample_offset: 6,
                },
                CuePoint {
                    name: 7,
                    position: 8,
                    chunk_id: 9,
                    chunk_start: 10,
                    block_start: 11,
                    sample_offset: 12,
                },
            ]
        });
    }

    #[test]
    fn test_write_and_read_label_chunk() {
        assert_write_and_read!(LabelChunk { name: 1, text: CString::new("fumo").unwrap() });
    }

    #[test]
    fn test_write_and_read_ltxt_chunk() {
        assert_write_and_read!(LabelTextChunk {
            name: 1,
            sample_length: 2,
            purpose: 3,
            country: 4,
            language: 5,
            dialect: 6,
            code_page: 7,
            text: CString::new("fumo").unwrap(),
        });
    }
}
