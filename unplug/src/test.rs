use crate::common::{ReadFrom, WriteTo};
use crate::event::block::{Ip, WriteIp};
use byteorder::{WriteBytesExt, LE};
use std::fmt::Debug;
use std::io::{self, Cursor, Seek, SeekFrom};

/// Asserts that writing a value to a byte array and reading it back produces the same value.
#[macro_export]
macro_rules! assert_write_and_read {
    ($val:expr) => {
        let val = $val;
        assert_eq!($crate::test::write_and_read(&val), val);
    };
}

/// Writes a value to a byte array and reads it back.
/// Use `assert_write_and_read!()` instead of calling this directly.
pub(crate) fn write_and_read<T>(val: &T) -> T
where
    T: ReadFrom<Cursor<Vec<u8>>> + WriteTo<Cursor<Vec<u8>>>,
    <T as ReadFrom<Cursor<Vec<u8>>>>::Error: Debug,
    <T as WriteTo<Cursor<Vec<u8>>>>::Error: Debug,
{
    write_and_read_custom(val, |r| T::read_from(r).unwrap())
}

/// Writes a value to a byte array and reads it back.
/// Use `assert_write_and_read!()` instead of calling this directly.
pub(crate) fn write_and_read_custom<T, F>(val: &T, read: F) -> T
where
    T: WriteTo<Cursor<Vec<u8>>>,
    T::Error: Debug,
    F: FnOnce(&mut Cursor<Vec<u8>>) -> T,
{
    let bytes: Vec<u8> = vec![];
    let mut cursor = Cursor::new(bytes);
    val.write_to(&mut cursor).unwrap();

    let offset = cursor.seek(SeekFrom::Current(0)).unwrap();
    let end_offset = cursor.seek(SeekFrom::End(0)).unwrap();
    assert_eq!(offset, end_offset);

    cursor.seek(SeekFrom::Start(0)).unwrap();
    let val = read(&mut cursor);

    let offset = cursor.seek(SeekFrom::Current(0)).unwrap();
    assert_eq!(offset, end_offset);
    val
}

/// WriteIp implementation which just writes an offset directly
impl WriteIp for Cursor<Vec<u8>> {
    fn write_ip(&mut self, ip: Ip) -> io::Result<()> {
        if let Ip::Offset(offset) = ip {
            Ok(self.write_u32::<LE>(offset)?)
        } else {
            panic!("IP is not an offset: {:?}", ip);
        }
    }
}
