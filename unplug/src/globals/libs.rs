use super::{Error, Result};
use crate::common::{ReadFrom, ReadSeek, WriteSeek, WriteTo};
use crate::event::pointer::BlockId;
use crate::event::script::{Script, ScriptReader, ScriptWriter};
use byteorder::{ByteOrder, ReadBytesExt, LE};
use std::io::{Read, SeekFrom, Write};

/// The number of library functions in a globals.bin file.
pub const NUM_LIBS: usize = 376;

/// A table of entry point offsets for script library functions.
struct LibTable {
    entry_points: Box<[u32]>,
}

impl<R: Read + ?Sized> ReadFrom<R> for LibTable {
    type Error = Error;
    fn read_from(reader: &mut R) -> Result<Self> {
        let mut entry_points = vec![0u32; NUM_LIBS];
        reader.read_u32_into::<LE>(&mut entry_points)?;
        Ok(Self { entry_points: entry_points.into_boxed_slice() })
    }
}

impl<W: Write + ?Sized> WriteTo<W> for LibTable {
    type Error = Error;
    fn write_to(&self, writer: &mut W) -> Result<()> {
        let mut bytes = vec![0u8; self.entry_points.len() * 4];
        LE::write_u32_into(&self.entry_points, &mut bytes);
        writer.write_all(&bytes)?;
        Ok(())
    }
}

/// The script library functions in a globals.bin file.
#[derive(Clone)]
pub struct Libs {
    pub script: Script,
    pub entry_points: Box<[BlockId]>,
}

impl<R: ReadSeek + ?Sized> ReadFrom<R> for Libs {
    type Error = Error;
    fn read_from(mut reader: &mut R) -> Result<Self> {
        let table = LibTable::read_from(reader)?;
        let mut entry_points = Vec::with_capacity(NUM_LIBS);
        let mut script_reader = ScriptReader::new(&mut reader);
        for &entry_point in table.entry_points.iter() {
            let id = script_reader.read_event(entry_point)?;
            entry_points.push(id);
        }
        let script = script_reader.finish()?;
        Ok(Self { script, entry_points: entry_points.into_boxed_slice() })
    }
}

impl<W: WriteSeek + ?Sized> WriteTo<W> for Libs {
    type Error = Error;
    fn write_to(&self, mut writer: &mut W) -> Result<()> {
        assert_eq!(self.entry_points.len(), NUM_LIBS);

        // Write an empty entry point table because it has to come first
        let table_offset = writer.seek(SeekFrom::Current(0))?;
        let mut table = LibTable { entry_points: vec![0u32; NUM_LIBS].into_boxed_slice() };
        table.write_to(writer)?;

        // Write out the script data
        let mut script = ScriptWriter::new(&self.script);
        for &id in &*self.entry_points {
            script.add_block(id)?;
        }
        let offsets = script.write_to(&mut writer)?;

        // Go back and fill in the entry point table
        for (&id, offset) in self.entry_points.iter().zip(&mut *table.entry_points) {
            *offset = offsets.get(id);
        }
        let end_offset = writer.seek(SeekFrom::Current(0))?;
        writer.seek(SeekFrom::Start(table_offset))?;
        table.write_to(writer)?;
        writer.seek(SeekFrom::Start(end_offset))?;
        Ok(())
    }
}
