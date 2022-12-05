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
    /// The label's corresponding script block (if any).
    pub block: Option<BlockId>,
    /// The span for the label's declaration.
    pub span: Span,
}

impl Label {
    /// Creates a label named `name` with no block or span assigned.
    pub fn new(name: impl Into<Arc<str>>) -> Self {
        Self { name: name.into(), block: None, span: Span::EMPTY }
    }

    /// Creates a label named `name` with `block` but no span assigned.
    pub fn with_block(name: impl Into<Arc<str>>, block: BlockId) -> Self {
        Self { name: name.into(), block: Some(block), span: Span::EMPTY }
    }

    pub fn with_block_and_span(name: impl Into<Arc<str>>, block: BlockId, span: Span) -> Self {
        Self { name: name.into(), block: Some(block), span }
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
        if let Some(block) = label.block {
            self.by_block.entry(block).or_default().push(id);
        }
        Ok(id)
    }

    /// Inserts a new label named `name`, optionally associates it with `block`, and returns its ID.
    /// The label name must be unique or else this will fail.
    pub fn insert_new(
        &mut self,
        name: impl Into<Arc<str>>,
        block: Option<BlockId>,
        span: Span,
    ) -> Result<LabelId> {
        self.insert(Label { name: name.into(), block, span })
    }

    /// Changes the name of label `id` to `name`. Label names must be unique or else this will fail.
    /// Returns `id`.
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
        let id = labels.insert(Label::with_block("foo", block))?;
        let label = labels.get(id);
        assert_eq!(&*label.name, "foo");
        assert_eq!(label.block, Some(block));
        assert_eq!(labels.find_name("foo"), Some(id));
        assert_eq!(labels.find_block(block), &[id]);
        Ok(())
    }

    #[test]
    fn test_label_map_insert_name_collision() -> Result<()> {
        let mut labels = LabelMap::new();
        let id = labels.insert(Label::new("foo"))?;
        assert!(labels.insert(Label::new("foo")).is_err());
        assert_eq!(&*labels.get(id).name, "foo");
        assert_eq!(labels.find_name("foo"), Some(id));
        Ok(())
    }

    #[test]
    fn test_label_map_insert_same_block() -> Result<()> {
        let mut labels = LabelMap::new();
        let block = BlockId::new(123);
        let id1 = labels.insert(Label::with_block("foo", block))?;
        let id2 = labels.insert(Label::with_block("bar", block))?;
        assert_eq!(&*labels.get(id1).name, "foo");
        assert_eq!(&*labels.get(id2).name, "bar");
        assert_eq!(labels.find_block(block), &[id1, id2]);
        Ok(())
    }

    #[test]
    fn test_label_map_rename() -> Result<()> {
        let mut labels = LabelMap::new();
        let block = BlockId::new(123);
        let id = labels.insert(Label::with_block("foo", block))?;
        labels.rename(id, "bar")?;
        let label = labels.get(id);
        assert_eq!(&*label.name, "bar");
        assert_eq!(label.block, Some(block));
        assert_eq!(labels.find_name("foo"), None);
        assert_eq!(labels.find_name("bar"), Some(id));
        assert_eq!(labels.find_block(block), &[id]);
        Ok(())
    }

    #[test]
    fn test_label_map_rename_same() -> Result<()> {
        let mut labels = LabelMap::new();
        let block = BlockId::new(123);
        let id = labels.insert(Label::with_block("foo", block))?;
        labels.rename(id, "foo")?;
        let label = labels.get(id);
        assert_eq!(&*label.name, "foo");
        assert_eq!(label.block, Some(block));
        assert_eq!(labels.find_name("foo"), Some(id));
        assert_eq!(labels.find_block(block), &[id]);
        Ok(())
    }

    #[test]
    fn test_label_map_rename_collision() -> Result<()> {
        let mut labels = LabelMap::new();
        let id1 = labels.insert(Label::new("foo"))?;
        let id2 = labels.insert(Label::new("bar"))?;
        assert!(labels.rename(id1, "bar").is_err());
        assert_eq!(&*labels.get(id1).name, "foo");
        assert_eq!(&*labels.get(id2).name, "bar");
        assert_eq!(labels.find_name("foo"), Some(id1));
        assert_eq!(labels.find_name("bar"), Some(id2));
        Ok(())
    }
}
