use crate::dvd::{EntryId, FileTree};
use regex::RegexSet;

/// Globbing modes.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum GlobMode {
    /// The glob should not include the contents of matching directories.
    Exact,
    /// The glob should include the contents of matching directories.
    Prefix,
}

/// Characters which need to be escaped if they appear in a glob.
const SPECIAL_REGEX_CHARS: &str = r".+()|[]{}^$";

/// Returns whether `ch` is a path separator.
fn is_separator(ch: char) -> bool {
    ch == '/' || ch == '\\'
}

/// Converts a glob string into a regex that can match paths.
/// Supports the typical `*`, `**`, and `?` wildcards.
fn glob_to_regex(glob: &str, mode: GlobMode) -> String {
    let mut regex = "(?i)^/".to_owned(); // Case-insensitive
    let mut chars = glob.chars().peekable();
    let mut is_first = true; // true if no characters have been processed yet
    let mut is_dir = true; // true if the regex currently ends with a separator
    while let Some(ch) = chars.next() {
        if ch == '*' {
            if chars.peek().copied() == Some('*') {
                // `**` - match any characters including slashes
                regex.push('.');
                chars.next();
            } else {
                // `*` - match any characters except slashes
                regex.push_str(r"[^/]");
            }
            // Do not match on the directory before this component
            regex.push(if is_dir { '+' } else { '*' });
        } else if ch == '?' {
            // Wildcard, match any single character except slashes
            regex.push_str(r"[^/]");
        } else if is_separator(ch) {
            // Ignore separators at the beginning
            if !is_first {
                regex.push('/');
            }
            // Normalize path separators
            while matches!(chars.peek().copied(), Some(ch) if is_separator(ch)) {
                chars.next();
            }
        } else if SPECIAL_REGEX_CHARS.contains(ch) {
            // Escape special characters
            regex.push('\\');
            regex.push(ch);
        } else {
            regex.push(ch);
        }
        is_dir = is_separator(ch);
        is_first = false;
    }

    // If the pattern ends with a separator, it must match a directory, otherwise it could match
    // either a file or a directory
    let is_dir = regex.ends_with('/');
    match mode {
        GlobMode::Exact => {
            // The path must end here, possibly with a separator
            regex.push_str(if is_dir { r"$" } else { r"/?$" });
        }
        GlobMode::Prefix => {
            if !is_dir {
                // The path must either be a child or end here
                regex.push_str(r"(/|$)");
            }
        }
    }
    regex
}

/// Filters paths based on glob expressions.
#[derive(Clone)]
pub struct Glob(RegexSet);

impl Glob {
    /// Compiles a set of glob expressions into a `Glob`. If no globs are provided, the glob matches
    /// all paths.
    pub fn new<I, S>(mode: GlobMode, globs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let regexes = globs.into_iter().map(|g| glob_to_regex(g.as_ref(), mode));
        Self(RegexSet::new(regexes).unwrap())
    }

    /// Returns a glob which matches all paths.
    pub fn all() -> Self {
        Self(RegexSet::empty())
    }

    /// Returns whether a path matches the glob.
    pub fn is_match(&self, path: &str) -> bool {
        self.0.is_empty() || self.0.is_match(path)
    }

    /// Returns an iterator over the files in a tree which match the glob.
    pub fn find<'t, 's: 't>(
        &'s self,
        tree: &'t FileTree,
    ) -> impl Iterator<Item = (String, EntryId)> + 't {
        tree.recurse().filter(|(p, _)| self.is_match(p))
    }
}

impl Default for Glob {
    fn default() -> Self {
        Self::all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dvd::{DirectoryEntry, Entry, FileEntry};
    use lazy_static::lazy_static;

    fn file(name: &str) -> Entry {
        FileEntry::new(name, 1, 2).into()
    }

    fn dir(name: &str) -> Entry {
        DirectoryEntry::new(name).into()
    }

    lazy_static! {
        static ref TEST_TREE: FileTree = {
            let mut files = FileTree::new();
            files.insert(files.root(), file("qp.bin"));
            let qp = files.insert(files.root(), dir("qp"));
            files.insert(qp, file("sfx_army.ssm"));
            files.insert(qp, file("sfx_bb.ssm"));
            let streaming = files.insert(qp, dir("streaming"));
            files.insert(streaming, file("bgm.hps"));
            files.insert(streaming, file("menu.hps"));
            files
        };
    }

    const ALL_PATHS: &[&str] = &[
        "/",
        "/qp.bin",
        "/qp/",
        "/qp/sfx_army.ssm",
        "/qp/sfx_bb.ssm",
        "/qp/streaming/",
        "/qp/streaming/bgm.hps",
        "/qp/streaming/menu.hps",
    ];

    const QP_PATHS: &[&str] = &[
        "/qp/",
        "/qp/sfx_army.ssm",
        "/qp/sfx_bb.ssm",
        "/qp/streaming/",
        "/qp/streaming/bgm.hps",
        "/qp/streaming/menu.hps",
    ];

    fn do_glob(mode: GlobMode, glob_str: &str) -> Vec<String> {
        let glob = Glob::new(mode, [glob_str]);
        glob.find(&TEST_TREE).map(|(p, _)| p).collect()
    }

    fn do_common_globs(mode: GlobMode) {
        let glob = |glob_str| do_glob(mode, glob_str);

        assert!(glob("q").is_empty());
        assert!(glob("qp?").is_empty());
        assert_eq!(glob("qp????"), &["/qp.bin"]);
        assert_eq!(glob("qp.bin"), &["/qp.bin"]);
        assert_eq!(glob("/qp.bin"), &["/qp.bin"]);
        assert_eq!(glob("QP.bin"), &["/qp.bin"]);
        assert!(glob("qp.bin/").is_empty());

        assert_eq!(glob("qp/sfx_army.ssm"), &["/qp/sfx_army.ssm"]);
        assert_eq!(glob("qp\\sfx_army.ssm"), &["/qp/sfx_army.ssm"]);
        assert_eq!(glob("qp/\\/sfx_army.ssm"), &["/qp/sfx_army.ssm"]);
        assert_eq!(glob("/qp/\\/sfx_army.ssm"), &["/qp/sfx_army.ssm"]);
        assert_eq!(glob("/\\/qp/\\/sfx_army.ssm"), &["/qp/sfx_army.ssm"]);

        assert_eq!(glob("*in"), &["/qp.bin"]);
        assert!(glob("*.in").is_empty());
        assert_eq!(glob("*.bin"), &["/qp.bin"]);

        assert_eq!(glob("**.bin"), &["/qp.bin"]);
        assert!(glob("**/*.bin").is_empty());

        assert!(glob("*.hps").is_empty());
        assert_eq!(glob("**.hps"), &["/qp/streaming/bgm.hps", "/qp/streaming/menu.hps"]);
        assert_eq!(glob("**/*.hps"), &["/qp/streaming/bgm.hps", "/qp/streaming/menu.hps"]);
        assert_eq!(glob("**/\\/*.hps"), &["/qp/streaming/bgm.hps", "/qp/streaming/menu.hps"]);
        assert_eq!(glob("*/*/*"), &["/qp/streaming/bgm.hps", "/qp/streaming/menu.hps"]);
        assert_eq!(
            glob("qp/streaming/*.hps"),
            &["/qp/streaming/bgm.hps", "/qp/streaming/menu.hps"]
        );
    }

    #[test]
    fn test_glob_exact() {
        let glob = |glob_str| do_glob(GlobMode::Exact, glob_str);
        do_common_globs(GlobMode::Exact);

        assert_eq!(glob(""), &["/"]);
        assert_eq!(glob("/"), &["/"]);
        assert_eq!(glob("*"), &["/qp.bin", "/qp/"]);
        assert_eq!(glob("**"), &ALL_PATHS[1..]);
        assert_eq!(glob("**/"), &["/qp/", "/qp/streaming/"]);
        assert_eq!(glob("**/*"), &QP_PATHS[1..]);
        assert_eq!(glob("**/**"), &QP_PATHS[1..]);
        assert_eq!(glob("qp"), &["/qp/"]);
        assert_eq!(glob("qp/"), &["/qp/"]);
        assert_eq!(glob("qp/streaming"), &["/qp/streaming/"]);
        assert_eq!(glob("qp/streaming/"), &["/qp/streaming/"]);
    }

    #[test]
    fn test_glob_prefix() {
        let glob = |glob_str| do_glob(GlobMode::Prefix, glob_str);
        do_common_globs(GlobMode::Prefix);

        assert_eq!(glob(""), ALL_PATHS);
        assert_eq!(glob("/"), ALL_PATHS);
        assert_eq!(glob("*"), &ALL_PATHS[1..]);
        assert_eq!(glob("**"), &ALL_PATHS[1..]);
        assert_eq!(glob("**/"), QP_PATHS);
        assert_eq!(glob("**/*"), &QP_PATHS[1..]);
        assert_eq!(glob("**/**"), &QP_PATHS[1..]);
        assert_eq!(glob("qp"), QP_PATHS);
        assert_eq!(glob("qp/"), QP_PATHS);
        assert_eq!(
            glob("qp/streaming"),
            &["/qp/streaming/", "/qp/streaming/bgm.hps", "/qp/streaming/menu.hps"],
        );
        assert_eq!(
            glob("qp/streaming/"),
            &["/qp/streaming/", "/qp/streaming/bgm.hps", "/qp/streaming/menu.hps"],
        );
    }
}
