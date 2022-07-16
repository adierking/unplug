pub mod lexer;
pub mod opcodes;
pub mod parser;
pub mod writer;

pub use lexer::{Number, Token};
pub use opcodes::{AsmMsgOp, DataOp, NamedOpcode};
pub use parser::Ast;
pub use writer::{ProgramBuilder, ProgramWriter};

use anyhow::{ensure, Result};
use slotmap::{new_key_type, SlotMap};
use std::collections::HashMap;
use std::rc::Rc;
use unplug::common::Text;
use unplug::event::opcodes::{ExprOp, TypeOp};
use unplug::event::BlockId;

/// A label for a block of code in an assembly program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    /// The name of the label as it appears in source files.
    pub name: Rc<str>,
    /// The label's corresponding script block (if any).
    pub block: Option<BlockId>,
}

impl Label {
    /// Creates a label named `name` with no block assigned.
    pub fn new(name: impl Into<Rc<str>>) -> Self {
        Self { name: name.into(), block: None }
    }

    /// Creates a label named `name` with `block` assigned.
    pub fn with_block(name: impl Into<Rc<str>>, block: BlockId) -> Self {
        Self { name: name.into(), block: Some(block) }
    }
}

new_key_type! {
    /// A unique label identifier used to look up a label.
    pub struct LabelId;
}

/// Stores labels in a program and allows fast lookup by ID, name, or block.
#[derive(Default, Clone)]
pub struct LabelMap {
    slots: SlotMap<LabelId, Label>,
    by_block: HashMap<BlockId, LabelId>,
    by_name: HashMap<Rc<str>, LabelId>,
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

    /// Finds the label corresponding to `block` and returns its ID.
    pub fn find_block(&self, block: BlockId) -> Option<LabelId> {
        self.by_block.get(&block).copied()
    }

    /// Finds the label corresponding to `name` and returns its ID.
    pub fn find_name(&self, name: impl AsRef<str>) -> Option<LabelId> {
        self.by_name.get(name.as_ref()).copied()
    }

    /// Inserts `label` and returns its ID. The label name and block must each be unique or else
    /// this will fail.
    pub fn insert(&mut self, label: Label) -> Result<LabelId> {
        ensure!(!self.by_name.contains_key(&label.name), "Duplicate label name: {}", label.name);
        if let Some(block) = label.block {
            ensure!(!self.by_block.contains_key(&block), "Block {:?} already has a label", block);
        }
        let id = self.slots.insert(label.clone());
        self.by_name.insert(label.name, id);
        if let Some(block) = label.block {
            self.by_block.insert(block, id);
        }
        Ok(id)
    }

    /// Changes the name of label `id` to `name`. Label names must be unique or else this will fail.
    pub fn rename<S>(&mut self, id: LabelId, name: S) -> Result<()>
    where
        S: AsRef<str> + Into<Rc<str>>,
    {
        let label = &mut self.slots[id];
        let name = name.as_ref();
        if &*label.name != name {
            ensure!(!self.by_name.contains_key(name), "Duplicate label name: {}", name);
            self.by_name.remove(&label.name);
            label.name = name.into();
            self.by_name.insert(Rc::clone(&label.name), id);
        }
        Ok(())
    }

    /// If `block` has a corresponding label, renames it to `name`, otherwise inserts a new label.
    /// Returns the label ID on success, or fails if the new name is not unique.
    pub fn rename_or_insert<S>(&mut self, block: BlockId, name: S) -> Result<LabelId>
    where
        S: AsRef<str> + Into<Rc<str>>,
    {
        match self.find_block(block) {
            Some(id) => {
                self.rename(id, name)?;
                Ok(id)
            }
            None => Ok(self.insert(Label::with_block(name, block))?),
        }
    }

    /// Finds the label corresponding to `block`, and if it is not found, inserts a label named from
    /// `name_fn()` and returns its ID.
    pub fn find_block_or_insert<S, F>(&mut self, block: BlockId, name_fn: F) -> Result<LabelId>
    where
        S: AsRef<str> + Into<Rc<str>>,
        F: FnOnce() -> S,
    {
        match self.find_block(block) {
            Some(id) => Ok(id),
            None => {
                let name = name_fn();
                ensure!(
                    !self.by_name.contains_key(name.as_ref()),
                    "Duplicate label name: {}",
                    name.as_ref()
                );
                let label = Label::with_block(name, block);
                let id = self.slots.insert(label.clone());
                self.by_name.insert(label.name, id);
                self.by_block.insert(block, id);
                Ok(id)
            }
        }
    }
}

/// An operation consisting of an opcode and zero or more operands.
#[derive(Debug, Clone)]
pub struct Operation<T: NamedOpcode> {
    pub opcode: T,
    pub operands: Vec<Operand>,
}

impl<T: NamedOpcode> Operation<T> {
    pub fn new(opcode: T) -> Self {
        Self { opcode, operands: vec![] }
    }
}

/// Data which can be operated on.
#[derive(Debug, Clone)]
pub enum Operand {
    /// An 8-bit signed integer.
    I8(i8),
    /// An 8-bit unsigned integer.
    U8(u8),
    /// A 16-bit signed integer.
    I16(i16),
    /// A 16-bit unsigned integer.
    U16(u16),
    /// A 32-bit signed integer.
    I32(i32),
    /// A 32-bit unsigned integer.
    U32(u32),
    /// A printable text string.
    Text(Text),
    /// A label reference.
    Label(LabelId),
    /// A label reference indicating it is an "else" condition.
    ElseLabel(LabelId),
    /// A raw file offset reference.
    Offset(u32),
    /// A type expression.
    Type(TypeOp),
    /// An expression.
    Expr(Operation<ExprOp>),
    /// A message command.
    MsgCommand(Operation<AsmMsgOp>),
}

macro_rules! impl_operand_from {
    ($type:ty, $name:ident) => {
        impl From<$type> for Operand {
            fn from(x: $type) -> Self {
                Self::$name(x)
            }
        }
    };
}
impl_operand_from!(i8, I8);
impl_operand_from!(u8, U8);
impl_operand_from!(i16, I16);
impl_operand_from!(u16, U16);
impl_operand_from!(i32, I32);
impl_operand_from!(u32, U32);
impl_operand_from!(Text, Text);
impl_operand_from!(TypeOp, Type);
impl_operand_from!(Operation<ExprOp>, Expr);
impl_operand_from!(Operation<AsmMsgOp>, MsgCommand);

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
        assert_eq!(labels.find_block(block), Some(id));
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
    fn test_label_map_insert_block_collision() -> Result<()> {
        let mut labels = LabelMap::new();
        let block = BlockId::new(123);
        let id = labels.insert(Label::with_block("foo", block))?;
        assert!(labels.insert(Label::with_block("bar", block)).is_err());
        assert_eq!(&*labels.get(id).name, "foo");
        assert_eq!(labels.find_block(block), Some(id));
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
        assert_eq!(labels.find_block(block), Some(id));
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
        assert_eq!(labels.find_block(block), Some(id));
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

    #[test]
    fn test_label_map_rename_or_insert() -> Result<()> {
        let mut labels = LabelMap::new();
        let block = BlockId::new(123);
        let id1 = labels.rename_or_insert(block, "foo")?;
        assert_eq!(&*labels.get(id1).name, "foo");
        let id2 = labels.rename_or_insert(block, "bar")?;
        assert_eq!(id1, id2);
        assert_eq!(&*labels.get(id1).name, "bar");
        assert_eq!(labels.find_name("foo"), None);
        assert_eq!(labels.find_name("bar"), Some(id1));
        Ok(())
    }

    #[test]
    fn test_label_map_find_block_or_insert() -> Result<()> {
        let mut labels = LabelMap::new();
        let block = BlockId::new(123);
        let id1 = labels.find_block_or_insert(block, || "foo")?;
        assert_eq!(&*labels.get(id1).name, "foo");
        let id2 = labels.find_block_or_insert(block, || "bar")?;
        assert_eq!(id1, id2);
        assert_eq!(&*labels.get(id1).name, "foo");
        assert_eq!(labels.find_name("foo"), Some(id1));
        assert_eq!(labels.find_name("bar"), None);
        Ok(())
    }
}
