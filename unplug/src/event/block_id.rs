use std::fmt;

/// A strongly-typed index into a script's block list.
#[derive(Copy, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockId(usize);

impl BlockId {
    /// Constructs an ID from an index into a block list.
    #[inline]
    pub fn new(index: usize) -> Self {
        Self(index)
    }

    /// Gets the block list index corresponding to this ID.
    #[inline]
    pub fn index(&self) -> usize {
        self.0
    }

    /// Uses this ID to get a reference to an object in a slice.
    #[inline]
    pub fn get<'a, T>(&self, slice: &'a [T]) -> &'a T {
        &slice[self.index()]
    }

    /// Uses this ID to get a mutable reference to an object in a slice.
    #[inline]
    pub fn get_mut<'a, T>(&self, slice: &'a mut [T]) -> &'a mut T {
        &mut slice[self.index()]
    }
}

impl fmt::Debug for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.index())
    }
}

impl From<usize> for BlockId {
    #[inline]
    fn from(index: usize) -> Self {
        Self::new(index)
    }
}

impl From<BlockId> for usize {
    #[inline]
    fn from(id: BlockId) -> Self {
        id.index()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_id_new_index() {
        assert_eq!(BlockId::new(123).index(), 123);
    }

    #[test]
    fn test_block_id_get() {
        let mut values = [1, 2, 3];
        assert_eq!(*BlockId::new(1).get(&values), 2);
        *BlockId::new(1).get_mut(&mut values) = 4;
        assert_eq!(values[1], 4);
    }

    #[test]
    fn test_block_id_to_from_usize() {
        assert_eq!(usize::from(BlockId::new(1)), 1);
        assert_eq!(BlockId::from(1usize), BlockId::new(1));
    }

    #[test]
    fn test_compare_block_ids() {
        assert_eq!(BlockId::new(1), BlockId::new(1));
        assert_ne!(BlockId::new(0), BlockId::new(1));
        assert!(BlockId::new(0) < BlockId::new(1));
        assert!(BlockId::new(1) > BlockId::new(0));
    }
}
