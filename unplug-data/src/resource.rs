use crate::private::Sealed;
use num_traits::{AsPrimitive, FromPrimitive, NumAssignOps, One, PrimInt, Zero};
use std::iter::FusedIterator;
use std::marker::PhantomData;

/// Trait for resources which can be iterated over.
pub trait Resource: Sealed + Sized {
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

    /// Creates an iterator over all resource IDs.
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
    _marker: PhantomData<T>,
}

impl<T: Resource> ResourceIterator<T> {
    fn new() -> Self {
        Self {
            front: T::Value::zero(),
            back: T::Value::from_usize(T::COUNT).unwrap(),
            _marker: PhantomData,
        }
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

    #[derive(Debug, Copy, Clone, PartialEq, Eq, IntoPrimitive, TryFromPrimitive)]
    #[repr(u32)]
    enum Test {
        A,
        B,
        C,
        D,
        E,
        F,
    }

    const VALS: &[Test] = &[Test::A, Test::B, Test::C, Test::D, Test::E, Test::F];

    impl Sealed for Test {}
    impl Resource for Test {
        type Value = u32;
        const COUNT: usize = VALS.len();
        fn at(index: u32) -> Self {
            VALS[index as usize]
        }
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
        assert_eq!(iter.next(), Some(Test::A));
        assert_eq!(iter.next_back(), Some(Test::F));
        assert_eq!(iter.next(), Some(Test::B));
        assert_eq!(iter.next(), Some(Test::C));
        assert_eq!(iter.next_back(), Some(Test::E));
        assert_eq!(iter.next_back(), Some(Test::D));
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
