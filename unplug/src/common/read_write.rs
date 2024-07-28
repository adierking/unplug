use byteorder::ReadBytesExt;
use std::ffi::CString;
use std::io::{self, Read, Seek, Write};
use std::mem::MaybeUninit;

/// Trait for a readable and seekable stream.
pub trait ReadSeek: Read + Seek + Send {}
impl<R: Read + Seek + Send + ?Sized> ReadSeek for R {}

/// Trait for a writable and seekable stream.
pub trait WriteSeek: Write + Seek + Send {}
impl<W: Write + Seek + Send + ?Sized> WriteSeek for W {}

/// Trait for a readable, writable, and seekable stream.
pub trait ReadWriteSeek: Read + Write + Seek + Send {}
impl<S: Read + Write + Seek + Send + ?Sized> ReadWriteSeek for S {}

/// Trait for an object which can be read from a stream.
pub trait ReadFrom<R: Read + ?Sized>: Sized {
    /// The error type returned from `read_from()`.
    type Error;

    /// Reads an instance of this object from `reader`.
    fn read_from(reader: &mut R) -> Result<Self, Self::Error>;

    /// Fills a slice with instances of this object read from `reader`.
    fn read_all_from(reader: &mut R, buf: &mut [Self]) -> Result<(), Self::Error> {
        for elem in buf {
            *elem = Self::read_from(reader)?;
        }
        Ok(())
    }
}

/// Trait for a nullable object which can be read from a stream.
pub trait ReadOptionFrom<R: Read + ?Sized>: Sized {
    type Error;
    fn read_option_from(reader: &mut R) -> Result<Option<Self>, Self::Error>;
}

/// Trait for an object which can be written to a stream.
pub trait WriteTo<W: Write + ?Sized>: Sized {
    /// The error type returned from `write_to()`.
    type Error;

    /// Writes this object to `writer`.
    fn write_to(&self, writer: &mut W) -> Result<(), Self::Error>;

    /// Writes a slice of instances of this object to `writer`.
    fn write_all_to(writer: &mut W, buf: &[Self]) -> Result<(), Self::Error> {
        for elem in buf {
            elem.write_to(writer)?;
        }
        Ok(())
    }
}

/// Trait for a nullable object which can be written to a stream.
pub trait WriteOptionTo<W: Write + ?Sized>: Sized {
    type Error;
    fn write_option_to(opt: Option<&Self>, writer: &mut W) -> Result<(), Self::Error>;
}

/// Blanket implementation for reading `Option<T>`
impl<R: Read + ?Sized, T: ReadOptionFrom<R>> ReadFrom<R> for Option<T> {
    type Error = T::Error;
    fn read_from(reader: &mut R) -> Result<Self, Self::Error> {
        T::read_option_from(reader)
    }
}

/// Blanket implementation for writing optional `&T`
impl<W: Write + ?Sized, T: WriteOptionTo<W>> WriteOptionTo<W> for &T {
    type Error = T::Error;
    fn write_option_to(opt: Option<&Self>, writer: &mut W) -> Result<(), Self::Error> {
        T::write_option_to(opt.copied(), writer)
    }
}

/// Blanket implementation for writing `Option<T>`
impl<W: Write + ?Sized, T: WriteOptionTo<W>> WriteTo<W> for Option<T> {
    type Error = T::Error;
    fn write_to(&self, writer: &mut W) -> Result<(), Self::Error> {
        T::write_option_to(self.as_ref(), writer)
    }
}

/// `ReadFrom` implementation for reading a null-terminated string
impl<R: Read + ?Sized> ReadFrom<R> for CString {
    type Error = io::Error;
    fn read_from(reader: &mut R) -> io::Result<Self> {
        let mut bytes: Vec<u8> = vec![];
        loop {
            let b = reader.read_u8()?;
            bytes.push(b);
            if b == 0 {
                break;
            }
        }
        unsafe { Ok(CString::from_vec_with_nul_unchecked(bytes)) }
    }
}

/// `WriteTo` implementation for writing a null-terminated string
impl<W: Write + ?Sized> WriteTo<W> for CString {
    type Error = io::Error;
    fn write_to(&self, writer: &mut W) -> io::Result<()> {
        writer.write_all(self.as_bytes_with_nul())
    }
}

/// `ReadFrom` implementation for reading bytes
impl<R: Read + ?Sized> ReadFrom<R> for u8 {
    type Error = io::Error;
    fn read_from(reader: &mut R) -> Result<Self, Self::Error> {
        let b = &mut [0u8];
        reader.read_exact(b)?;
        Ok(b[0])
    }
    fn read_all_from(reader: &mut R, buf: &mut [u8]) -> Result<(), Self::Error> {
        reader.read_exact(buf)
    }
}

/// `WriteTo` implementation for writing bytes
impl<W: Write + ?Sized> WriteTo<W> for u8 {
    type Error = io::Error;
    fn write_to(&self, writer: &mut W) -> Result<(), Self::Error> {
        writer.write_all(&[*self])
    }
    fn write_all_to(writer: &mut W, buf: &[u8]) -> Result<(), Self::Error> {
        writer.write_all(buf)
    }
}

/// `ReadFrom` implementation for reading arrays of trivial types
impl<R: Read + ?Sized, T: ReadFrom<R> + Copy, const N: usize> ReadFrom<R> for [T; N] {
    type Error = T::Error;
    fn read_from(reader: &mut R) -> Result<Self, Self::Error> {
        let mut result = MaybeUninit::<[T; N]>::uninit();
        unsafe {
            let slice = std::slice::from_raw_parts_mut(result.as_mut_ptr().cast(), N);
            T::read_all_from(reader, slice)?;
            Ok(result.assume_init())
        }
    }
}

/// `WriteTo` implementation for writing arrays of trivial types
impl<W: Write + ?Sized, T: WriteTo<W> + Copy, const N: usize> WriteTo<W> for [T; N] {
    type Error = T::Error;
    fn write_to(&self, writer: &mut W) -> Result<(), Self::Error> {
        T::write_all_to(writer, self)
    }
}
