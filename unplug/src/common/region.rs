use std::cmp;
use std::io::{self, ErrorKind, Read, Seek, SeekFrom, Write};

/// A seekable region inside a stream.
pub struct Region<S: Seek> {
    stream: S,
    rel_offset: u64,
    start: u64,
    len: u64,
    max_len: u64,
}

impl<S: Seek> Region<S> {
    /// Constructs a new `Region<S>` which wraps `stream`. The region starts at `start`, is `len`
    /// bytes large, and cannot grow.
    pub fn new(stream: S, start: u64, len: u64) -> io::Result<Self> {
        Self::with_max_len(stream, start, len, len)
    }

    /// Constructs a new `Region<S>` which wraps `stream`. The region starts at `start`, is `len`
    /// bytes large, and can grow up to `max_len` bytes large.
    pub fn with_max_len(mut stream: S, start: u64, len: u64, max_len: u64) -> io::Result<Self> {
        stream.seek(SeekFrom::Start(start))?;
        Ok(Self { stream, rel_offset: 0, start, len, max_len })
    }

    /// Constructs a new `Region<S>` which wraps `stream`. The region starts at `start`, is `len`
    /// bytes large, and the maximum length is set as large as possible.
    pub fn with_inf_max_len(stream: S, start: u64, len: u64) -> io::Result<Self> {
        Self::with_max_len(stream, start, len, std::u64::MAX - start)
    }

    /// Returns true if this region has a length of 0.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the current length of the region.
    pub fn len(&self) -> u64 {
        self.len
    }

    /// Returns the maximum length of the region.
    pub fn max_len(&self) -> u64 {
        self.max_len
    }

    /// Unwraps this `Region<S>`, returning the underlying reader.
    pub fn into_inner(self) -> S {
        self.stream
    }
}

impl<S: Seek> Seek for Region<S> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let rel_offset = match pos {
            SeekFrom::Start(offset) => offset,
            SeekFrom::Current(offset) => {
                if offset == 0 {
                    return Ok(self.rel_offset);
                }
                self.rel_offset.wrapping_add(offset as u64)
            }
            SeekFrom::End(offset) => self.len.wrapping_add(offset as u64),
        };
        let abs_offset = match self.start.checked_add(rel_offset) {
            Some(o) => o,
            None => return Err(ErrorKind::InvalidInput.into()),
        };
        self.stream.seek(SeekFrom::Start(abs_offset))?;
        self.rel_offset = rel_offset;
        Ok(self.rel_offset)
    }
}

impl<S: Read + Seek> Read for Region<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() || self.rel_offset >= self.len {
            return Ok(0);
        }
        let remaining = self.len - self.rel_offset;
        let read_len = cmp::min(buf.len() as u64, remaining) as usize;
        let result = self.stream.read(&mut buf[..read_len]);
        if let Ok(num_read) = result {
            self.rel_offset += num_read as u64;
        }
        result
    }
}

impl<S: Write + Seek> Write for Region<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() || self.rel_offset >= self.max_len {
            return Ok(0);
        }
        let remaining = self.max_len - self.rel_offset;
        let write_len = cmp::min(buf.len() as u64, remaining) as usize;
        let result = self.stream.write(&buf[..write_len]);
        if let Ok(num_written) = result {
            self.rel_offset += num_written as u64;
            self.len = cmp::max(self.len, self.rel_offset);
        }
        result
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_seek_start() -> io::Result<()> {
        let cursor = Cursor::new(vec![0u8; 5]);
        let mut region = Region::new(cursor, 1, 3)?;

        assert_eq!(region.seek(SeekFrom::Start(0))?, 0);
        assert_eq!(region.seek(SeekFrom::Start(3))?, 3);
        assert_eq!(region.seek(SeekFrom::Start(4))?, 4);
        assert!(region.seek(SeekFrom::Start(std::u64::MAX)).is_err());
        assert_eq!(region.seek(SeekFrom::Current(0))?, 4);
        Ok(())
    }

    #[test]
    fn test_seek_current() -> io::Result<()> {
        let cursor = Cursor::new(vec![0u8; 5]);
        let mut region = Region::new(cursor, 1, 3)?;

        assert_eq!(region.seek(SeekFrom::Current(0))?, 0);
        assert_eq!(region.seek(SeekFrom::Current(1))?, 1);
        assert_eq!(region.seek(SeekFrom::Current(2))?, 3);
        assert_eq!(region.seek(SeekFrom::Current(1))?, 4);
        assert_eq!(region.seek(SeekFrom::Current(-1))?, 3);
        assert_eq!(region.seek(SeekFrom::Current(-3))?, 0);
        assert!(region.seek(SeekFrom::Current(-1)).is_err());
        Ok(())
    }

    #[test]
    fn test_seek_end() -> io::Result<()> {
        let cursor = Cursor::new(vec![0u8; 5]);
        let mut region = Region::new(cursor, 1, 3)?;

        assert_eq!(region.seek(SeekFrom::End(0))?, 3);
        assert_eq!(region.seek(SeekFrom::End(1))?, 4);
        assert_eq!(region.seek(SeekFrom::End(-1))?, 2);
        assert_eq!(region.seek(SeekFrom::End(-3))?, 0);
        assert!(region.seek(SeekFrom::End(-4)).is_err());
        assert_eq!(region.seek(SeekFrom::Current(0))?, 0);
        Ok(())
    }

    #[test]
    fn test_read_single() -> io::Result<()> {
        let cursor = Cursor::new(vec![0u8, 1u8, 2u8, 3u8, 4u8]);
        let mut region = Region::new(cursor, 1, 3)?;
        let mut buf = vec![0u8; 1];

        assert_eq!(region.read(&mut buf)?, 1);
        assert_eq!(buf[0], 1u8);
        assert_eq!(region.seek(SeekFrom::Current(0))?, 1);

        assert_eq!(region.read(&mut buf)?, 1);
        assert_eq!(buf[0], 2u8);
        assert_eq!(region.seek(SeekFrom::Current(0))?, 2);

        assert_eq!(region.read(&mut buf)?, 1);
        assert_eq!(buf[0], 3u8);
        assert_eq!(region.seek(SeekFrom::Current(0))?, 3);

        assert_eq!(region.read(&mut buf)?, 0);
        assert_eq!(buf[0], 3u8);
        assert_eq!(region.seek(SeekFrom::Current(0))?, 3);

        Ok(())
    }

    #[test]
    fn test_read_multiple() -> io::Result<()> {
        let cursor = Cursor::new(vec![0u8, 1u8, 2u8, 3u8, 4u8]);
        let mut region = Region::new(cursor, 1, 3)?;
        let mut buf = vec![0u8; 5];

        assert_eq!(region.read(&mut buf)?, 3);
        assert_eq!(&buf[..3], [1u8, 2u8, 3u8]);
        assert_eq!(region.seek(SeekFrom::Current(0))?, 3);

        assert_eq!(region.read(&mut buf)?, 0);
        assert_eq!(&buf[..3], [1u8, 2u8, 3u8]);
        assert_eq!(region.seek(SeekFrom::Current(0))?, 3);
        Ok(())
    }

    #[test]
    fn test_seek_and_read() -> io::Result<()> {
        let cursor = Cursor::new(vec![0u8, 1u8, 2u8, 3u8, 4u8]);
        let mut region = Region::new(cursor, 1, 3)?;
        let mut buf = vec![0u8; 5];

        assert_eq!(region.seek(SeekFrom::Start(1))?, 1);
        assert_eq!(region.read(&mut buf)?, 2);
        assert_eq!(&buf[..2], [2u8, 3u8]);
        assert_eq!(region.seek(SeekFrom::Current(0))?, 3);
        Ok(())
    }

    #[test]
    fn test_write_single() -> io::Result<()> {
        let cursor = Cursor::new(vec![0u8; 5]);
        let mut region = Region::new(cursor, 1, 3)?;

        assert_eq!(region.write(&[1u8])?, 1);
        assert_eq!(region.seek(SeekFrom::Current(0))?, 1);

        assert_eq!(region.write(&[2u8])?, 1);
        assert_eq!(region.seek(SeekFrom::Current(0))?, 2);

        assert_eq!(region.write(&[3u8])?, 1);
        assert_eq!(region.seek(SeekFrom::Current(0))?, 3);

        assert_eq!(region.write(&[3u8])?, 0);
        assert_eq!(region.seek(SeekFrom::Current(0))?, 3);

        let bytes = region.into_inner().into_inner();
        assert_eq!(bytes, [0u8, 1u8, 2u8, 3u8, 0u8]);
        Ok(())
    }

    #[test]
    fn test_write_multiple() -> io::Result<()> {
        let cursor = Cursor::new(vec![0u8; 5]);
        let mut region = Region::new(cursor, 1, 3)?;

        assert_eq!(region.write(&[1u8, 2u8, 3u8, 4u8, 5u8])?, 3);
        assert_eq!(region.seek(SeekFrom::Current(0))?, 3);

        assert_eq!(region.write(&[1u8, 2u8, 3u8, 4u8, 5u8])?, 0);
        assert_eq!(region.seek(SeekFrom::Current(0))?, 3);

        let bytes = region.into_inner().into_inner();
        assert_eq!(bytes, [0u8, 1u8, 2u8, 3u8, 0u8]);
        Ok(())
    }

    #[test]
    fn test_seek_and_write() -> io::Result<()> {
        let cursor = Cursor::new(vec![0u8; 5]);
        let mut region = Region::new(cursor, 1, 3)?;

        assert_eq!(region.seek(SeekFrom::Start(1))?, 1);
        assert_eq!(region.write(&[1u8, 2u8, 3u8, 4u8, 5u8])?, 2);
        assert_eq!(region.seek(SeekFrom::Current(0))?, 3);

        let bytes = region.into_inner().into_inner();
        assert_eq!(bytes, [0u8, 0u8, 1u8, 2u8, 0u8]);
        Ok(())
    }

    #[test]
    fn test_max_len() -> io::Result<()> {
        let cursor = Cursor::new(vec![0u8; 6]);
        let mut region = Region::with_max_len(cursor, 1, 0, 4)?;
        assert_eq!(region.max_len(), 4);

        assert_eq!(region.len(), 0);
        assert_eq!(region.seek(SeekFrom::End(0))?, 0);

        assert_eq!(region.write(&[1u8, 2u8, 3u8])?, 3);
        assert_eq!(region.seek(SeekFrom::Current(0))?, 3);
        assert_eq!(region.len(), 3);
        assert_eq!(region.seek(SeekFrom::End(0))?, 3);

        assert_eq!(region.seek(SeekFrom::Start(1))?, 1);
        assert_eq!(region.write(&[2u8])?, 1);
        assert_eq!(region.len(), 3);
        assert_eq!(region.seek(SeekFrom::End(0))?, 3);

        assert_eq!(region.write(&[4u8, 5u8, 6u8])?, 1);
        assert_eq!(region.seek(SeekFrom::Current(0))?, 4);
        assert_eq!(region.len(), 4);

        assert_eq!(region.write(&[4u8, 5u8, 6u8])?, 0);
        assert_eq!(region.seek(SeekFrom::Current(0))?, 4);
        assert_eq!(region.len(), 4);

        let bytes = region.into_inner().into_inner();
        assert_eq!(bytes, [0u8, 1u8, 2u8, 3u8, 4u8, 0u8]);
        Ok(())
    }

    #[test]
    fn test_inf_max_len() -> io::Result<()> {
        let cursor = Cursor::new(vec![0u8; 6]);
        let mut region = Region::with_inf_max_len(cursor, 1, 0)?;
        assert_eq!(region.max_len(), std::u64::MAX - 1);

        assert_eq!(region.write(&[1u8, 2u8, 3u8, 4u8, 5u8])?, 5);
        assert_eq!(region.len(), 5);

        let bytes = region.into_inner().into_inner();
        assert_eq!(bytes, [0u8, 1u8, 2u8, 3u8, 4u8, 5u8]);
        Ok(())
    }
}
