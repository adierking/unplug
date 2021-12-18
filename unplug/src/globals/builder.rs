use super::collision::ObjectColliders;
use super::header::FileHeader;
use super::metadata::Metadata;
use super::reader::CopyGlobals;
use super::{Libs, Result};
use crate::common::io::pad;
use crate::common::{Region, WriteSeek, WriteTo};
use crate::event::opcodes::CMD_RETURN;
use std::io::{BufWriter, SeekFrom, Write};

/// Partitions are aligned on 4-byte boundaries
const PARTITION_ALIGN: u64 = 4;

#[derive(Default)]
/// Builds globals.bin data.
pub struct GlobalsBuilder<'a> {
    base: Option<&'a mut dyn CopyGlobals>,
    metadata: Option<&'a Metadata>,
    colliders: Option<&'a ObjectColliders>,
    libs: Option<&'a Libs>,
}

impl<'a> GlobalsBuilder<'a> {
    /// Constructs a new `GlobalsBuilder`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets a `GlobalsReader<R>` to copy partitions from by default.
    pub fn base<'s>(&'s mut self, globals: &'a mut dyn CopyGlobals) -> &'s mut Self {
        self.base = Some(globals);
        self
    }

    /// Sets the metadata to write.
    pub fn metadata<'s>(&'s mut self, metadata: &'a Metadata) -> &'s mut Self {
        self.metadata = Some(metadata);
        self
    }

    /// Sets the colliders to write.
    pub fn colliders<'s>(&'s mut self, colliders: &'a ObjectColliders) -> &'s mut Self {
        self.colliders = Some(colliders);
        self
    }

    /// Sets the library functions to write.
    pub fn libs<'s>(&'s mut self, libs: &'a Libs) -> &'s mut Self {
        self.libs = Some(libs);
        self
    }

    /// Writes out a globals.bin file.
    pub fn write_to(&mut self, mut writer: impl WriteSeek) -> Result<()> {
        // Write an empty header we can fill in later
        writer.seek(SeekFrom::Start(0))?;
        let mut buf = BufWriter::new(&mut writer);
        let mut header = FileHeader::default();
        header.write_to(&mut buf)?;
        buf.flush()?;
        drop(buf);

        // Metadata
        let metadata_start = writer.seek(SeekFrom::Current(0))? as u32;
        if let Some(metadata) = self.metadata {
            let region = Region::with_inf_max_len(&mut writer, metadata_start as u64, 0);
            let mut buf = BufWriter::new(region);
            metadata.write_to(&mut buf)?;
            buf.flush()?;
        } else {
            let base = self.base.as_mut().expect("Missing metadata");
            base.copy_metadata(&mut writer)?;
        }
        header.set_metadata(metadata_start, writer.seek(SeekFrom::Current(0))? as u32);
        pad(&mut writer, PARTITION_ALIGN, 0)?;

        // Collision
        let collision_start = writer.seek(SeekFrom::Current(0))? as u32;
        if let Some(colliders) = self.colliders {
            let region = Region::with_inf_max_len(&mut writer, collision_start as u64, 0);
            let mut buf = BufWriter::new(region);
            colliders.write_to(&mut buf)?;
            buf.flush()?;
        } else {
            let base = self.base.as_mut().expect("Missing colliders data");
            base.copy_colliders(&mut writer)?;
        }
        header.set_collision(collision_start, writer.seek(SeekFrom::Current(0))? as u32);
        pad(&mut writer, PARTITION_ALIGN, 0)?;

        // Libs
        let libs_start = writer.seek(SeekFrom::Current(0))? as u32;
        if let Some(libs) = self.libs {
            let region = Region::with_inf_max_len(&mut writer, libs_start as u64, 0);
            let mut buf = BufWriter::new(region);
            libs.write_to(&mut buf)?;
            buf.flush()?;
        } else {
            let base = self.base.as_mut().expect("Missing libs data");
            base.copy_libs(&mut writer)?;
        }
        header.set_libs(libs_start, writer.seek(SeekFrom::Current(0))? as u32);
        // The libs partition seems to be padded with return commands
        pad(&mut writer, PARTITION_ALIGN, CMD_RETURN)?;

        // Go back and fill in the header
        writer.seek(SeekFrom::Start(0))?;
        let mut buf = BufWriter::new(&mut writer);
        header.write_to(&mut buf)?;
        buf.flush()?;
        Ok(())
    }
}
