use std::num::NonZeroU64;
use std::sync::Arc;

/// A marked point or range in an audio stream.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Cue {
    /// The index of the sample frame where the cue starts.
    pub start: u64,
    /// The cue's type and type-specific data.
    pub kind: CueKind,
    /// The cue's name.
    pub name: Arc<str>,
}

/// Describes how a cue is used.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CueKind {
    /// The cue is a single point in the audio stream.
    Point,
    /// The cue indicates the start of the looped section.
    Loop,
    /// The cue covers a range of samples.
    Range(NonZeroU64),
}

impl Cue {
    /// Creates a new `Cue` which is a simple point.
    pub fn new(name: impl Into<Arc<str>>, start: u64) -> Self {
        Self { start, kind: CueKind::Point, name: name.into() }
    }

    /// Creates a new `Cue` which defines a loop point.
    pub fn new_loop(name: impl Into<Arc<str>>, start: u64) -> Self {
        Self { start, kind: CueKind::Loop, name: name.into() }
    }

    /// Creates a new range `Cue` with a duration. ***Panics*** if the duration is zero.
    pub fn new_range(name: impl Into<Arc<str>>, start: u64, duration: u64) -> Self {
        let duration = NonZeroU64::new(duration).expect("zero-duration range cue");
        Self { start, kind: CueKind::Range(duration), name: name.into() }
    }

    /// Returns the duration of the cue. This will be 0 for non-range cues.
    pub fn duration(&self) -> u64 {
        match self.kind {
            CueKind::Point | CueKind::Loop => 0,
            CueKind::Range(duration) => duration.get(),
        }
    }

    /// Returns true if this cue is a simple point. This does not include loop points.
    pub fn is_simple(&self) -> bool {
        matches!(self.kind, CueKind::Point)
    }

    /// Returns true if this cue is a loop point.
    pub fn is_loop(&self) -> bool {
        matches!(self.kind, CueKind::Loop)
    }

    /// Returns true if this cue is a range.
    pub fn is_range(&self) -> bool {
        matches!(self.kind, CueKind::Range(_))
    }
}

impl Default for Cue {
    fn default() -> Self {
        Self { start: 0, kind: CueKind::Point, name: "".into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cue_ordering() {
        // Cues MUST be ordered by start, then kind, then name
        let mut cues = vec![
            Cue::new("b", 1),
            Cue::new("a", 0),
            Cue::new_loop("e", 1),
            Cue::new_range("f", 1, 1),
            Cue::new_range("d", 1, 2),
            Cue::new("c", 1),
            Cue::new("g", 2),
        ];
        cues.sort_unstable();
        assert_eq!(
            cues,
            &[
                Cue::new("a", 0),
                Cue::new("b", 1),
                Cue::new("c", 1),
                Cue::new_loop("e", 1),
                Cue::new_range("f", 1, 1),
                Cue::new_range("d", 1, 2),
                Cue::new("g", 2),
            ]
        );
    }
}
