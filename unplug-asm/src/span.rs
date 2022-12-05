use num_traits::ToPrimitive;
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
    pub const EMPTY: Self = Self::new(0, 0);

    /// Creates a new span beginning at `start` and ending at `end` (exclusive).
    pub const fn new(start: SourceOffset, end: SourceOffset) -> Self {
        debug_assert!(start <= end);
        Self { start, end }
    }

    /// Creates a new span from a range of source offsets.
    pub const fn from_range(range: Range<SourceOffset>) -> Self {
        Self { start: range.start, end: range.end }
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

impl chumsky::Span for Span {
    type Context = ();
    type Offset = SourceOffset;

    fn new(_context: Self::Context, range: Range<Self::Offset>) -> Self {
        Self::from_range(range)
    }

    fn context(&self) -> Self::Context {}

    fn start(&self) -> Self::Offset {
        self.start
    }

    fn end(&self) -> Self::Offset {
        self.end
    }
}

impl<I: ToPrimitive> TryFrom<Range<I>> for Span {
    type Error = ();
    fn try_from(value: Range<I>) -> Result<Self, Self::Error> {
        Ok(Self { start: value.start.to_u32().ok_or(())?, end: value.end.to_u32().ok_or(())? })
    }
}

/// Trait for a node which knows its span in the source code.
pub trait Spanned {
    /// Returns the node's span.
    fn span(&self) -> Span;
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
}
