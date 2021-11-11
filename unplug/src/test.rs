use crate::audio::format::{FormatTag, PcmS16Le, ReadWriteBytes};
use crate::audio::Samples;
use crate::common::{ReadFrom, WriteTo};
use crate::event::block::{Ip, WriteIp};
use byteorder::{ByteOrder, WriteBytesExt, LE};
use ctor::ctor;
use std::fmt::{Debug, Display};
use std::io::{self, Cursor, Seek, SeekFrom};
use std::ops::Sub;

/// Test sound provided by whirligig231
pub(crate) const TEST_WAV: &[u8] = include_bytes!("test/ionpack.wav");
/// Offset of the data section in `TEST_WAV`
pub(crate) const TEST_WAV_DATA_OFFSET: usize = 0x24;

/// Test sound left channel encoded to GC ADPCM format
pub(crate) const TEST_WAV_LEFT_DSP: &[u8] = include_bytes!("test/ionpack-dsp-left.bin");
/// End address of the encoded test sound
pub(crate) const TEST_WAV_DSP_END_ADDRESS: usize = 0x30af8;
/// Test sound left channel coefficients
pub(crate) const TEST_WAV_LEFT_COEFFICIENTS: [i16; 16] =
    [73, 1854, 3534, -1788, 923, 977, 3818, -1807, 437, 1541, 3534, -1587, 1768, 228, 3822, -1781];

/// Test sound right channel encoded to GC ADPCM format
pub(crate) const TEST_WAV_RIGHT_DSP: &[u8] = include_bytes!("test/ionpack-dsp-right.bin");
/// Test sound right channel coefficients
pub(crate) const TEST_WAV_RIGHT_COEFFICIENTS: [i16; 16] =
    [49, 1829, 3542, -1781, 784, 1112, 3720, -1707, 374, 1605, 3677, -1738, 1630, 371, 3840, -1800];

/// `ionpack.wav` as PCMS32LE
pub(crate) const TEST_WAV_S32: &[u8] = include_bytes!("test/ionpack-s32.wav");
pub(crate) const TEST_WAV_S32_DATA_OFFSET: usize = 0x66;

/// `ionpack.wav` as PCMF32LE
pub(crate) const TEST_WAV_F32: &[u8] = include_bytes!("test/ionpack-f32.wav");
pub(crate) const TEST_WAV_F32_DATA_OFFSET: usize = 0x72;

/// `ionpack.wav` as FLAC
pub(crate) const TEST_FLAC: &[u8] = include_bytes!("test/ionpack.flac");

/// `ionpack.wav` as MP3
pub(crate) const TEST_MP3: &[u8] = include_bytes!("test/ionpack.mp3");
/// `ionpack.wav` encoded to MP3 and back to WAV
pub(crate) const TEST_MP3_WAV: &[u8] = include_bytes!("test/ionpack-mp3.wav");

/// `ionpack.wav` as Ogg Vorbis
pub(crate) const TEST_OGG: &[u8] = include_bytes!("test/ionpack.ogg");
/// `ionpack.wav` encoded to Ogg Vorbis and back to WAV
pub(crate) const TEST_OGG_WAV: &[u8] = include_bytes!("test/ionpack-ogg.wav");

/// Asserts that writing a value to a byte array and reading it back produces the same value.
#[macro_export]
macro_rules! assert_write_and_read {
    ($val:expr) => {
        let val = $val;
        assert_eq!($crate::test::write_and_read(&val), val);
    };
}

// Initialize env_logger before each unit test. This sucks.
#[ctor]
unsafe fn init_logging() {
    env_logger::init();
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

/// Returns the sample data from the test WAV. The data is stereo PCMS16LE.
pub(crate) fn open_test_wav() -> Vec<i16> {
    let data_header = &TEST_WAV[TEST_WAV_DATA_OFFSET..(TEST_WAV_DATA_OFFSET + 8)];
    let data_id = LE::read_u32(&data_header[0..4]);
    let data_size = LE::read_u32(&data_header[4..8]) as usize;
    assert_eq!(data_id, 0x61746164); // 'data'

    let samples_start = TEST_WAV_DATA_OFFSET + 8;
    let samples_end = samples_start + data_size;
    PcmS16Le::read_bytes(&TEST_WAV[samples_start..samples_end]).unwrap()
}

/// Asserts that two sets of samples are close to each other within a tolerance band.
pub(crate) fn assert_samples_close<F: FormatTag>(
    actual: &Samples<'_, F>,
    expected: &Samples<'_, F>,
    tolerance: F::Data,
) where
    F::Data: PartialOrd + Sub<Output = F::Data> + Display,
{
    assert_eq!(actual.channels, expected.channels);
    assert_eq!(actual.len, expected.len);
    for (&a, &e) in actual.data.iter().take(actual.len).zip(&expected.data[..expected.len]) {
        let delta = if a >= e { a - e } else { e - a };
        assert!(delta <= tolerance, "actual = {}, expected = {}, delta = {}", a, e, delta);
    }
}
