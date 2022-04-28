use anyhow::Result;
use std::convert::{AsRef, TryInto};
use std::fs::File;
use std::io::{self, Cursor, Read, Seek, SeekFrom, Stdout, Write};
use std::path::Path;

/// A cursor around a byte slice.
pub type MemoryCursor = Cursor<Box<[u8]>>;

/// A writer which forwards output to either stdout or a file.
/// This is more useful than shell redirection on Windows because it supports Unicode better.
pub enum OutputRedirect {
    File(File),
    Stdout(Stdout),
}

impl OutputRedirect {
    /// Opens an output file if `path` is specified or stdout otherwise.
    pub fn new(path: Option<impl AsRef<Path>>) -> Result<Self> {
        Ok(match path {
            Some(path) => Self::File(File::create(path)?),
            None => Self::Stdout(io::stdout()),
        })
    }
}

impl Write for OutputRedirect {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::File(f) => f.write(buf),
            Self::Stdout(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::File(f) => f.flush(),
            Self::Stdout(s) => s.flush(),
        }
    }
}

/// Copies an entire reader into memory and then returns a `Cursor` over the bytes.
pub fn copy_into_memory(mut reader: (impl Read + Seek)) -> io::Result<MemoryCursor> {
    let size = reader.seek(SeekFrom::End(0))?.try_into().expect("File size overflow");
    reader.seek(SeekFrom::Start(0))?;
    let mut buf = Vec::with_capacity(size);
    reader.read_to_end(&mut buf)?;
    Ok(Cursor::new(buf.into_boxed_slice()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_copy_into_memory() -> io::Result<()> {
        let mut original = Cursor::new(vec![]);
        original.write_all(&[0u8, 1u8, 2u8, 3u8, 4u8])?;
        let copy = copy_into_memory(&mut original)?;
        assert_eq!(&*copy.into_inner(), &*original.into_inner());
        Ok(())
    }
}
