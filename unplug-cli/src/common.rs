use anyhow::{bail, Result};
use regex::RegexSet;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{self, Display};
use std::time::Duration;
use unicase::UniCase;
use unplug::dvd::{EntryId, FileTree};

/// Formats a duration using `MM:SS.mmm`.
pub fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let minutes = total_secs / 60;
    let secs = total_secs % 60;
    let millis = duration.subsec_millis();
    format!("{:>02}:{:>02}.{:>03}", minutes, secs, millis)
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

/// Characters which need to be escaped if they appear in a glob.
const SPECIAL_REGEX_CHARS: &str = r".+()|[]{}^$";

/// Converts a glob string into a regex that can match paths.
/// Supports the typical `*`, `**`, and `?` wildcards.
fn glob_to_regex(glob: &str) -> String {
    let mut regex = "(?i)^".to_owned(); // Case-insensitive
    let mut chars = glob.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '*' {
            if let Some('*') = chars.peek().copied() {
                // `**` - match any characters including slashes
                regex.push_str(r".*");
                chars.next();
                // Discard separators after `**`
                while let Some('\\') | Some('/') = chars.peek().copied() {
                    chars.next();
                }
            } else {
                // `*` - match any characters except slashes
                regex.push_str(r"[^/]*");
            }
        } else if ch == '?' {
            // Wildcard, match any single character except slashes
            regex.push_str(r"[^/]");
        } else if ch == '\\' || ch == '/' {
            // Normalize path separators
            regex.push('/');
            while let Some('\\') | Some('/') = chars.peek().copied() {
                chars.next();
            }
        } else if SPECIAL_REGEX_CHARS.contains(ch) {
            // Escape special characters
            regex.push('\\');
            regex.push(ch);
        } else {
            regex.push(ch);
        }
    }
    if !regex.ends_with('/') {
        // End on separator boundary
        regex.push_str(r"(/|$)");
    }
    regex
}

/// Filters paths based on glob expressions.
#[derive(Clone)]
pub struct PathFilter(RegexSet);

impl PathFilter {
    /// Compiles a set of glob expressions into a `PathFilter`. If no globs are provided, the filter
    /// matches all paths.
    pub fn new<I, S>(globs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let regexes = globs.into_iter().map(|g| glob_to_regex(g.as_ref()));
        Self(RegexSet::new(regexes).unwrap())
    }

    /// Returns a filter which matches all paths.
    pub fn all() -> Self {
        Self(RegexSet::empty())
    }

    /// Returns whether a path matches any glob in the filter.
    pub fn is_match(&self, path: &str) -> bool {
        self.0.is_empty() || self.0.is_match(path)
    }
}

impl Default for PathFilter {
    fn default() -> Self {
        Self::all()
    }
}

/// Scans a file tree for files and returns the list of paths and entry IDs. If `filter` is
/// provided, only paths matching the regex will be returned. If no files are found, this will
/// return an error.
pub fn find_files(tree: &FileTree, filter: PathFilter) -> Result<Vec<(String, EntryId)>> {
    let result = tree
        .recurse()
        .filter(|(p, e)| tree[*e].is_file() && filter.is_match(p))
        .collect::<Vec<_>>();
    if result.is_empty() {
        bail!("No files were found");
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_globbing() {
        let paths = &[
            "qp.bin",
            "qp/sfx_army.ssm",
            "qp/sfx_bb.ssm",
            "qp/streaming/bgm.hps",
            "qp/streaming/menu.hps",
        ];

        let check = |glob: &str, expected: &[&str]| {
            let filter = PathFilter::new(&[glob]);
            let actual = paths.iter().copied().filter(|p| filter.is_match(p)).collect::<Vec<_>>();
            assert_eq!(&actual, expected, "glob: {:?}", glob);
        };

        check("", &[]);
        check("q", &[]);
        check(
            "qp",
            &["qp/sfx_army.ssm", "qp/sfx_bb.ssm", "qp/streaming/bgm.hps", "qp/streaming/menu.hps"],
        );

        check("qp?", &[]);
        check("qp????", &["qp.bin"]);
        check("qp.bin", &["qp.bin"]);
        check("QP.bin", &["qp.bin"]);
        check("qp.bin/", &[]);

        check("qp/sfx_army.ssm", &["qp/sfx_army.ssm"]);
        check("qp\\sfx_army.ssm", &["qp/sfx_army.ssm"]);
        check("qp/\\/sfx_army.ssm", &["qp/sfx_army.ssm"]);
        check("qp/streaming", &["qp/streaming/bgm.hps", "qp/streaming/menu.hps"]);
        check("qp/streaming/", &["qp/streaming/bgm.hps", "qp/streaming/menu.hps"]);

        check("*in", &["qp.bin"]);
        check("*.in", &[]);
        check("*.bin", &["qp.bin"]);

        check("*.hps", &[]);
        check("**.hps", &["qp/streaming/bgm.hps", "qp/streaming/menu.hps"]);
        check("**/*.hps", &["qp/streaming/bgm.hps", "qp/streaming/menu.hps"]);
        check("**/\\/*.hps", &["qp/streaming/bgm.hps", "qp/streaming/menu.hps"]);
        check("*/*/*", &["qp/streaming/bgm.hps", "qp/streaming/menu.hps"]);
        check("qp/streaming/*.hps", &["qp/streaming/bgm.hps", "qp/streaming/menu.hps"]);

        check("**/*.bin", &["qp.bin"]);
        check("**/**/*.bin", &["qp.bin"]);

        check("*", paths);
        check("**", paths);
        check("**/*", paths);
        check("**/**", paths);

        let all = PathFilter::all();
        assert!(paths.iter().all(|p| all.is_match(p)));
    }
}
