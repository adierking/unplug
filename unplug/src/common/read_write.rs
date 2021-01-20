use byteorder::ReadBytesExt;
use std::ffi::CString;
use std::io::{self, Read, Seek, Write};

/// Trait for a readable and seekable stream.
pub trait ReadSeek: Read + Seek {}
impl<R: Read + Seek> ReadSeek for R {}

/// Trait for a writable and seekable stream.
pub trait WriteSeek: Write + Seek {}
impl<W: Write + Seek> WriteSeek for W {}

/// Trait for a readable, writable, and seekable stream.
pub trait ReadWriteSeek: Read + Write + Seek {}
impl<S: Read + Write + Seek> ReadWriteSeek for S {}

/// Trait for an object which can be read from a stream.
pub trait ReadFrom<R: Read>: Sized {
    type Error;
    fn read_from(reader: &mut R) -> Result<Self, Self::Error>;
}

/// Trait for a nullable object which can be read from a stream.
pub trait ReadOptionFrom<R: Read>: Sized {
    type Error;
    fn read_option_from(reader: &mut R) -> Result<Option<Self>, Self::Error>;
}

/// Trait for an object which can be written to a stream.
pub trait WriteTo<W: Write>: Sized {
    type Error;
    fn write_to(&self, writer: &mut W) -> Result<(), Self::Error>;
}

/// Trait for a nullable object which can be written to a stream.
pub trait WriteOptionTo<W: Write>: Sized {
    type Error;
    fn write_option_to(opt: Option<&Self>, writer: &mut W) -> Result<(), Self::Error>;
}

/// Blanket implementation for reading `Option<T>`
impl<R: Read, T: ReadOptionFrom<R>> ReadFrom<R> for Option<T> {
    type Error = T::Error;
    fn read_from(reader: &mut R) -> Result<Self, Self::Error> {
        T::read_option_from(reader)
    }
}

/// Blanket implementation for writing optional `&T`
impl<W: Write, T: WriteOptionTo<W>> WriteOptionTo<W> for &T {
    type Error = T::Error;
    fn write_option_to(opt: Option<&Self>, writer: &mut W) -> Result<(), Self::Error> {
        T::write_option_to(opt.copied(), writer)
    }
}

/// Blanket implementation for writing `Option<T>`
impl<W: Write, T: WriteOptionTo<W>> WriteTo<W> for Option<T> {
    type Error = T::Error;
    fn write_to(&self, writer: &mut W) -> Result<(), Self::Error> {
        T::write_option_to(self.as_ref(), writer)
    }
}

/// `ReadFrom` implementation for reading a null-terminated string
impl<R: Read> ReadFrom<R> for CString {
    type Error = io::Error;
    fn read_from(reader: &mut R) -> io::Result<Self> {
        let mut bytes: Vec<u8> = vec![];
        loop {
            let b = reader.read_u8()?;
            if b == 0 {
                break;
            }
            bytes.push(b);
        }
        Ok(CString::new(bytes).unwrap())
    }
}

/// `WriteTo` implementation for writing a null-terminated string
impl<W: Write> WriteTo<W> for CString {
    type Error = io::Error;
    fn write_to(&self, writer: &mut W) -> io::Result<()> {
        Ok(writer.write_all(self.as_bytes_with_nul())?)
    }
}
