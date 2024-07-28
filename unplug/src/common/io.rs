use std::cmp;
use std::io::{self, ErrorKind, Read, Seek, SeekFrom, Write};

pub const BUFFER_SIZE: usize = 0x8000;

/// Fills the next `len` bytes in `writer` with `byte`.
pub fn fill(mut writer: impl Write, byte: u8, len: u64) -> io::Result<()> {
    if len > 0 {
        io::copy(&mut io::repeat(byte).take(len), &mut writer)?;
    }
    Ok(())
}

/// Writes padding bytes so that a writer's offset is aligned to a power of two.
pub fn pad(mut writer: (impl Write + Seek), align: u64, fill: u8) -> io::Result<()> {
    let offset = writer.stream_position()?;
    let aligned = super::align(offset, align);
    let padding = aligned - offset;
    self::fill(writer, fill, padding)?;
    Ok(())
}

/// Implementation of `std::io::copy` which uses `buf` as the buffer. This can be much faster than
/// the built-in implementation.
pub fn copy_buffered(
    reader: &mut (impl Read + ?Sized),
    writer: &mut (impl Write + ?Sized),
    buf: &mut [u8],
) -> io::Result<u64> {
    let mut total = 0;
    loop {
        let num_read = match reader.read(buf) {
            Ok(0) => return Ok(total),
            Ok(len) => len,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        writer.write_all(&buf[..num_read])?;
        total += num_read as u64;
    }
}

/// Copies a region of bytes inside a stream to another position in the stream. The source and
/// destination regions are permitted to overlap.
pub fn copy_within(
    mut stream: (impl Read + Write + Seek),
    from_offset: u64,
    len: u64,
    to_offset: u64,
) -> io::Result<()> {
    if len == 0 || from_offset == to_offset {
        return Ok(());
    }
    let mut buf = [0u8; BUFFER_SIZE];
    let mut remaining = len;
    while remaining != 0 {
        let read_len = cmp::min(buf.len() as u64, remaining);
        let copy_offset = if to_offset > from_offset {
            // Moving forward - start at the end
            remaining - read_len
        } else {
            // Moving backward - start at the front
            len - remaining
        };
        stream.seek(SeekFrom::Start(from_offset + copy_offset))?;
        stream.read_exact(&mut buf[..(read_len as usize)])?;
        stream.seek(SeekFrom::Start(to_offset + copy_offset))?;
        stream.write_all(&buf[..(read_len as usize)])?;
        remaining -= read_len;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_copy_buffered() -> io::Result<()> {
        let mut bytes = vec![0u8; 0x100000];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = i as u8;
        }
        let mut writer = Cursor::new(Vec::with_capacity(bytes.len()));
        let mut reader = Cursor::new(bytes);
        let mut buf = [0u8; BUFFER_SIZE];
        copy_buffered(&mut reader, &mut writer, &mut buf)?;
        assert_eq!(reader.into_inner(), writer.into_inner());
        Ok(())
    }

    #[test]
    fn test_copy_within() -> io::Result<()> {
        let mut cursor = Cursor::new(vec![0u8, 1u8, 2u8, 3u8, 4u8, 5u8]);
        copy_within(&mut cursor, 1, 4, 0)?;
        assert_eq!(cursor.get_ref(), &[1u8, 2u8, 3u8, 4u8, 4u8, 5u8]);
        copy_within(&mut cursor, 1, 4, 2)?;
        assert_eq!(cursor.get_ref(), &[1u8, 2u8, 2u8, 3u8, 4u8, 4u8]);
        Ok(())
    }
}
