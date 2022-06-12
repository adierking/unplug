use crate::private::Sealed;
use std::iter::FusedIterator;
use std::marker::PhantomData;

/// Trait for resources which can be iterated over.
pub trait Resource: Sealed {
    /// The total number of this type of resource.
    const COUNT: usize;

    /// Returns the resource corresponding to an index in the range `[0, count())`.
    /// ***Panics*** if the index is out-of-range.
    fn at(index: usize) -> Self;
}

/// An iterator over all resources of a particular type.
pub struct ResourceIterator<T: Resource> {
    /// Index of the next element to be returned by `next()`
    front: usize,
    /// Index + 1 of the next element to be returned by `next_back()`
    back: usize,
    _marker: PhantomData<T>,
}

impl<T: Resource> ResourceIterator<T> {
    pub(crate) fn new() -> Self {
        Self { front: 0, back: T::COUNT, _marker: PhantomData }
    }
}

impl<T: Resource> Iterator for ResourceIterator<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.front < self.back {
            self.front += 1;
            Some(T::at(self.front - 1))
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.back - self.front;
        (len, Some(len))
    }
}

impl<T: Resource> DoubleEndedIterator for ResourceIterator<T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.back > self.front {
            self.back -= 1;
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

    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
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
        const COUNT: usize = VALS.len();
        fn at(index: usize) -> Self {
            VALS[index]
        }
    }

    #[test]
    fn test_iter_forward() {
        let iter = ResourceIterator::<Test>::new();
        let all = iter.collect::<Vec<_>>();
        assert_eq!(all, VALS);
    }

    #[test]
    fn test_iter_backward() {
        let iter = ResourceIterator::<Test>::new().rev();
        let all = iter.collect::<Vec<_>>();
        let expected = VALS.iter().copied().rev().collect::<Vec<_>>();
        assert_eq!(all, expected);
    }

    #[test]
    fn test_iter_bidirectional() {
        let mut iter = ResourceIterator::<Test>::new();
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
        let mut iter = ResourceIterator::<Test>::new();
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
        let mut iter = ResourceIterator::<Test>::new();
        while iter.next().is_some() {}
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next_back(), None);
        assert_eq!(iter.next_back(), None);
    }
}
