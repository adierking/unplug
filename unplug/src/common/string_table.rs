use std::ffi::CStr;
use thiserror::Error;

/// The result type for string table operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The error type for string table operations.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    #[error("missing null terminator")]
    MissingNullTerminator,

    #[error("invalid offset: {0:#x}")]
    InvalidOffset(u32),
}

/// A table of null-terminated strings.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct StringTable {
    bytes: Vec<u8>,
}

impl StringTable {
    /// Constructs an empty `StringTable`.
    pub fn new() -> Self {
        Self { bytes: vec![] }
    }

    /// Constructs a `StringTable` from a set of bytes.
    /// This will fail if the last string does not end with a null terminator.
    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Result<Self> {
        let bytes = bytes.into();
        if bytes.is_empty() || *bytes.last().unwrap() == 0 {
            Ok(Self { bytes })
        } else {
            Err(Error::MissingNullTerminator)
        }
    }

    /// Gets a reference to the bytes in the string table.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the string at `offset`.
    pub fn at(&self, offset: u32) -> Result<&CStr> {
        let start = offset as usize;
        if start < self.bytes.len() {
            let len = self.bytes[start..].iter().position(|b| *b == 0).unwrap();
            Ok(CStr::from_bytes_with_nul(&self.bytes[start..=(start + len)]).unwrap())
        } else {
            Err(Error::InvalidOffset(offset))
        }
    }

    /// Adds a string onto the end of the table and returns its offset.
    pub fn push(&mut self, string: impl AsRef<CStr>) -> u64 {
        let offset = self.bytes.len() as u64;
        self.bytes.extend(string.as_ref().to_bytes_with_nul());
        offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn test_at() -> Result<()> {
        let strings = StringTable::from_bytes(vec![b'a', 0, b'b', b'c', 0, b'd', b'e', b'f', 0])?;
        assert_eq!(strings.at(0)?.to_str().unwrap(), "a");
        assert_eq!(strings.at(2)?.to_str().unwrap(), "bc");
        assert_eq!(strings.at(5)?.to_str().unwrap(), "def");
        assert_eq!(strings.at(6)?.to_str().unwrap(), "ef");
        assert_eq!(strings.at(8)?.to_str().unwrap(), "");
        assert!(strings.at(9).is_err());
        Ok(())
    }

    #[test]
    fn test_push() -> Result<()> {
        let mut strings = StringTable::new();
        assert_eq!(strings.push(CString::new("").unwrap()), 0);
        assert_eq!(strings.push(CString::new("a").unwrap()), 1);
        assert_eq!(strings.push(CString::new("bc").unwrap()), 3);
        assert_eq!(strings.push(CString::new("def").unwrap()), 6);
        Ok(())
    }
}
