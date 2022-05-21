use encoding_rs::{SHIFT_JIS, WINDOWS_1252};
use std::borrow::Cow;
use std::ffi::CString;
use std::fmt;
use thiserror::Error;

/// The result type for text operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for text operations.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("text could not be encoded with SHIFT-JIS or Windows-1252: {0:?}")]
    Encode(String),

    #[error("text could not be decoded with SHIFT-JIS or Windows-1252")]
    Decode,
}

/// A raw localized string.
#[derive(Clone, Default, Hash, PartialEq, Eq)]
pub struct Text {
    bytes: Vec<u8>,
}

impl Text {
    /// Constructs an empty `Text`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Constructs a `Text` from a raw byte string.
    pub fn with_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        let mut text = Self { bytes: bytes.into() };
        text.bytes.shrink_to_fit();
        text
    }

    /// Returns a slice over the bytes in the text.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns a mutable slice over the bytes in the text.
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        &mut self.bytes
    }

    /// Consumes the text and returns the inner bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// Consumes the text and converts it to a `CString`.
    pub fn into_c_string(self) -> CString {
        CString::new(self.into_bytes()).unwrap()
    }

    /// Returns whether the text is an empty string.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Constructs a `Text` by encoding a UTF-8 string.
    pub fn encode(string: &str) -> Result<Self> {
        let (bytes, _, unmappable) = SHIFT_JIS.encode(string);
        if unmappable {
            let (bytes, _, unmappable) = WINDOWS_1252.encode(string);
            match unmappable {
                false => Ok(Self { bytes: bytes.into() }),
                true => Err(Error::Encode(string.to_owned())),
            }
        } else {
            Ok(Self { bytes: bytes.into() })
        }
    }

    /// Decodes the `Text` as a UTF-8 string.
    pub fn decode(&self) -> Result<Cow<'_, str>> {
        // If we want to be technically correct here, we should decode based on the region the game
        // is using because that's how it will display on the console. However this is not ideal in
        // practice because the dev stages still have a lot of debug text in Japanese, and most
        // `PrintF` commands are also in Japanese. As a compromise, it seems to be sufficient to try
        // and decode as SHIFT-JIS first (which works for most messages) and then fall back on
        // Windows-1252 if that fails.
        match SHIFT_JIS.decode_without_bom_handling_and_without_replacement(&self.bytes) {
            Some(s) => Ok(s),
            None => WINDOWS_1252
                .decode_without_bom_handling_and_without_replacement(&self.bytes)
                .ok_or(Error::Decode),
        }
    }
}

impl fmt::Debug for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (unicode, _) = SHIFT_JIS.decode_without_bom_handling(&self.bytes);
        // Replace ideographic spaces with regular spaces so messages look nicer in debug output
        let unicode = unicode.replace('\u{3000}', " ");
        write!(f, "\"{}\"", unicode.escape_debug())
    }
}

impl From<CString> for Text {
    fn from(string: CString) -> Self {
        Self::with_bytes(string.into_bytes())
    }
}

impl From<Text> for CString {
    fn from(text: Text) -> Self {
        text.into_c_string()
    }
}
