mod constants;
mod reader;
mod writer;

pub use reader::MessageReader;
pub use writer::MessageWriter;

use anyhow::{anyhow, ensure, Result};
use std::fmt;
use unplug::data::{Resource, Stage};
use unplug::event::msg::MsgArgs;
use unplug::event::script::CommandLocation;
use unplug::event::{Command, Script};

/// Describes the file where a message is located.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MessageSource {
    /// The message is in globals.bin.
    Globals,
    /// The message is in a stage file.
    Stage(Stage),
}

impl MessageSource {
    /// Returns the source's filename without the extension.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Globals => "globals",
            Self::Stage(stage) => stage.name(),
        }
    }

    /// Parses a `MessageSource` from a string (the reverse of `name()`).
    pub fn parse(s: &str) -> Result<Self> {
        if s == "globals" {
            Ok(Self::Globals)
        } else {
            let stage = Stage::iter()
                .find(|stage| stage.name() == s)
                .ok_or_else(|| anyhow!("Invalid message source: {}", s))?;
            Ok(Self::Stage(stage))
        }
    }
}

/// A unique identifier for a message.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MessageId {
    /// The message's origin.
    pub source: MessageSource,
    /// The ID of the code block which originally contained the message.
    pub block_id: usize,
    /// The index of the message command within its code block.
    pub command_index: usize,
}

impl MessageId {
    /// Creates a new `MessageId` from its components.
    pub fn new(source: MessageSource, block_id: usize, command_index: usize) -> Self {
        Self { source, block_id, command_index }
    }

    /// Parses a `MessageId` from its display string.
    pub fn parse(s: &str) -> Result<Self> {
        let parts: Vec<_> = s.split(':').collect();
        ensure!(parts.len() == 3, "Invalid message ID: {}", s);
        Ok(Self {
            source: MessageSource::parse(parts[0])?,
            block_id: parts[1].parse()?,
            command_index: usize::from_str_radix(parts[2], 16)?,
        })
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{:x}", self.source.name(), self.block_id, self.command_index)
    }
}

/// If a command is for a non-empty message, return its ID and message arguments.
fn filter_message(
    source: MessageSource,
    loc: CommandLocation,
    command: &Command,
) -> Option<(MessageId, &MsgArgs)> {
    if let Command::Msg(arg) | Command::Select(arg) = command {
        if !arg.commands.is_empty() {
            return Some((MessageId::new(source, loc.block.id.index(), loc.index), arg));
        }
    }
    None
}

/// If a mutable command is for a non-empty message, return its ID and message arguments.
fn filter_message_mut(
    source: MessageSource,
    loc: CommandLocation,
    command: &mut Command,
) -> Option<(MessageId, &mut MsgArgs)> {
    if let Command::Msg(arg) | Command::Select(arg) = command {
        if !arg.commands.is_empty() {
            return Some((MessageId::new(source, loc.block.id.index(), loc.index), arg));
        }
    }
    None
}

/// Returns an iterator over the messages in a script.
pub fn iter_messages(
    source: MessageSource,
    script: &Script,
) -> impl Iterator<Item = (MessageId, &MsgArgs)> {
    script.commands_ordered().filter_map(move |(l, c)| filter_message(source, l, c))
}

/// Returns a mutable iterator over the messages in a script.
pub fn iter_messages_mut(
    source: MessageSource,
    script: &mut Script,
) -> impl Iterator<Item = (MessageId, &mut MsgArgs)> {
    script.commands_ordered_mut().filter_map(move |(l, c)| filter_message_mut(source, l, c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write;

    #[test]
    fn test_parse_message_id() -> Result<()> {
        let parse = MessageId::parse;
        assert_eq!(parse("globals:123:cd")?, MessageId::new(MessageSource::Globals, 123, 0xcd));
        assert_eq!(
            parse("stage02:123:cd")?,
            MessageId::new(MessageSource::Stage(Stage::Foyer), 123, 0xcd)
        );
        assert_eq!(
            parse("ahk:123:cd")?,
            MessageId::new(MessageSource::Stage(Stage::Ahk), 123, 0xcd)
        );
        assert!(parse("foo:123:cd").is_err());
        assert!(parse("stage07:123:cd:").is_err());
        assert!(parse("foo").is_err());
        assert!(parse("").is_err());
        Ok(())
    }

    #[test]
    fn test_display_message_id() {
        let id = MessageId::new(MessageSource::Stage(Stage::LivingRoom), 123, 0xcd);
        let mut s = String::new();
        write!(s, "{}", id).unwrap();
        assert_eq!(s, "stage07:123:cd");

        let id = MessageId::new(MessageSource::Globals, 123, 0xcd);
        s.clear();
        write!(s, "{}", id).unwrap();
        assert_eq!(s, "globals:123:cd");
    }
}
