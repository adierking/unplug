use super::opcodes::{CmdOp, ExprOp, Ggte, MsgOp, OpcodeMap, TypeOp};
use super::pointer::{Pointer, WritePointer};
use super::serialize::{Error, EventDeserializer, EventSerializer, Result};
use crate::common::text::Text;
use crate::common::{ReadFrom, WriteTo};
use byteorder::{ReadBytesExt, WriteBytesExt, BE, LE};
use std::ffi::CString;
use std::io::{Read, Seek, SeekFrom, Write};

/// The maximum size of a serialized message command list in bytes.
const MAX_MSG_SIZE: u64 = 2048;

/// Event serializer for writing events to .bin files.
pub struct BinSerializer<W: Write + WritePointer + Seek> {
    writer: W,
    call_start_offset: u64, // u64::MAX if not set
    msg_start_offset: u64,  // u64::MAX if not set
}

impl<W: Write + WritePointer + Seek> BinSerializer<W> {
    /// Creates a new `BinSerializer` which wraps `writer`.
    pub fn new(writer: W) -> Self {
        Self { writer, call_start_offset: u64::MAX, msg_start_offset: u64::MAX }
    }

    /// Consumes the serializer, returning the inner writer.
    pub fn into_inner(self) -> W {
        self.writer
    }
}

impl<W: Write + WritePointer + Seek> EventSerializer for BinSerializer<W> {
    fn serialize_i8(&mut self, val: i8) -> Result<()> {
        Ok(self.writer.write_i8(val)?)
    }

    fn serialize_u8(&mut self, val: u8) -> Result<()> {
        Ok(self.writer.write_u8(val)?)
    }

    fn serialize_i16(&mut self, val: i16) -> Result<()> {
        Ok(self.writer.write_i16::<LE>(val)?)
    }

    fn serialize_u16(&mut self, val: u16) -> Result<()> {
        Ok(self.writer.write_u16::<LE>(val)?)
    }

    fn serialize_i32(&mut self, val: i32) -> Result<()> {
        Ok(self.writer.write_i32::<LE>(val)?)
    }

    fn serialize_u32(&mut self, val: u32) -> Result<()> {
        Ok(self.writer.write_u32::<LE>(val)?)
    }

    fn serialize_pointer(&mut self, ptr: Pointer) -> Result<()> {
        Ok(ptr.write_to(&mut self.writer)?)
    }

    fn serialize_i8_array(&mut self, arr: &[i8]) -> Result<()> {
        arr.iter().try_for_each(|&x| self.serialize_i8(x))
    }

    fn serialize_u8_array(&mut self, arr: &[u8]) -> Result<()> {
        arr.iter().try_for_each(|&x| self.serialize_u8(x))
    }

    fn serialize_i16_array(&mut self, arr: &[i16]) -> Result<()> {
        arr.iter().try_for_each(|&x| self.serialize_i16(x))
    }

    fn serialize_u16_array(&mut self, arr: &[u16]) -> Result<()> {
        arr.iter().try_for_each(|&x| self.serialize_u16(x))
    }

    fn serialize_i32_array(&mut self, arr: &[i32]) -> Result<()> {
        arr.iter().try_for_each(|&x| self.serialize_i32(x))
    }

    fn serialize_u32_array(&mut self, arr: &[u32]) -> Result<()> {
        arr.iter().try_for_each(|&x| self.serialize_u32(x))
    }

    fn serialize_pointer_array(&mut self, arr: &[Pointer]) -> Result<()> {
        arr.iter().try_for_each(|&x| self.serialize_pointer(x))
    }

    fn serialize_type(&mut self, ty: TypeOp) -> Result<()> {
        let opcode = Ggte::value(ty).map_err(Error::UnsupportedType)?;
        self.begin_expr(ExprOp::Imm32)?;
        self.serialize_i32(opcode)?;
        self.end_expr()
    }

    fn serialize_text(&mut self, text: &Text) -> Result<()> {
        self.writer.write_all(text.as_bytes())?;
        self.writer.write_u8(0)?;
        Ok(())
    }

    fn serialize_rgba(&mut self, rgba: u32) -> Result<()> {
        Ok(self.writer.write_u32::<BE>(rgba)?)
    }

    fn begin_expr(&mut self, expr: ExprOp) -> Result<()> {
        let opcode = Ggte::value(expr).map_err(Error::UnsupportedExpr)?;
        self.serialize_u8(opcode)
    }

    fn end_expr(&mut self) -> Result<()> {
        Ok(())
    }

    fn begin_command(&mut self, command: CmdOp) -> Result<()> {
        let opcode = Ggte::value(command).map_err(Error::UnsupportedCommand)?;
        self.serialize_u8(opcode)
    }

    fn end_command(&mut self) -> Result<()> {
        Ok(())
    }

    fn begin_call(&mut self) -> Result<()> {
        assert!(self.call_start_offset == u64::MAX, "Call start offset already set");
        // Write a command size of 0 for now
        self.call_start_offset = self.writer.stream_position()?;
        self.serialize_i16(0)
    }

    fn end_call(&mut self) -> Result<()> {
        assert!(self.call_start_offset < u64::MAX, "Call start offset not set");
        // Go back and fill in the command size
        let end_offset = self.writer.stream_position()?;
        let command_size =
            i16::try_from(end_offset - self.call_start_offset).expect("Call command size overflow");
        self.writer.seek(SeekFrom::Start(self.call_start_offset))?;
        self.serialize_i16(command_size)?;
        self.writer.seek(SeekFrom::Start(end_offset))?;
        self.call_start_offset = u64::MAX;
        Ok(())
    }

    fn begin_msg(&mut self) -> Result<()> {
        assert!(self.msg_start_offset == u64::MAX, "Message start offset already set");
        // Write an end offset of 0 for now
        self.msg_start_offset = self.writer.stream_position()?;
        self.serialize_i32(0)
    }

    fn serialize_msg_char(&mut self, ch: MsgOp) -> Result<()> {
        let b = Ggte::value(ch).map_err(Error::UnsupportedMsgChar)?;
        if let MsgOp::Char(_) = ch {
            if Ggte::get(b) != Ok(ch) {
                return Err(Error::InvalidMsgChar(b as u16));
            }
        }
        self.serialize_u8(b)
    }

    fn end_msg(&mut self) -> Result<()> {
        assert!(self.msg_start_offset < u64::MAX, "Message start offset not set");

        // Ensure we don't overflow the game's message buffer
        let end_offset = self.writer.stream_position()?;
        let msg_size = end_offset - self.msg_start_offset;
        if msg_size > MAX_MSG_SIZE {
            return Err(Error::MsgTooLarge { len: msg_size, max: MAX_MSG_SIZE });
        }

        // Now go back and fill in the end offset
        self.writer.seek(SeekFrom::Start(self.msg_start_offset))?;
        self.writer.write_rel_offset((end_offset - self.msg_start_offset).try_into().unwrap())?;
        self.writer.seek(SeekFrom::Start(end_offset))?;
        self.msg_start_offset = u64::MAX;
        Ok(())
    }
}

/// Event deserializer for reading events from .bin files.
pub struct BinDeserializer<R: Read + Seek> {
    reader: R,
    call_end_offset: u64, // u64::MAX if not set
    msg_end_offset: u64,  // u64::MAX if not set
}

impl<R: Read + Seek> BinDeserializer<R> {
    /// Creates a new `BinDeserializer` which wraps `reader`.
    pub fn new(reader: R) -> Self {
        Self { reader, call_end_offset: u64::MAX, msg_end_offset: u64::MAX }
    }

    /// Consumes the deserializer, returning the inner reader.
    pub fn into_inner(self) -> R {
        self.reader
    }
}

impl<R: Read + Seek> EventDeserializer for BinDeserializer<R> {
    fn deserialize_i8(&mut self) -> Result<i8> {
        Ok(self.reader.read_i8()?)
    }

    fn deserialize_u8(&mut self) -> Result<u8> {
        Ok(self.reader.read_u8()?)
    }

    fn deserialize_i16(&mut self) -> Result<i16> {
        Ok(self.reader.read_i16::<LE>()?)
    }

    fn deserialize_u16(&mut self) -> Result<u16> {
        Ok(self.reader.read_u16::<LE>()?)
    }

    fn deserialize_i32(&mut self) -> Result<i32> {
        Ok(self.reader.read_i32::<LE>()?)
    }

    fn deserialize_u32(&mut self) -> Result<u32> {
        Ok(self.reader.read_u32::<LE>()?)
    }

    fn deserialize_pointer(&mut self) -> Result<Pointer> {
        Ok(Pointer::read_from(&mut self.reader)?)
    }

    fn deserialize_i8_array(&mut self, len: usize) -> Result<Vec<i8>> {
        let mut arr = vec![0; len];
        self.reader.read_i8_into(&mut arr)?;
        Ok(arr)
    }

    fn deserialize_u8_array(&mut self, len: usize) -> Result<Vec<u8>> {
        let mut arr = vec![0; len];
        self.reader.read_exact(&mut arr)?;
        Ok(arr)
    }

    fn deserialize_i16_array(&mut self, len: usize) -> Result<Vec<i16>> {
        let mut arr = vec![0; len];
        self.reader.read_i16_into::<LE>(&mut arr)?;
        Ok(arr)
    }

    fn deserialize_u16_array(&mut self, len: usize) -> Result<Vec<u16>> {
        let mut arr = vec![0; len];
        self.reader.read_u16_into::<LE>(&mut arr)?;
        Ok(arr)
    }

    fn deserialize_i32_array(&mut self, len: usize) -> Result<Vec<i32>> {
        let mut arr = vec![0; len];
        self.reader.read_i32_into::<LE>(&mut arr)?;
        Ok(arr)
    }

    fn deserialize_u32_array(&mut self, len: usize) -> Result<Vec<u32>> {
        let mut arr = vec![0; len];
        self.reader.read_u32_into::<LE>(&mut arr)?;
        Ok(arr)
    }

    fn deserialize_pointer_array(&mut self, max_len: usize) -> Result<Vec<Pointer>> {
        let mut offsets = Vec::with_capacity(max_len);
        while offsets.len() < max_len {
            // We don't have any context on how the array is used, so assume that it
            // contains nothing but offsets and that it may be terminated by a zero value.
            let offset = self.reader.read_u32::<LE>()?;
            offsets.push(Pointer::Offset(offset));
            if offset == 0 {
                break;
            }
        }
        Ok(offsets)
    }

    fn deserialize_type(&mut self) -> Result<TypeOp> {
        // Do a lightweight expression deserialize so we don't have to depend on Expr
        let value = match self.begin_expr()? {
            ExprOp::Imm16 => self.deserialize_i16()?.into(),
            ExprOp::Imm32 => self.deserialize_i32()?,
            expr => return Err(Error::ExpectedType(expr)),
        };
        self.end_expr()?;
        Ggte::get(value).map_err(Error::UnrecognizedType)
    }

    fn deserialize_text(&mut self) -> Result<Text> {
        Ok(CString::read_from(&mut self.reader)?.into())
    }

    fn deserialize_rgba(&mut self) -> Result<u32> {
        Ok(self.reader.read_u32::<BE>()?)
    }

    fn begin_expr(&mut self) -> Result<ExprOp> {
        let opcode = self.reader.read_u8()?;
        Ggte::get(opcode).map_err(Error::UnrecognizedExpr)
    }

    fn end_expr(&mut self) -> Result<()> {
        Ok(())
    }

    fn begin_command(&mut self) -> Result<CmdOp> {
        let opcode = self.reader.read_u8()?;
        Ggte::get(opcode).map_err(Error::UnrecognizedCommand)
    }

    fn end_command(&mut self) -> Result<()> {
        Ok(())
    }

    fn begin_call(&mut self) -> Result<()> {
        assert!(self.call_end_offset == u64::MAX, "Call end offset already set");
        let start_offset = self.reader.stream_position()?;
        let command_size = self.reader.read_i16::<LE>()?;
        self.call_end_offset = start_offset + command_size as u64;
        Ok(())
    }

    fn have_call_arg(&mut self) -> Result<bool> {
        assert!(self.call_end_offset < u64::MAX, "Call end offset not set");
        Ok(self.reader.stream_position()? < self.call_end_offset)
    }

    fn end_call(&mut self) -> Result<()> {
        assert!(self.call_end_offset < u64::MAX, "Call end offset not set");
        self.call_end_offset = u64::MAX;
        Ok(())
    }

    fn begin_msg(&mut self) -> Result<()> {
        assert!(self.msg_end_offset == u64::MAX, "Message end offset already set");

        // The message string is prefixed with the offset of the next command to jump to.
        let end_offset = self.reader.read_i32::<LE>()? as u64;
        let start_offset = self.reader.stream_position()?;
        if end_offset <= start_offset {
            return Err(Error::InvalidMsgOffset { start: start_offset, end: end_offset });
        }

        self.msg_end_offset = end_offset;
        Ok(())
    }

    fn deserialize_msg_char(&mut self) -> Result<MsgOp> {
        let b = self.reader.read_u8()?;
        Ggte::get(b).map_err(|b| Error::UnrecognizedMsgChar(b as u16))
    }

    fn end_msg(&mut self) -> Result<()> {
        assert!(self.msg_end_offset < u64::MAX, "Message end offset not set");

        let offset = self.reader.stream_position()?;
        self.reader.seek(SeekFrom::Start(self.msg_end_offset))?;
        self.msg_end_offset = u64::MAX;

        if offset <= self.msg_end_offset {
            Ok(())
        } else {
            Err(Error::PassedEndOfMsg { offset, end: self.msg_end_offset })
        }
    }
}
