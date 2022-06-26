mod constants;
mod reader;
mod writer;

pub use reader::MessageReader;
pub use writer::MessageWriter;

use anyhow::{anyhow, ensure, Result};
use byteorder::{ByteOrder, LE};
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
    /// The file offset of the code block which originally contained the message.
    pub block_offset: u32,
    /// The index of the message command within its code block.
    pub command_index: usize,
}

impl MessageId {
    /// Parses a `MessageId` from its display string.
    pub fn parse(s: &str) -> Result<Self> {
        let parts: Vec<_> = s.split(':').collect();
        ensure!(parts.len() == 3, "Invalid message ID: {}", s);
        Ok(Self {
            source: MessageSource::parse(parts[0])?,
            block_offset: u32::from_str_radix(parts[1], 16)?,
            command_index: usize::from_str_radix(parts[2], 16)?,
        })
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{:x}:{:x}", self.source.name(), self.block_offset, self.command_index)
    }
}

const BLOCK_OFFSET_MAGIC: u8 = b'U';

/// Encodes a block offset so that it can be stored in a message's `extra_data`.
pub fn encode_block_offset(offset: u32) -> [u8; 4] {
    // In practice, block offsets should only ever require 24 bits.
    // Use the first byte as an identifier character.
    let mut bytes = [0u8; 4];
    bytes[0] = BLOCK_OFFSET_MAGIC;
    assert!(offset < 0x1000000);
    LE::write_u24(&mut bytes[1..], offset);
    bytes
}

/// Decodes a block offset previously encoded with `encode_block_offset()`.
pub fn decode_block_offset(bytes: &[u8]) -> Option<u32> {
    if bytes.len() == 4 && bytes[0] == BLOCK_OFFSET_MAGIC {
        Some(LE::read_u24(&bytes[1..]))
    } else {
        None
    }
}

/// Returns a message's `MessageId`.
fn message_id(source: MessageSource, loc: CommandLocation, msg: &MsgArgs) -> MessageId {
    // Since we edit ISOs in-place, message IDs need to be stable between rewrites of each file so
    // that a user can re-import an XML file into an already-edited ISO. To prevent the block offset
    // from changing, we use the `extra_data` feature to store the original block offset.
    let block_offset = decode_block_offset(&msg.extra_data).unwrap_or(loc.block.offset);
    MessageId { source, block_offset, command_index: loc.index }
}

/// If a command is for a non-empty message, return its ID and message arguments.
fn filter_message(
    source: MessageSource,
    loc: CommandLocation,
    command: &Command,
) -> Option<(MessageId, &MsgArgs)> {
    if let Command::Msg(arg) | Command::Select(arg) = command {
        if !arg.commands.is_empty() {
            return Some((message_id(source, loc, arg), arg));
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
            return Some((message_id(source, loc, arg), arg));
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

    fn id(source: MessageSource, block: u32, index: usize) -> MessageId {
        MessageId { source, block_offset: block, command_index: index }
    }

    #[test]
    fn test_parse_message_id() -> Result<()> {
        let parse = MessageId::parse;
        assert_eq!(parse("globals:ab:cd")?, id(MessageSource::Globals, 0xab, 0xcd));
        assert_eq!(parse("stage02:ab:cd")?, id(MessageSource::Stage(Stage::Foyer), 0xab, 0xcd));
        assert_eq!(parse("ahk:ab:cd")?, id(MessageSource::Stage(Stage::Ahk), 0xab, 0xcd));
        assert!(parse("foo:ab:cd").is_err());
        assert!(parse("stage07:ab:cd:").is_err());
        assert!(parse("foo").is_err());
        assert!(parse("").is_err());
        Ok(())
    }

    #[test]
    fn test_encode_block_offset() {
        assert_eq!(&encode_block_offset(0x123456), &[b'U', 0x56, 0x34, 0x12]);
        assert_eq!(&encode_block_offset(0xffffff), &[b'U', 0xff, 0xff, 0xff]);
    }

    #[test]
    fn test_decode_block_offset() {
        assert_eq!(decode_block_offset(&[b'U', 0x56, 0x34, 0x12]), Some(0x123456));
        assert_eq!(decode_block_offset(&[b'X', 0x56, 0x34, 0x12]), None);
        assert_eq!(decode_block_offset(&[b'U', 0x56, 0x34]), None);
        assert_eq!(decode_block_offset(&[b'U', 0x56, 0x34, 0x12, 0x00]), None);
        assert_eq!(decode_block_offset(&[]), None);
    }
}
