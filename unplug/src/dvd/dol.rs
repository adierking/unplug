use crate::common::{ReadFrom, WriteTo};
use byteorder::{ByteOrder, ReadBytesExt, WriteBytesExt, BE};
use std::io::{self, Read, Write};
use thiserror::Error;

const NUM_TEXT_SECTIONS: usize = 7;
const NUM_DATA_SECTIONS: usize = 11;

/// The result type for DOL operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for DOL operations.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("unmapped address: {0:#x}")]
    UnmappedAddress(u32),

    #[error("unmapped offset: {0:#x}")]
    UnmappedOffset(u32),

    #[error("invalid text section index: {0}")]
    InvalidTextSection(usize),

    #[error("invalid text section index: {0}")]
    InvalidDataSection(usize),

    #[error("BSS pointers cannot be converted to file offsets")]
    BssPointer,

    #[error("offset {offset:#x} is past the end of the section ({end:#x})")]
    PastEndOfSection { offset: u32, end: u32 },

    #[error(transparent)]
    Io(Box<io::Error>),
}

from_error_boxed!(Error::Io, io::Error);

/// A pointer within an executable section.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DolPointer {
    Text { index: usize, offset: u32 },
    Data { index: usize, offset: u32 },
    Bss { offset: u32 },
}

/// The header in a Dolphin executable (.dol).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DolHeader {
    pub text_offsets: [u32; NUM_TEXT_SECTIONS],
    pub data_offsets: [u32; NUM_DATA_SECTIONS],
    pub text_addresses: [u32; NUM_TEXT_SECTIONS],
    pub data_addresses: [u32; NUM_DATA_SECTIONS],
    pub text_sizes: [u32; NUM_TEXT_SECTIONS],
    pub data_sizes: [u32; NUM_DATA_SECTIONS],
    pub bss_address: u32,
    pub bss_size: u32,
    pub entry_point: u32,
}

impl DolHeader {
    /// Constructs an empty `DolHeader` with all fields set to zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// Calculates the total size of the executable file.
    pub fn file_size(&self) -> u32 {
        let text = self.text_offsets.iter().zip(&self.text_sizes);
        let data = self.data_offsets.iter().zip(&self.data_sizes);
        text.chain(data).map(|(&offset, &size)| offset + size).max().unwrap()
    }

    /// Returns a `DolPointer` corresponding to a memory address.
    pub fn locate_address(&self, address: u32) -> Result<DolPointer> {
        let text = self.text_addresses.iter().zip(&self.text_sizes).enumerate();
        let data = self.data_addresses.iter().zip(&self.data_sizes).enumerate();
        match Self::locate(address, text, data) {
            Some(o) => Ok(o),
            None => {
                if address >= self.bss_address && address < self.bss_address + self.bss_size {
                    Ok(DolPointer::Bss { offset: address - self.bss_address })
                } else {
                    Err(Error::UnmappedAddress(address))
                }
            }
        }
    }

    /// Returns a `DolPointer` corresponding to a file offset.
    pub fn locate_offset(&self, offset: u32) -> Result<DolPointer> {
        let text = self.text_offsets.iter().zip(&self.text_sizes).enumerate();
        let data = self.data_offsets.iter().zip(&self.data_sizes).enumerate();
        Self::locate(offset, text, data).ok_or(Error::UnmappedOffset(offset))
    }

    fn locate<'a, I>(pointer: u32, mut text: I, mut data: I) -> Option<DolPointer>
    where
        I: Iterator<Item = (usize, (&'a u32, &'a u32))>,
    {
        text.find(|(_, (&offset, &size))| pointer >= offset && pointer < offset + size)
            .map(|(i, (offset, _))| DolPointer::Text { index: i, offset: pointer - offset })
            .or_else(|| {
                data.find(|(_, (&offset, &size))| pointer >= offset && pointer < offset + size)
                    .map(|(i, (offset, _))| DolPointer::Data { index: i, offset: pointer - offset })
            })
    }

    /// Returns the memory address corresponding to a `DolPointer`.
    pub fn address_of(&self, pointer: DolPointer) -> Result<u32> {
        let (base, size, offset) = match pointer {
            DolPointer::Text { index, offset } => {
                if index >= NUM_TEXT_SECTIONS {
                    return Err(Error::InvalidTextSection(index));
                }
                (self.text_addresses[index], self.text_sizes[index], offset)
            }
            DolPointer::Data { index, offset } => {
                if index >= NUM_DATA_SECTIONS {
                    return Err(Error::InvalidDataSection(index));
                }
                (self.data_addresses[index], self.data_sizes[index], offset)
            }
            DolPointer::Bss { offset } => (self.bss_address, self.bss_size, offset),
        };
        if offset < size {
            Ok(base + offset)
        } else {
            Err(Error::PastEndOfSection { offset, end: size })
        }
    }

    /// Returns the file offset corresponding to a `DolPointer`.
    pub fn offset_of(&self, pointer: DolPointer) -> Result<u32> {
        let (base, size, offset) = match pointer {
            DolPointer::Text { index, offset } => {
                if index >= NUM_TEXT_SECTIONS {
                    return Err(Error::InvalidTextSection(index));
                }
                (self.text_offsets[index], self.text_sizes[index], offset)
            }
            DolPointer::Data { index, offset } => {
                if index >= NUM_DATA_SECTIONS {
                    return Err(Error::InvalidDataSection(index));
                }
                (self.data_offsets[index], self.data_sizes[index], offset)
            }
            DolPointer::Bss { .. } => return Err(Error::BssPointer),
        };
        if offset < size {
            Ok(base + offset)
        } else {
            Err(Error::PastEndOfSection { offset, end: size })
        }
    }

    pub fn address_to_offset(&self, address: u32) -> Result<u32> {
        self.offset_of(self.locate_address(address)?)
    }

    pub fn offset_to_address(&self, offset: u32) -> Result<u32> {
        self.address_of(self.locate_offset(offset)?)
    }
}

impl<R: Read + ?Sized> ReadFrom<R> for DolHeader {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let mut header = Self::new();
        reader.read_u32_into::<BE>(&mut header.text_offsets)?;
        reader.read_u32_into::<BE>(&mut header.data_offsets)?;
        reader.read_u32_into::<BE>(&mut header.text_addresses)?;
        reader.read_u32_into::<BE>(&mut header.data_addresses)?;
        reader.read_u32_into::<BE>(&mut header.text_sizes)?;
        reader.read_u32_into::<BE>(&mut header.data_sizes)?;
        header.bss_address = reader.read_u32::<BE>()?;
        header.bss_size = reader.read_u32::<BE>()?;
        header.entry_point = reader.read_u32::<BE>()?;
        Ok(header)
    }
}

fn write_u32_slice(
    writer: &mut (impl Write + ?Sized),
    slice: &[u32],
    storage: &mut [u8],
) -> Result<()> {
    BE::write_u32_into(slice, storage);
    Ok(writer.write_all(storage)?)
}

impl<W: Write + ?Sized> WriteTo<W> for DolHeader {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        let mut text_bytes = [0u8; NUM_TEXT_SECTIONS * 4];
        let mut data_bytes = [0u8; NUM_DATA_SECTIONS * 4];
        write_u32_slice(writer, &self.text_offsets, &mut text_bytes)?;
        write_u32_slice(writer, &self.data_offsets, &mut data_bytes)?;
        write_u32_slice(writer, &self.text_addresses, &mut text_bytes)?;
        write_u32_slice(writer, &self.data_addresses, &mut data_bytes)?;
        write_u32_slice(writer, &self.text_sizes, &mut text_bytes)?;
        write_u32_slice(writer, &self.data_sizes, &mut data_bytes)?;
        writer.write_u32::<BE>(self.bss_address)?;
        writer.write_u32::<BE>(self.bss_size)?;
        writer.write_u32::<BE>(self.entry_point)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assert_write_and_read;

    static TEST_HEADER: DolHeader = DolHeader {
        text_offsets: [0x100, 0x2600, 0, 0, 0, 0, 0],
        data_offsets: [
            0x1f83e0, 0x1f9460, 0x1fa660, 0x1fa6c0, 0x1fa6e0, 0x20cbc0, 0x259200, 0x25a480, 0, 0, 0,
        ],
        text_addresses: [0x80003100, 0x80007880, 0, 0, 0, 0, 0],
        data_addresses: [
            0x80005600, 0x80006680, 0x801fd660, 0x801fd6c0, 0x801fd6e0, 0x8020fbc0, 0x80659ec0,
            0x8065bd40, 0, 0, 0,
        ],
        text_sizes: [0x2500, 0x1f5de0, 0, 0, 0, 0, 0],
        data_sizes: [0x1080, 0x1200, 0x60, 0x20, 0x124e0, 0x4c640, 0x1280, 0x4220, 0, 0, 0],
        bss_address: 0x8025c200,
        bss_size: 0x4040f0,
        entry_point: 0x80005240,
    };

    #[test]
    fn test_write_and_read_dol_header() -> Result<()> {
        let mut header = DolHeader::new();
        for i in 0..NUM_TEXT_SECTIONS {
            header.text_offsets[i] = i as u32;
            header.text_addresses[i] = 100 + i as u32;
            header.text_sizes[i] = 200 + i as u32;
        }
        for i in 0..NUM_DATA_SECTIONS {
            header.data_offsets[i] = 300 + i as u32;
            header.data_addresses[i] = 400 + i as u32;
            header.data_sizes[i] = 500 + i as u32;
        }
        header.bss_address = 600;
        header.bss_size = 601;
        header.entry_point = 602;
        assert_write_and_read!(header);
        Ok(())
    }

    #[test]
    fn test_file_size() {
        assert_eq!(TEST_HEADER.file_size(), 0x25e6a0);
    }

    fn text(index: usize, offset: u32) -> DolPointer {
        DolPointer::Text { index, offset }
    }

    fn data(index: usize, offset: u32) -> DolPointer {
        DolPointer::Data { index, offset }
    }

    fn bss(offset: u32) -> DolPointer {
        DolPointer::Bss { offset }
    }

    #[test]
    fn test_locate_address() -> Result<()> {
        assert_eq!(TEST_HEADER.locate_address(0x80003100)?, text(0, 0));
        assert_eq!(TEST_HEADER.locate_address(0x801fd65f)?, text(1, 0x1f5ddf));
        assert_eq!(TEST_HEADER.locate_address(0x80005600)?, data(0, 0));
        assert_eq!(TEST_HEADER.locate_address(0x8065ff5f)?, data(7, 0x421f));
        assert_eq!(TEST_HEADER.locate_address(0x8025c200)?, bss(0));
        assert_eq!(TEST_HEADER.locate_address(0x806602ef)?, bss(0x4040ef));
        assert!(TEST_HEADER.locate_address(0x800030ff).is_err());
        assert!(TEST_HEADER.locate_address(0x806602f0).is_err());
        assert!(TEST_HEADER.locate_address(0).is_err());
        Ok(())
    }

    #[test]
    fn test_address_of() -> Result<()> {
        assert_eq!(TEST_HEADER.address_of(text(0, 0))?, 0x80003100);
        assert_eq!(TEST_HEADER.address_of(text(1, 0x1f5ddf))?, 0x801fd65f);
        assert_eq!(TEST_HEADER.address_of(data(0, 0))?, 0x80005600);
        assert_eq!(TEST_HEADER.address_of(data(7, 0x421f))?, 0x8065ff5f);
        assert_eq!(TEST_HEADER.address_of(bss(0))?, 0x8025c200);
        assert_eq!(TEST_HEADER.address_of(bss(0x4040ef))?, 0x806602ef);
        assert!(TEST_HEADER.address_of(text(1, 0x1f5de0)).is_err());
        assert!(TEST_HEADER.address_of(text(2, 0)).is_err());
        assert!(TEST_HEADER.address_of(text(7, 0)).is_err());
        assert!(TEST_HEADER.address_of(data(7, 0x4220)).is_err());
        assert!(TEST_HEADER.address_of(data(8, 0)).is_err());
        assert!(TEST_HEADER.address_of(data(11, 0)).is_err());
        assert!(TEST_HEADER.address_of(bss(0x4040f0)).is_err());
        Ok(())
    }

    #[test]
    fn test_address_to_offset() -> Result<()> {
        assert_eq!(TEST_HEADER.address_to_offset(0x80003100)?, 0x100);
        assert_eq!(TEST_HEADER.address_to_offset(0x801fd65f)?, 0x1f83df);
        assert_eq!(TEST_HEADER.address_to_offset(0x80005600)?, 0x1f83e0);
        assert_eq!(TEST_HEADER.address_to_offset(0x8065ff5f)?, 0x25e69f);
        assert!(TEST_HEADER.address_to_offset(0x8025c200).is_err());
        assert!(TEST_HEADER.address_to_offset(0x806602ef).is_err());
        assert!(TEST_HEADER.address_to_offset(0x800030ff).is_err());
        assert!(TEST_HEADER.address_to_offset(0x806602f0).is_err());
        assert!(TEST_HEADER.address_to_offset(0).is_err());
        Ok(())
    }

    #[test]
    fn test_locate_offset() -> Result<()> {
        assert_eq!(TEST_HEADER.locate_offset(0x100)?, text(0, 0));
        assert_eq!(TEST_HEADER.locate_offset(0x1f83df)?, text(1, 0x1f5ddf));
        assert_eq!(TEST_HEADER.locate_offset(0x1f83e0)?, data(0, 0));
        assert_eq!(TEST_HEADER.locate_offset(0x25e69f)?, data(7, 0x421f));
        assert!(TEST_HEADER.locate_offset(0x8025c200).is_err());
        assert!(TEST_HEADER.locate_offset(0x806602ef).is_err());
        assert!(TEST_HEADER.locate_offset(0xff).is_err());
        assert!(TEST_HEADER.locate_offset(0x25e6a0).is_err());
        assert!(TEST_HEADER.locate_offset(0).is_err());
        Ok(())
    }

    #[test]
    fn test_offset_of() -> Result<()> {
        assert_eq!(TEST_HEADER.offset_of(text(0, 0))?, 0x100);
        assert_eq!(TEST_HEADER.offset_of(text(1, 0x1f5ddf))?, 0x1f83df);
        assert_eq!(TEST_HEADER.offset_of(data(0, 0))?, 0x1f83e0);
        assert_eq!(TEST_HEADER.offset_of(data(7, 0x421f))?, 0x25e69f);
        assert!(TEST_HEADER.offset_of(text(1, 0x1f5de0)).is_err());
        assert!(TEST_HEADER.offset_of(text(2, 0)).is_err());
        assert!(TEST_HEADER.offset_of(text(7, 0)).is_err());
        assert!(TEST_HEADER.offset_of(data(7, 0x4220)).is_err());
        assert!(TEST_HEADER.offset_of(data(8, 0)).is_err());
        assert!(TEST_HEADER.offset_of(data(11, 0)).is_err());
        assert!(TEST_HEADER.offset_of(bss(0)).is_err());
        Ok(())
    }

    #[test]
    fn test_offset_to_address() -> Result<()> {
        assert_eq!(TEST_HEADER.offset_to_address(0x100)?, 0x80003100);
        assert_eq!(TEST_HEADER.offset_to_address(0x1f83df)?, 0x801fd65f);
        assert_eq!(TEST_HEADER.offset_to_address(0x1f83e0)?, 0x80005600);
        assert_eq!(TEST_HEADER.offset_to_address(0x25e69f)?, 0x8065ff5f);
        assert!(TEST_HEADER.offset_to_address(0xff).is_err());
        assert!(TEST_HEADER.offset_to_address(0x25e6a0).is_err());
        assert!(TEST_HEADER.offset_to_address(0).is_err());
        Ok(())
    }
}
