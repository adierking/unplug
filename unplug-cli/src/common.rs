use anyhow::Result;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{self, Display};
use std::path::Path;
use std::time::Duration;
use unicase::UniCase;

/// Formats a duration using `MM:SS.mmm`.
pub fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let minutes = total_secs / 60;
    let secs = total_secs % 60;
    let millis = duration.subsec_millis();
    format!("{:>02}:{:>02}.{:>03}", minutes, secs, millis)
}

/// Takes an output path passed to a command along with whether multiple items need to be written to
/// the output, and returns a (dir, name) pair of the output directory and filename. The name will
/// be `None` if the output should be treated as a directory.
pub fn output_dir_and_name(output: Option<&Path>, multiple_items: bool) -> (&Path, Option<String>) {
    match output {
        Some(output) => {
            // The output is always treated as a directory if there are multiple items to write
            if multiple_items || output.is_dir() {
                (output, None)
            } else {
                let dir = output.parent().unwrap_or_else(|| Path::new("."));
                let name = output.file_name().map(|n| n.to_string_lossy().into_owned());
                (dir, name)
            }
        }
        None => (Path::new("."), None),
    }
}

/// A case-insensitive string wrapper with support for serde.
#[derive(Debug, Default, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct IString(UniCase<String>);

impl IString {
    /// Creates a new `IString` which takes ownership of `s`.
    pub fn new(s: impl Into<String>) -> Self {
        Self(UniCase::new(s.into()))
    }

    /// Returns whether this string is equal to `s` if case is ignored.
    pub fn matches(&self, s: impl AsRef<str>) -> bool {
        self.0 == UniCase::unicode(s)
    }

    /// Returns the underlying string reference.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Consumes the `IString` and returns the inner `String`.
    pub fn into_string(self) -> String {
        self.0.into_inner()
    }
}

impl AsRef<str> for IString {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl From<&str> for IString {
    fn from(s: &str) -> Self {
        Self::new(s.to_owned())
    }
}

impl From<String> for IString {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<IString> for String {
    fn from(s: IString) -> Self {
        s.into_string()
    }
}

impl Display for IString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_str().fmt(f)
    }
}

impl Serialize for IString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for IString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = IString;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a string")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(v.into())
            }

            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(v.into())
            }
        }
        deserializer.deserialize_string(Visitor)
    }
}
