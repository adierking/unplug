use super::WriteTo;
use byteorder::WriteBytesExt;
use std::cmp;
use std::ffi::{CStr, CString};
use std::io::{self, ErrorKind, Read, Seek, SeekFrom, Write};

pub const BUFFER_SIZE: usize = 0x8000;

/// Writes a byte followed by a writable object.
pub fn write_u8_and<W, T>(writer: &mut W, op: u8, obj: &T) -> Result<(), T::Error>
where
    W: Write,
    T: WriteTo<W>,
    T::Error: From<io::Error>,
{
    writer.write_u8(op)?;
    obj.write_to(writer)
}

/// Reads a fixed-size null-terminated string from `reader`. This will allocate `size` bytes.
pub fn read_fixed_string(mut reader: impl Read, size: usize) -> io::Result<CString> {
    let mut bytes = vec![0u8; size];
    reader.read_exact(&mut bytes[..size])?;
    let len = match bytes.iter().position(|&b| b == 0) {
        Some(i) => i,
        None => {
            return Err(io::Error::new(ErrorKind::InvalidData, "string is not null-terminated"))
        }
    };
    bytes.resize(len, 0);
    Ok(CString::new(bytes)?)
}

/// Writes a fixed-size null-terminated string to `writer`. This will allocate `size` bytes.
pub fn write_fixed_string(
    mut writer: impl Write,
    string: impl AsRef<CStr>,
    size: usize,
) -> io::Result<()> {
    let mut out_bytes = vec![0u8; size];
    let in_bytes = string.as_ref().to_bytes();
    let len = in_bytes.len();
    if len >= size {
        return Err(io::Error::new(ErrorKind::InvalidInput, "string is too long"));
    }
    out_bytes[..len].copy_from_slice(in_bytes);
    writer.write_all(&out_bytes)?;
    Ok(())
}

/// Fills the next `len` bytes in `writer` with `byte`.
pub fn fill(mut writer: impl Write, byte: u8, len: u64) -> io::Result<()> {
    if len > 0 {
        io::copy(&mut io::repeat(byte).take(len), &mut writer)?;
    }
    Ok(())
}

/// Writes padding bytes so that a writer's offset is aligned to a power of two.
pub fn pad(mut writer: (impl Write + Seek), align: u64, fill: u8) -> io::Result<()> {
    let offset = writer.seek(SeekFrom::Current(0))?;
    let aligned = (offset + align - 1) & !(align - 1);
    let padding = aligned - offset;
    self::fill(writer, fill, padding)?;
    Ok(())
}

/// Implementation of `std::io::copy` which uses `buf` as the buffer. This can be much faster than
/// the built-in implementation.
pub fn copy_buffered(
    reader: &mut impl Read,
    writer: &mut impl Write,
    buf: &mut [u8],
) -> io::Result<u64> {
    let mut total = 0;
    loop {
        let num_read = match reader.read(buf) {
            Ok(0) => return Ok(total),
            Ok(len) => len,
            Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
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
    fn test_read_fixed_string() -> io::Result<()> {
        let mut cursor = Cursor::new(&[b't', b'e', b's', b't', 0, 0, 0, 0]);

        assert_eq!(read_fixed_string(&mut cursor, 8)?, CString::new("test")?);
        assert_eq!(cursor.seek(SeekFrom::Current(0))?, 8);
        cursor.seek(SeekFrom::Start(0))?;

        assert_eq!(read_fixed_string(&mut cursor, 5)?, CString::new("test")?);
        assert_eq!(cursor.seek(SeekFrom::Current(0))?, 5);
        cursor.seek(SeekFrom::Start(0))?;

        assert!(read_fixed_string(&mut cursor, 4).is_err());
        Ok(())
    }

    #[test]
    fn test_write_fixed_string() -> io::Result<()> {
        let mut bytes = vec![];
        write_fixed_string(&mut bytes, CString::new("test")?, 8)?;
        assert_eq!(bytes, &[b't', b'e', b's', b't', 0, 0, 0, 0]);

        let mut bytes = vec![];
        write_fixed_string(&mut bytes, CString::new("test")?, 5)?;
        assert_eq!(bytes, &[b't', b'e', b's', b't', 0]);

        let mut bytes = vec![];
        assert!(write_fixed_string(&mut bytes, CString::new("test")?, 4).is_err());
        assert_eq!(bytes, &[]);
        Ok(())
    }

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
