use super::{ReadFrom, WriteTo};
use encoding_rs::{EncoderResult, SHIFT_JIS, WINDOWS_1252};
use std::borrow::Cow;
use std::error::Error as StdError;
use std::ffi::CString;
use std::fmt;
use std::io::{Read, Write};
use std::iter;
use std::result::Result as StdResult;
use thiserror::Error;

/// The result type for text operations.
pub type Result<T> = StdResult<T, Error>;

/// The error type for text operations.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("text could not be encoded with SHIFT-JIS or Windows-1252: {0:?}")]
    Encode(String),

    #[error("text is too long ({0} bytes max)")]
    TooLong(usize),

    #[error("text contains an unexpected NUL byte")]
    UnexpectedNul,

    #[error("text data must contain a NUL terminator")]
    NoNulTerminator,

    #[error("text could not be decoded with SHIFT-JIS or Windows-1252")]
    Decode,

    #[error(transparent)]
    Other(Box<dyn StdError + Send + Sync>),
}

/// Base trait for raw localized text data.
pub trait TextData: Sized {
    /// The maximum text size in bytes, excluding any NULs.
    const MAX_LEN: usize;

    /// Returns the raw bytes of the text data, which may include NULs.
    fn as_raw_bytes(&self) -> &[u8];

    /// Returns the bytes of the text up to any NUL.
    fn as_bytes_without_nul(&self) -> &[u8];

    /// Creates text data from raw bytes.
    fn from_raw_bytes(bytes: Vec<u8>) -> Result<Self>;

    /// Creates text data from bytes which do not contain NUL.
    fn from_bytes_without_nul(bytes: Vec<u8>) -> Result<Self>;

    /// Converts the data into raw bytes.
    fn into_raw_bytes(self) -> Vec<u8>;

    /// Converts the data into bytes up to any NUL.
    fn into_bytes_without_nul(self) -> Vec<u8>;

    /// Returns data corresponding to an empty string.
    fn empty() -> Self;
}

/// Text stored in a `Vec<u8>` is not null-terminated.
impl TextData for Vec<u8> {
    const MAX_LEN: usize = usize::MAX;
    fn as_raw_bytes(&self) -> &[u8] {
        self.as_ref()
    }
    fn as_bytes_without_nul(&self) -> &[u8] {
        self.as_ref()
    }
    fn from_raw_bytes(bytes: Vec<u8>) -> Result<Self> {
        if bytes.iter().all(|b| *b != 0) {
            Ok(bytes)
        } else {
            Err(Error::UnexpectedNul)
        }
    }
    fn from_bytes_without_nul(bytes: Vec<u8>) -> Result<Self> {
        Self::from_raw_bytes(bytes)
    }
    fn into_raw_bytes(self) -> Vec<u8> {
        self
    }
    fn into_bytes_without_nul(self) -> Vec<u8> {
        self
    }
    fn empty() -> Self {
        Vec::new()
    }
}

/// Text stored in a `CString` is null-terminated.
impl TextData for CString {
    const MAX_LEN: usize = usize::MAX;
    fn as_raw_bytes(&self) -> &[u8] {
        CString::as_bytes_with_nul(&self)
    }
    fn as_bytes_without_nul(&self) -> &[u8] {
        CString::as_bytes(&self)
    }
    fn from_raw_bytes(bytes: Vec<u8>) -> Result<Self> {
        Self::from_vec_with_nul(bytes).map_err(|_| Error::NoNulTerminator)
    }
    fn from_bytes_without_nul(bytes: Vec<u8>) -> Result<Self> {
        Self::new(bytes).map_err(|_| Error::UnexpectedNul)
    }
    fn into_raw_bytes(self) -> Vec<u8> {
        CString::into_bytes_with_nul(self)
    }
    fn into_bytes_without_nul(self) -> Vec<u8> {
        CString::into_bytes(self)
    }
    fn empty() -> Self {
        CString::default()
    }
}

fn to_array_with_nul<const N: usize>(b: &[u8]) -> Result<[u8; N]> {
    let len = b.len();
    if len < N {
        let mut array = [0u8; N];
        array[..len].copy_from_slice(b);
        Ok(array)
    } else {
        Err(Error::TooLong(N.saturating_sub(1)))
    }
}

/// Text stored in an array is null-terminated and has a maximum length.
impl<const N: usize> TextData for [u8; N] {
    const MAX_LEN: usize = N.saturating_sub(1);
    fn as_raw_bytes(&self) -> &[u8] {
        self.as_ref()
    }
    fn as_bytes_without_nul(&self) -> &[u8] {
        let len = self.iter().position(|b| *b == 0).unwrap_or(self.len());
        &self.as_ref()[..len]
    }
    fn from_raw_bytes(bytes: Vec<u8>) -> Result<Self> {
        let len = bytes.iter().position(|b| *b == 0).ok_or(Error::NoNulTerminator)?;
        to_array_with_nul::<N>(&bytes[..len])
    }
    fn from_bytes_without_nul(bytes: Vec<u8>) -> Result<Self> {
        if bytes.iter().all(|b| *b != 0) {
            to_array_with_nul::<N>(&bytes)
        } else {
            Err(Error::UnexpectedNul)
        }
    }
    fn into_raw_bytes(self) -> Vec<u8> {
        Vec::from(self)
    }
    fn into_bytes_without_nul(self) -> Vec<u8> {
        Vec::from(self.as_bytes_without_nul())
    }
    fn empty() -> Self {
        [0u8; N]
    }
}

/// Text backed by a `CString`.
/// It is null-terminated and has no maximum length.
pub type CText = Text<CString>;

/// Text backed by a byte array.
/// It is null-terminated and has a maximum length.
pub type FixedText<const N: usize> = Text<[u8; N]>;

/// Text backed by a `Vec<u8>`.
/// It is not null-terminated and has no maximum length.
pub type VecText = Text<Vec<u8>>;

/// A raw localized string which uses either Latin-1 or SHIFT-JIS encoding.
///
/// The type of data determines how the string is stored as raw bytes:
///
/// - `Vec<u8>` is not null-terminated and has no maximum length.
/// - `CString` is null-terminated and has no maximum length.
/// - `[u8; N]` is null-terminated and has a maximum length.
///
/// The data format is responsible for determining the length of a string without NULs. Except in
/// the case of arrays, this information is usually available in the inner type, so `Text` does not
/// track the length itself. Requesting the non-NUL bytes in an array requires a scan.
#[derive(Copy, Clone, Hash, Eq)]
pub struct Text<D: TextData>(D);

impl<D: TextData> Text<D> {
    /// The maximum text size in bytes, excluding any NULs.
    pub const MAX_LEN: usize = D::MAX_LEN;

    /// Constructs a new `Text` with the given data.
    /// This will fail if the data is invalid for usage as text.
    pub fn new(data: D) -> Result<Self> {
        // Go to/from raw bytes to validate the data
        Ok(Self(D::from_raw_bytes(data.into_raw_bytes())?))
    }

    /// Constructs a new `Text` from a byte string. The bytes must not contain any NULs.
    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Result<Self> {
        Ok(Self(D::from_bytes_without_nul(bytes.into())?))
    }

    /// Returns a slice over the bytes in the text up to any NUL.
    /// This may need to iterate over the bytes to find the endpoint.
    pub fn to_bytes(&self) -> &[u8] {
        self.0.as_bytes_without_nul()
    }

    /// Returns a slice over the bytes in the text, including any NULs.
    pub fn as_raw_bytes(&self) -> &[u8] {
        self.0.as_raw_bytes()
    }

    /// Consumes the text and returns the inner bytes up to any NUL.
    /// This may need to iterate over the bytes to find the endpoint.
    pub fn into_bytes(self) -> Vec<u8> {
        self.0.into_bytes_without_nul()
    }

    /// Consumes the text and returns the inner bytes, including any NULs.
    pub fn into_raw_bytes(self) -> Vec<u8> {
        self.0.into_raw_bytes()
    }

    /// Returns whether the text is an empty string.
    pub fn is_empty(&self) -> bool {
        self.as_raw_bytes().first().copied().unwrap_or(0) == 0
    }

    /// Returns the maximum text size in bytes, excluding any null terminators.
    ///
    /// For example, a `Text<[u8; 32]>` will return 31 because there always needs to be a null
    /// terminator. An unbounded string like `Text<Vec<u8>>` will return `usize::MAX`.
    ///
    /// This is a convenience accessor for `MAX_LEN`.
    pub const fn max_len(&self) -> usize {
        Self::MAX_LEN
    }

    /// Clears the text.
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Returns an iterator over the bytes in the text up to any NUL.
    pub fn iter(&self) -> impl Iterator<Item = u8> + '_ {
        self.as_raw_bytes().iter().copied().take_while(|x| *x != 0)
    }

    /// Decodes the text into a UTF-8 string.
    pub fn decode(&self) -> Result<Cow<'_, str>> {
        // If we want to be technically correct here, we should decode based on the region the game
        // is using because that's how it will display on the console. However this is not ideal in
        // practice because the dev stages still have a lot of debug text in Japanese, and most
        // `PrintF` commands are also in Japanese. As a compromise, it seems to be sufficient to try
        // and decode as SHIFT-JIS first (which works for most messages) and then fall back on
        // Windows-1252 if that fails.
        let bytes = self.to_bytes();
        match SHIFT_JIS.decode_without_bom_handling_and_without_replacement(bytes) {
            Some(s) => Ok(s),
            None => WINDOWS_1252
                .decode_without_bom_handling_and_without_replacement(bytes)
                .ok_or(Error::Decode),
        }
    }

    /// Decodes the text into a UTF-8 string and replaces invalid characters so this cannot fail.
    pub fn decode_replacing(&self) -> Cow<'_, str> {
        let bytes = self.to_bytes();
        match SHIFT_JIS.decode_without_bom_handling_and_without_replacement(bytes) {
            Some(s) => s,
            None => WINDOWS_1252.decode_without_bom_handling(bytes).0,
        }
    }

    /// Encodes a UTF-8 string as text. This will fail if the string cannot be encoded or if the
    /// string cannot fit the data format.
    pub fn encode(string: &str) -> Result<Self> {
        Self::do_encode(string, Self::MAX_LEN, false)
    }

    /// Encodes a UTF-8 string as text, truncating the output if the string cannot fit the data
    /// format. This will fail if the string cannot be encoded.
    pub fn encode_truncated(string: &str) -> Result<Self> {
        Self::do_encode(string, Self::MAX_LEN, true)
    }

    /// Encodes a UTF-8 string as text, truncating the output if the string cannot fit the requested
    /// maximum length in bytes. This will fail if the string cannot be encoded.
    pub fn encode_truncated_to(string: &str, max_len: usize) -> Result<Self> {
        Self::do_encode(string, max_len, true)
    }

    fn do_encode(string: &str, max_len: usize, truncate: bool) -> Result<Self> {
        let mut jis_encoder = SHIFT_JIS.new_encoder();
        let buffer_size = jis_encoder
            .max_buffer_length_from_utf8_without_replacement(string.len())
            .unwrap_or(usize::MAX)
            .min(max_len);
        let mut buffer = vec![0u8; buffer_size];
        let (mut status, mut _num_read, mut num_written) =
            jis_encoder.encode_from_utf8_without_replacement(string, &mut buffer, true);
        if let EncoderResult::Unmappable(_) = status {
            (status, _num_read, num_written) = WINDOWS_1252
                .new_encoder()
                .encode_from_utf8_without_replacement(string, &mut buffer, true);
        }
        match status {
            EncoderResult::InputEmpty => (),
            EncoderResult::OutputFull if truncate => (),
            EncoderResult::OutputFull => return Err(Error::TooLong(max_len)),
            EncoderResult::Unmappable(_) => return Err(Error::Encode(string.to_owned())),
        }
        buffer.truncate(num_written);
        Self::from_bytes(buffer)
    }

    /// Attempts to convert the underlying data format.
    pub fn convert<T: TextData>(self) -> Result<Text<T>> {
        Text::from_bytes(self.into_bytes())
    }
}

impl<D: TextData> PartialEq for Text<D> {
    fn eq(&self, other: &Self) -> bool {
        self.as_raw_bytes() == other.as_raw_bytes()
    }
}

impl<D: TextData> From<Text<D>> for CString {
    fn from(text: Text<D>) -> Self {
        CString::new(text.into_bytes()).unwrap()
    }
}

impl<D: TextData> TryFrom<Vec<u8>> for Text<D> {
    type Error = Error;
    fn try_from(bytes: Vec<u8>) -> Result<Self> {
        Self::from_bytes(bytes)
    }
}

impl<D: TextData> TryFrom<Box<[u8]>> for Text<D> {
    type Error = Error;
    fn try_from(bytes: Box<[u8]>) -> Result<Self> {
        Self::from_bytes(bytes)
    }
}

impl<D: TextData> TryFrom<CString> for Text<D> {
    type Error = Error;
    fn try_from(cstr: CString) -> Result<Self> {
        Self::from_bytes(cstr)
    }
}

impl<D: TextData> TryFrom<&[u8]> for Text<D> {
    type Error = Error;
    fn try_from(bytes: &[u8]) -> Result<Self> {
        Self::from_bytes(bytes)
    }
}

impl<D: TextData> Default for Text<D> {
    fn default() -> Self {
        Self(D::empty())
    }
}

impl<D: TextData + Extend<u8>> Text<D> {
    /// Appends a raw byte onto the end of the text if it is not NUL.
    pub fn push(&mut self, b: u8) -> Result<()> {
        if b != 0 {
            self.0.extend(iter::once(b));
            Ok(())
        } else {
            Err(Error::UnexpectedNul)
        }
    }
}

impl<D: TextData + Extend<u8>> Extend<u8> for Text<D> {
    fn extend<T: IntoIterator<Item = u8>>(&mut self, iter: T) {
        self.0.extend(
            iter.into_iter().inspect(|b| assert_ne!(*b, 0, "text cannot have interior NULs")),
        );
    }
}

impl<'a, D: TextData + Extend<u8>> Extend<&'a u8> for Text<D> {
    fn extend<T: IntoIterator<Item = &'a u8>>(&mut self, iter: T) {
        self.extend(iter.into_iter().copied())
    }
}

impl<D: TextData> fmt::Debug for Text<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let unicode = self.decode_replacing();
        // Replace ideographic spaces with regular spaces so messages look nicer in debug output
        let unicode = unicode.replace('\u{3000}', " ");
        write!(f, "\"{}\"", unicode.escape_debug())
    }
}

impl<D: TextData> IntoIterator for Text<D> {
    type Item = u8;
    type IntoIter = <Vec<u8> as IntoIterator>::IntoIter;
    fn into_iter(self) -> Self::IntoIter {
        self.into_bytes().into_iter()
    }
}

impl<R: Read + ?Sized, D: TextData + ReadFrom<R>> ReadFrom<R> for Text<D>
where
    D::Error: Into<Box<dyn StdError + Send + Sync>>,
{
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        Self::new(D::read_from(reader).map_err(|e| Error::Other(e.into()))?)
    }
}

impl<W: Write + ?Sized, D: TextData + WriteTo<W>> WriteTo<W> for Text<D> {
    type Error = D::Error;
    fn write_to(&self, writer: &mut W) -> StdResult<(), Self::Error> {
        self.0.write_to(writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vec() {
        let text = Text::<Vec<u8>>::default();
        assert!(text.is_empty());
        assert_eq!(text.to_bytes().len(), 0);
        assert_eq!(text.as_raw_bytes().len(), 0);
        assert_eq!(text.max_len(), usize::MAX);

        let raw = b"Hello, world!".to_vec();
        let text = Text::<Vec<u8>>::new(raw).unwrap();
        assert!(!text.is_empty());
        assert_eq!(text.to_bytes().len(), 13);
        assert_eq!(text.as_raw_bytes().len(), 13);
        assert_eq!(text.clone().into_bytes().len(), 13);
        assert_eq!(text.into_raw_bytes().len(), 13);

        let text = Text::<Vec<u8>>::from_bytes("Hello, world!").unwrap();
        assert!(!text.is_empty());
        assert_eq!(text.to_bytes().len(), 13);
        assert_eq!(text.as_raw_bytes().len(), 13);

        let text = Text::<Vec<u8>>::encode("Hello, world!").unwrap();
        assert!(!text.is_empty());
        assert_eq!(text.to_bytes().len(), 13);
        assert_eq!(text.as_raw_bytes().len(), 13);
        assert_eq!(text.decode().unwrap(), "Hello, world!");

        let result = Text::<Vec<u8>>::new(b"Hello, world!\0".to_vec());
        assert!(matches!(result, Err(Error::UnexpectedNul)));

        let result = Text::<Vec<u8>>::from_bytes(b"Hello, world!\0".to_vec());
        assert!(matches!(result, Err(Error::UnexpectedNul)));
    }

    #[test]
    fn test_array() {
        let text = Text::<[u8; 64]>::default();
        assert!(text.is_empty());
        assert_eq!(text.to_bytes().len(), 0);
        assert_eq!(text.as_raw_bytes().len(), 64);
        assert_eq!(text.max_len(), 63);

        let raw: [u8; 16] = *b"Hello, world!\0\0\0";
        let text = Text::<[u8; 16]>::new(raw).unwrap();
        assert_eq!(text.to_bytes().len(), 13);
        assert_eq!(text.as_raw_bytes().len(), 16);
        assert_eq!(text.clone().into_bytes().len(), 13);
        assert_eq!(text.into_raw_bytes().len(), 16);

        let text = Text::<[u8; 64]>::from_bytes("Hello, world!").unwrap();
        assert_eq!(text.to_bytes().len(), 13);
        assert_eq!(text.as_raw_bytes().len(), 64);

        let text = Text::<[u8; 64]>::encode("Hello, world!").unwrap();
        assert_eq!(text.to_bytes().len(), 13);
        assert_eq!(text.as_raw_bytes().len(), 64);
        assert_eq!(text.decode().unwrap(), "Hello, world!");

        let text = Text::<[u8; 4]>::encode("abc").unwrap();
        assert_eq!(text.to_bytes().len(), 3);
        assert_eq!(text.as_raw_bytes().len(), 4);
        assert_eq!(text.decode().unwrap(), "abc");

        let result = Text::<[u8; 4]>::encode("abcd");
        assert!(matches!(result, Err(Error::TooLong(3))));

        let text = Text::<[u8; 4]>::encode_truncated("abcd").unwrap();
        assert_eq!(text.to_bytes().len(), 3);
        assert_eq!(text.as_raw_bytes().len(), 4);
        assert_eq!(text.decode().unwrap(), "abc");

        let result = Text::<[u8; 64]>::from_bytes(b"Hello, world!\0".to_vec());
        assert!(matches!(result, Err(Error::UnexpectedNul)));

        let result = Text::<[u8; 4]>::new(*b"abcd");
        assert!(matches!(result, Err(Error::NoNulTerminator)));
    }

    #[test]
    fn test_cstring() {
        let text = Text::<CString>::default();
        assert!(text.is_empty());
        assert_eq!(text.to_bytes().len(), 0);
        assert_eq!(text.as_raw_bytes().len(), 1);
        assert_eq!(text.max_len(), usize::MAX);

        let raw = CString::new("Hello, world!").unwrap();
        let text = Text::<CString>::new(raw).unwrap();
        assert!(!text.is_empty());
        assert_eq!(text.to_bytes().len(), 13);
        assert_eq!(text.as_raw_bytes().len(), 14);
        assert_eq!(text.clone().into_bytes().len(), 13);
        assert_eq!(text.into_raw_bytes().len(), 14);

        let text = Text::<CString>::from_bytes("Hello, world!").unwrap();
        assert!(!text.is_empty());
        assert_eq!(text.to_bytes().len(), 13);
        assert_eq!(text.as_raw_bytes().len(), 14);
    }

    #[test]
    fn test_empty() {
        let text = Text::<Vec<u8>>::encode("").unwrap();
        assert!(text.is_empty());
        let text = Text::<[u8; 1]>::encode("").unwrap();
        assert!(text.is_empty());
        let text = Text::<Vec<u8>>::default();
        assert!(text.decode().unwrap().is_empty());
    }

    #[test]
    fn test_jis() {
        let s = "スプラトゥーン";
        let text = Text::<Vec<u8>>::encode(s).unwrap();
        assert_eq!(text.decode().unwrap(), s);
        let text = Text::<[u8; 13]>::encode_truncated(s).unwrap();
        assert_eq!(text.decode().unwrap(), "スプラトゥー");
        let text = Text::<[u8; 14]>::encode_truncated(s).unwrap();
        assert_eq!(text.decode().unwrap(), "スプラトゥー");

        // This triggers a weird edge case where the encoder needs 4 bytes instead of 3
        let s = "× ";
        let text = Text::<Vec<u8>>::encode(s).unwrap();
        assert_eq!(text.decode().unwrap(), s);
    }

    #[test]
    fn test_latin() {
        let s = "ááááá";
        let text = Text::<Vec<u8>>::encode(s).unwrap();
        assert_eq!(text.decode().unwrap(), s);
        let result = Text::<Vec<u8>>::encode("😳");
        assert!(matches!(result, Err(Error::Encode(_))));
    }

    #[test]
    fn test_mutable() {
        let mut text = Text::<Vec<u8>>::from_bytes("Hello, ").unwrap();
        assert!(text.push(b'w').is_ok());
        assert!(text.push(0).is_err());
        text.extend(b"orld!");
        assert_eq!(text.decode().unwrap(), "Hello, world!");
        text.clear();
        assert!(text.is_empty());
    }

    #[test]
    fn test_iter() {
        let text = Text::<[u8; 16]>::from_bytes("Hello, world!").unwrap();
        assert_eq!(text.iter().collect::<Vec<u8>>(), b"Hello, world!");
        assert_eq!(text.into_iter().collect::<Vec<u8>>(), b"Hello, world!");
    }

    #[test]
    #[should_panic]
    fn test_extend_with_nul() {
        let mut text = Text::<Vec<u8>>::default();
        text.extend(b"Hello,\0world!".to_vec());
    }

    #[test]
    #[should_panic]
    fn test_extend_ref_with_nul() {
        let mut text = Text::<Vec<u8>>::default();
        text.extend(b"Hello,\0world!");
    }

    #[test]
    fn test_read_from() {
        let mut bytes: &[u8] = b"Hello, world!\0";
        let text = Text::<CString>::read_from(&mut bytes).unwrap();
        assert_eq!(text.decode().unwrap(), "Hello, world!");
    }

    #[test]
    fn test_write_to() {
        let text = Text::<CString>::from_bytes("Hello, world!").unwrap();
        let mut bytes = vec![];
        text.write_to(&mut bytes).unwrap();
        assert_eq!(bytes, b"Hello, world!\0");
    }
}
