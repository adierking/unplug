use crate::private::Sealed;
use num_traits::{AsPrimitive, FromPrimitive, NumAssignOps, One, PrimInt, Zero};
use std::hash::Hash;
use std::iter::FusedIterator;

/// A type of resource ID.
pub trait Resource: Sealed + Copy + Eq + Hash + Ord + Sized {
    /// The ID type as an integer.
    type Value: PrimInt
        + NumAssignOps
        + AsPrimitive<usize>
        + FromPrimitive
        + From<Self>
        + TryInto<Self>;

    /// The total number of this type of resource.
    const COUNT: usize;

    /// Retrieves the resource corresponding to an index in the range `[0, COUNT)`.
    /// ***Panics*** if the index is out-of-range.
    fn at(index: Self::Value) -> Self;

    /// Returns a string which uniquely identifies the resource.
    fn name(self) -> &'static str;

    /// Checks whether the ID represents a "none" value. This may always be false for resource types
    /// which do not have "none" values.
    fn is_none(self) -> bool;

    /// Checks whether the ID does not represent a "none" value. This may always be true for
    /// resource types which do not have "none" values.
    #[inline]
    fn is_some(self) -> bool {
        !self.is_none()
    }

    /// Searches for the resource ID whose name matches `name` (case-insensitive).
    ///
    /// This may use a lookup table under the hood, potentially making it much faster than calling
    /// `iter()` and checking every name.
    fn find(name: impl AsRef<str>) -> Option<Self>;

    /// Creates an iterator over all resource IDs.
    #[inline]
    fn iter() -> ResourceIterator<Self> {
        ResourceIterator::new()
    }
}

/// An iterator over all resources of a particular type.
pub struct ResourceIterator<T: Resource> {
    /// Index of the next element to be returned by `next()`
    front: T::Value,
    /// Index + 1 of the next element to be returned by `next_back()`
    back: T::Value,
}

impl<T: Resource> ResourceIterator<T> {
    fn new() -> Self {
        Self { front: T::Value::zero(), back: T::Value::from_usize(T::COUNT).unwrap() }
    }
}

impl<T: Resource> Iterator for ResourceIterator<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.front < self.back {
            self.front += T::Value::one();
            Some(T::at(self.front - T::Value::one()))
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.back - self.front;
        (len.as_(), Some(len.as_()))
    }
}

impl<T: Resource> DoubleEndedIterator for ResourceIterator<T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.back > self.front {
            self.back -= T::Value::one();
            Some(T::at(self.back))
        } else {
            None
        }
    }
}

impl<T: Resource> ExactSizeIterator for ResourceIterator<T> {}
impl<T: Resource> FusedIterator for ResourceIterator<T> {}

#[cfg(test)]
mod tests {
    use super::*;
    use num_enum::{IntoPrimitive, TryFromPrimitive};

    #[derive(
        Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, IntoPrimitive, TryFromPrimitive,
    )]
    #[repr(u32)]
    enum Test {
        None,
        A,
        B,
        C,
        D,
        E,
    }

    const VALS: &[Test] = &[Test::None, Test::A, Test::B, Test::C, Test::D, Test::E];

    impl Sealed for Test {}
    impl Resource for Test {
        type Value = u32;
        const COUNT: usize = VALS.len();
        fn at(index: u32) -> Self {
            VALS[index as usize]
        }
        fn name(self) -> &'static str {
            match self {
                Self::None => "none",
                Self::A => "a",
                Self::B => "b",
                Self::C => "c",
                Self::D => "d",
                Self::E => "e",
            }
        }
        fn is_none(self) -> bool {
            self == Test::None
        }
        fn find(_name: impl AsRef<str>) -> Option<Self> {
            unimplemented!()
        }
    }

    #[test]
    fn test_none() {
        assert!(Test::None.is_none());
        assert!(!Test::None.is_some());
        assert!(!Test::A.is_none());
        assert!(Test::A.is_some());
    }

    #[test]
    fn test_iter_forward() {
        let iter = Test::iter();
        let all = iter.collect::<Vec<_>>();
        assert_eq!(all, VALS);
    }

    #[test]
    fn test_iter_backward() {
        let iter = Test::iter().rev();
        let all = iter.collect::<Vec<_>>();
        let expected = VALS.iter().copied().rev().collect::<Vec<_>>();
        assert_eq!(all, expected);
    }

    #[test]
    fn test_iter_bidirectional() {
        let mut iter = Test::iter();
        assert_eq!(iter.next(), Some(Test::None));
        assert_eq!(iter.next_back(), Some(Test::E));
        assert_eq!(iter.next(), Some(Test::A));
        assert_eq!(iter.next(), Some(Test::B));
        assert_eq!(iter.next_back(), Some(Test::D));
        assert_eq!(iter.next_back(), Some(Test::C));
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next_back(), None);
    }

    #[test]
    fn test_iter_len() {
        let mut iter = Test::iter();
        assert_eq!(iter.len(), 6);
        iter.next();
        assert_eq!(iter.len(), 5);
        iter.next_back();
        assert_eq!(iter.len(), 4);
        while iter.next().is_some() {}
        assert_eq!(iter.len(), 0);
    }

    #[test]
    fn test_iter_fused() {
        let mut iter = Test::iter();
        while iter.next().is_some() {}
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next_back(), None);
        assert_eq!(iter.next_back(), None);
    }
}
