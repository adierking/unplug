use crate::audio::{Error, Result};
use crate::common::ReadFrom;
use byteorder::{ReadBytesExt, BE};
use log::debug;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::convert::TryFrom;
use std::io::{Read, Seek, SeekFrom};

/// The file header.
#[derive(Debug, Clone, Default)]
struct Header {
    unk_00: u32, // zero
    unk_04: u32, // zero
    /// The base index for each event group. Usually these just correspond to each sound bank.
    group_bases: Vec<u32>,
    /// The file offsets of the first action in each event.
    event_offsets: Vec<u32>,
}

impl<R: Read> ReadFrom<R> for Header {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let mut header = Self {
            unk_00: reader.read_u32::<BE>()?,
            unk_04: reader.read_u32::<BE>()?,
            ..Default::default()
        };
        let num_groups = reader.read_u32::<BE>()?;
        for _ in 0..num_groups {
            header.group_bases.push(reader.read_u32::<BE>()?);
        }
        let num_events = reader.read_u32::<BE>()?;
        for _ in 0..num_events {
            header.event_offsets.push(reader.read_u32::<BE>()?);
        }
        Ok(header)
    }
}

/// An action command.
#[derive(Debug, Copy, Clone, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum Command {
    /// Does nothing except process the delay (if any).
    None = 0,
    /// Sets the sound to play.
    Sound = 1,
    Unk2 = 2,
    Unk3 = 3,
    Unk4 = 4,
    Unk5 = 5,
    Unk6 = 6,
    Unk7 = 7,
    Unk8 = 8,
    Unk9 = 9,
    Unk10 = 10,
    Unk11 = 11,
    Unk12 = 12,
    Unk13 = 13,
    /// Ends the event.
    End1 = 14,
    /// Also ends the event but sets a different flag than `End1`.
    End2 = 15,
    Unk16 = 16,
    Unk17 = 17,
    Unk18 = 18,
    Unk19 = 19,
    Unk20 = 20,
    Unk21 = 21,
    Unk22 = 22,
    Unk23 = 23,
    Unk24 = 24,
    Unk25 = 25,
    Unk26 = 26,
    Unk27 = 27,
    Unk28 = 28,
}

/// An action to perform as part of an event.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Action {
    /// The command to perform.
    pub command: Command,
    /// The delay (in 5ms units) after executing this action.
    pub delay: u8,
    /// Command-specific data.
    pub data: u16,
}

impl<R: Read> ReadFrom<R> for Action {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        // Each action is 32 bits and is interpreted based on the command in the high byte
        let op = reader.read_u32::<BE>()?;
        let code = (op >> 24) as u8;
        let command = match Command::try_from(code) {
            Ok(c) => c,
            Err(_) => return Err(Error::UnrecognizedEventCommand(code)),
        };
        let (delay, data) = match command {
            // No data
            Command::None | Command::End1 | Command::End2 => (op & 0xff, 0),

            // 16-bit data
            Command::Sound
            | Command::Unk2
            | Command::Unk3
            | Command::Unk12
            | Command::Unk13
            | Command::Unk23
            | Command::Unk24
            | Command::Unk25
            | Command::Unk26 => ((op >> 16) & 0xff, op & 0xffff),

            // 8-bit data
            Command::Unk4
            | Command::Unk5
            | Command::Unk6
            | Command::Unk7
            | Command::Unk8
            | Command::Unk9
            | Command::Unk10
            | Command::Unk11
            | Command::Unk16
            | Command::Unk17
            | Command::Unk18
            | Command::Unk19
            | Command::Unk20
            | Command::Unk21
            | Command::Unk22 => ((op >> 8) & 0xff, op & 0xff),

            // No delay
            Command::Unk27 | Command::Unk28 => (0, op & 0xff),
        };
        Ok(Self { command, delay: delay as u8, data: data as u16 })
    }
}

/// A sound event in an event bank.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// The actions in the event.
    pub actions: Vec<Action>,
}

impl<R: Read> ReadFrom<R> for Event {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        // Keep reading until we hit an end command
        let mut actions = vec![];
        loop {
            let action = Action::read_from(reader)?;
            actions.push(action);
            if let Command::End1 | Command::End2 = action.command {
                break;
            }
        }
        Ok(Self { actions })
    }
}

/// A collection of sound events (.sem).
#[derive(Debug, Clone)]
pub struct EventBank {
    /// The base index for each event group. Usually these just correspond to each sound bank.
    pub group_bases: Vec<u32>,
    /// The events in the bank.
    pub events: Vec<Event>,
}

impl<R: Read + Seek> ReadFrom<R> for EventBank {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let header = Header::read_from(reader)?;
        let mut events = Vec::with_capacity(header.event_offsets.len());
        for offset in header.event_offsets {
            reader.seek(SeekFrom::Start(offset as u64))?;
            events.push(Event::read_from(reader)?);
        }
        debug!(
            "Loaded event bank with {} groups and {} events",
            header.group_bases.len(),
            events.len()
        );
        Ok(Self { group_bases: header.group_bases, events })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_read_action() -> Result<()> {
        let bytes: &[u8] = &[0x0e, 0x12, 0x34, 0x56];
        let actual = Action::read_from(&mut Cursor::new(bytes))?;
        let expected = Action { command: Command::End1, delay: 0x56, data: 0 };
        assert_eq!(actual, expected);

        let bytes: &[u8] = &[0x01, 0x12, 0x34, 0x56];
        let actual = Action::read_from(&mut Cursor::new(bytes))?;
        let expected = Action { command: Command::Sound, delay: 0x12, data: 0x3456 };
        assert_eq!(actual, expected);

        let bytes: &[u8] = &[0x04, 0x12, 0x34, 0x56];
        let actual = Action::read_from(&mut Cursor::new(bytes))?;
        let expected = Action { command: Command::Unk4, delay: 0x34, data: 0x56 };
        assert_eq!(actual, expected);

        let bytes: &[u8] = &[0x1c, 0x12, 0x34, 0x56];
        let actual = Action::read_from(&mut Cursor::new(bytes))?;
        let expected = Action { command: Command::Unk28, delay: 0, data: 0x56 };
        assert_eq!(actual, expected);
        Ok(())
    }
}
