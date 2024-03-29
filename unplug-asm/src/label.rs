use crate::span::Span;
use crate::{Error, Result};
use slotmap::{new_key_type, SlotMap};
use smallvec::SmallVec;
use std::collections::HashMap;
use std::sync::Arc;
use unplug::event::BlockId;

/// A label for a block of code in an assembly program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    /// The name of the label as it appears in source files.
    pub name: Arc<str>,
    /// The label's corresponding script block.
    pub block: BlockId,
    /// The span for the label's declaration.
    pub span: Span,
}

impl Label {
    /// Creates a new label.
    pub fn new(name: impl Into<Arc<str>>, block: BlockId, span: Span) -> Self {
        Self { name: name.into(), block, span }
    }
}

new_key_type! {
    /// A unique label identifier used to look up a label.
    pub struct LabelId;
}

type LabelVec = SmallVec<[LabelId; 1]>;

/// Stores labels in a program and allows fast lookup by ID, name, or block.
#[derive(Default, Clone)]
pub struct LabelMap {
    slots: SlotMap<LabelId, Label>,
    by_block: HashMap<BlockId, LabelVec>,
    by_name: HashMap<Arc<str>, LabelId>,
}

impl LabelMap {
    /// Creates an empty label map.
    pub fn new() -> Self {
        Self::default()
    }

    /// Gets the label corresponding to `id`.
    /// ***Panics*** if the ID is invalid.
    pub fn get(&self, id: LabelId) -> &Label {
        &self.slots[id]
    }

    /// Finds the labels corresponding to `block` and returns a slice of IDs.
    pub fn find_block(&self, block: BlockId) -> &[LabelId] {
        self.by_block.get(&block).map(|l| l.as_slice()).unwrap_or_default()
    }

    /// Finds the label corresponding to `name` and returns its ID.
    pub fn find_name(&self, name: impl AsRef<str>) -> Option<LabelId> {
        self.by_name.get(name.as_ref()).copied()
    }

    /// Inserts `label` and returns its ID. The label name must be unique or else this will fail.
    pub fn insert(&mut self, label: Label) -> Result<LabelId> {
        if self.by_name.contains_key(&label.name) {
            return Err(Error::DuplicateLabel(Arc::clone(&label.name)));
        }
        let id = self.slots.insert(label.clone());
        self.by_name.insert(label.name, id);
        self.by_block.entry(label.block).or_default().push(id);
        Ok(id)
    }

    /// Creates and inserts a new label and returns its ID.
    /// The label name must be unique or else this will fail.
    pub fn insert_new(
        &mut self,
        name: impl Into<Arc<str>>,
        block: BlockId,
        span: Span,
    ) -> Result<LabelId> {
        self.insert(Label { name: name.into(), block, span })
    }

    /// Changes the name of label `id` to `name`. Label names must be unique or else this will fail.
    /// Returns `id`.
    #[allow(clippy::needless_pass_by_value)]
    pub fn rename<S>(&mut self, id: LabelId, name: S) -> Result<LabelId>
    where
        S: AsRef<str> + Into<Arc<str>>,
    {
        let label = &mut self.slots[id];
        let name = name.as_ref();
        if &*label.name != name {
            if self.by_name.contains_key(name) {
                return Err(Error::DuplicateLabel(name.into()));
            }
            self.by_name.remove(&label.name);
            label.name = name.into();
            self.by_name.insert(Arc::clone(&label.name), id);
        }
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_label_map_insert() -> Result<()> {
        let mut labels = LabelMap::new();
        let block = BlockId::new(123);
        let id = labels.insert(Label::new("foo", block, Span::EMPTY))?;
        let label = labels.get(id);
        assert_eq!(&*label.name, "foo");
        assert_eq!(label.block, block);
        assert_eq!(labels.find_name("foo"), Some(id));
        assert_eq!(labels.find_block(block), &[id]);
        Ok(())
    }

    #[test]
    fn test_label_map_insert_name_collision() -> Result<()> {
        let mut labels = LabelMap::new();
        let id = labels.insert(Label::new("foo", BlockId::new(0), Span::EMPTY))?;
        assert!(labels.insert(Label::new("foo", BlockId::new(1), Span::EMPTY)).is_err());
        assert_eq!(&*labels.get(id).name, "foo");
        assert_eq!(labels.find_name("foo"), Some(id));
        Ok(())
    }

    #[test]
    fn test_label_map_insert_same_block() -> Result<()> {
        let mut labels = LabelMap::new();
        let block = BlockId::new(123);
        let id1 = labels.insert(Label::new("foo", block, Span::EMPTY))?;
        let id2 = labels.insert(Label::new("bar", block, Span::EMPTY))?;
        assert_eq!(&*labels.get(id1).name, "foo");
        assert_eq!(&*labels.get(id2).name, "bar");
        assert_eq!(labels.find_block(block), &[id1, id2]);
        Ok(())
    }

    #[test]
    fn test_label_map_rename() -> Result<()> {
        let mut labels = LabelMap::new();
        let block = BlockId::new(123);
        let id = labels.insert(Label::new("foo", block, Span::EMPTY))?;
        labels.rename(id, "bar")?;
        let label = labels.get(id);
        assert_eq!(&*label.name, "bar");
        assert_eq!(label.block, block);
        assert_eq!(labels.find_name("foo"), None);
        assert_eq!(labels.find_name("bar"), Some(id));
        assert_eq!(labels.find_block(block), &[id]);
        Ok(())
    }

    #[test]
    fn test_label_map_rename_same() -> Result<()> {
        let mut labels = LabelMap::new();
        let block = BlockId::new(123);
        let id = labels.insert(Label::new("foo", block, Span::EMPTY))?;
        labels.rename(id, "foo")?;
        let label = labels.get(id);
        assert_eq!(&*label.name, "foo");
        assert_eq!(label.block, block);
        assert_eq!(labels.find_name("foo"), Some(id));
        assert_eq!(labels.find_block(block), &[id]);
        Ok(())
    }

    #[test]
    fn test_label_map_rename_collision() -> Result<()> {
        let mut labels = LabelMap::new();
        let id1 = labels.insert(Label::new("foo", BlockId::new(0), Span::EMPTY))?;
        let id2 = labels.insert(Label::new("bar", BlockId::new(1), Span::EMPTY))?;
        assert!(labels.rename(id1, "bar").is_err());
        assert_eq!(&*labels.get(id1).name, "foo");
        assert_eq!(&*labels.get(id2).name, "bar");
        assert_eq!(labels.find_name("foo"), Some(id1));
        assert_eq!(labels.find_name("bar"), Some(id2));
        Ok(())
    }
}
