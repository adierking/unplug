use num_traits::{FromPrimitive, ToPrimitive};
use std::fmt::{self, Debug, Formatter};
use std::ops::Range;

/// A byte offset in source code.
pub type SourceOffset = u32;

/// Represents a range of bytes in source code.
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Span {
    start: SourceOffset,
    end: SourceOffset,
}

impl Span {
    /// A span which is empty.
    pub const EMPTY: Self = Self { start: 0, end: 0 };

    /// Creates a new span beginning at `start` and ending at `end` (exclusive).
    pub fn new(start: SourceOffset, end: SourceOffset) -> Self {
        debug_assert!(start <= end, "start = {start}, end = {end}");
        Self { start, end }
    }

    /// Creates a new span from a range of source offsets.
    pub fn from_range(range: Range<SourceOffset>) -> Self {
        Self { start: range.start, end: range.end.max(range.start) }
    }

    /// Returns the start offset of the span.
    pub const fn start(self) -> SourceOffset {
        self.start
    }

    /// Returns the (exclusive) end offset of the span.
    pub const fn end(self) -> SourceOffset {
        self.end
    }

    /// Returns the length of the span.
    pub const fn len(self) -> SourceOffset {
        self.end - self.start
    }

    /// Returns whether the span is empty.
    pub const fn is_empty(self) -> bool {
        self.len() == 0
    }

    /// Returns a new span with length `len` and the same starting point.
    pub fn with_len(self, len: u32) -> Self {
        let end = self.start.checked_add(len).expect("span length overflow");
        Self { start: self.start, end }
    }

    /// Returns a new span of length `len` starting at the end of this span.
    pub fn at_end(self, len: u32) -> Self {
        Span::new(self.end, self.end).with_len(len)
    }

    /// Joins this span with another span. Empty spans are ignored.
    pub fn join(self, other: Self) -> Self {
        if self.is_empty() {
            other
        } else if other.is_empty() {
            self
        } else {
            Self { start: self.start.min(other.start), end: self.end.max(other.end) }
        }
    }
}

impl Debug for Span {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

impl<I: ToPrimitive> TryFrom<Range<I>> for Span {
    type Error = ();
    fn try_from(r: Range<I>) -> Result<Self, Self::Error> {
        let start = r.start.to_u32().ok_or(())?;
        let end = r.end.to_u32().ok_or(())?;
        Ok(Self { start, end })
    }
}

impl<I: FromPrimitive> TryFrom<Span> for Range<I> {
    type Error = ();
    fn try_from(s: Span) -> Result<Self, Self::Error> {
        let start = I::from_u32(s.start).ok_or(())?;
        let end = I::from_u32(s.end).ok_or(())?;
        Ok(start..end)
    }
}

/// Trait for a node which knows its span in the source code.
pub trait Spanned {
    /// Returns the node's span.
    fn span(&self) -> Span;
}

impl Spanned for Span {
    fn span(&self) -> Span {
        *self
    }
}

impl<T: Spanned> Spanned for &T {
    fn span(&self) -> Span {
        (*self).span()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic() {
        let s = Span::new(5, 10);
        assert_eq!(s.start(), 5);
        assert_eq!(s.end(), 10);
        assert_eq!(s.len(), 5);
        assert!(!s.is_empty());
    }

    #[test]
    fn test_empty() {
        let s = Span::EMPTY;
        assert_eq!(s.len(), 0);
        assert!(s.is_empty());
    }

    #[test]
    fn test_join() {
        assert_eq!(Span::new(5, 10).join(Span::new(4, 5)), Span::new(4, 10));
        assert_eq!(Span::new(5, 10).join(Span::new(4, 8)), Span::new(4, 10));
        assert_eq!(Span::new(5, 10).join(Span::new(10, 11)), Span::new(5, 11));
        assert_eq!(Span::new(5, 10).join(Span::new(8, 11)), Span::new(5, 11));
        assert_eq!(Span::new(5, 6).join(Span::new(9, 10)), Span::new(5, 10));
        assert_eq!(Span::new(5, 6).join(Span::EMPTY), Span::new(5, 6));
        assert_eq!(Span::EMPTY.join(Span::new(5, 6)), Span::new(5, 6));
        assert_eq!(Span::EMPTY.join(Span::EMPTY), Span::EMPTY);
    }

    #[test]
    fn test_with_len() {
        assert_eq!(Span::new(5, 10).with_len(6), Span::new(5, 11));
        assert_eq!(Span::new(5, 10).with_len(1), Span::new(5, 6));
        assert_eq!(Span::new(5, 10).with_len(0), Span::new(5, 5));
    }

    #[test]
    fn test_at_end() {
        assert_eq!(Span::new(5, 10).at_end(2), Span::new(10, 12));
        assert_eq!(Span::new(5, 10).at_end(0), Span::new(10, 10));
    }
}
