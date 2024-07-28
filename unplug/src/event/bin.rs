use super::opcodes::{Atom, CmdOp, ExprOp, Ggte, MsgOp, OpcodeMap};
use super::pointer::{Pointer, WritePointer};
use super::serialize::{Error, EventDeserializer, EventSerializer, Result};
use crate::common::{CText, ReadFrom, VecText, WriteTo};
use byteorder::{ReadBytesExt, WriteBytesExt, BE, LE};
use std::io::{Read, Seek, SeekFrom, Write};

/// The maximum size of a serialized message command list in bytes.
const MAX_MSG_SIZE: u64 = 2048;

/// Variadic argument states.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum VariadicType {
    /// Arguments for an `anim`/`anim1`/`anim2` command.
    /// Stored as a list of expressions ending in -1.
    Anim,
    /// Arguments for a `call` command.
    /// Stored with a 16-bit argument list size in front.
    Call { start_offset: u64, size: u64 },
    /// Arguments for a `msg`/`select` command.
    /// Stored with a 32-bit absolute end offset in front.
    Msg { start_offset: u64, end_offset: u64 },
    /// Arguments for a `ptcl(@lead)` command.
    /// Stored with an argument count expression in front.
    PtclLead { count: u64 },
}

/// Event serializer for writing events to .bin files.
pub struct BinSerializer<W: Write + WritePointer + Seek> {
    writer: W,
    command: Option<CmdOp>,
    variadic: Option<VariadicType>,
}

impl<W: Write + WritePointer + Seek> BinSerializer<W> {
    /// Creates a new `BinSerializer` which wraps `writer`.
    pub fn new(writer: W) -> Self {
        Self { writer, command: None, variadic: None }
    }

    /// Consumes the serializer, returning the inner writer.
    pub fn into_inner(self) -> W {
        self.writer
    }

    /// Serializes an immediate value expression.
    fn serialize_imm_expr(&mut self, value: i32) -> Result<()> {
        self.begin_expr(ExprOp::Imm32)?;
        self.serialize_i32(value)?;
        self.end_expr()
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

    fn serialize_atom(&mut self, atom: Atom) -> Result<()> {
        let opcode = Ggte::value(atom).map_err(Error::UnsupportedAtom)?;
        self.serialize_imm_expr(opcode)
    }

    fn serialize_text(&mut self, text: &VecText) -> Result<()> {
        self.writer.write_all(text.as_raw_bytes())?;
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
        self.serialize_u8(opcode)?;
        self.command = Some(command);
        Ok(())
    }

    fn end_command(&mut self) -> Result<()> {
        self.command = None;
        Ok(())
    }

    fn begin_variadic_args(&mut self, count: usize) -> Result<()> {
        assert!(self.variadic.is_none(), "Cannot nest variadic argument lists");
        self.variadic = Some(match self.command {
            Some(CmdOp::Anim | CmdOp::Anim1 | CmdOp::Anim2) => VariadicType::Anim,
            Some(CmdOp::Call) => {
                // Write a command size of 0 for now
                let start_offset = self.writer.stream_position()?;
                self.serialize_i16(0)?;
                VariadicType::Call { start_offset, size: 0 }
            }
            Some(CmdOp::Msg | CmdOp::Select) => {
                // Write an end offset of 0 for now
                let start_offset = self.writer.stream_position()?;
                self.serialize_i32(0)?;
                VariadicType::Msg { start_offset, end_offset: 0 }
            }
            Some(CmdOp::Ptcl) => {
                self.serialize_imm_expr(count.try_into().expect("ptcl(@lead) arg count overflow"))?;
                VariadicType::PtclLead { count: count as u64 }
            }
            _ => return Err(Error::VariadicArgsNotSupported(self.command.unwrap_or_default())),
        });
        Ok(())
    }

    fn end_variadic_args(&mut self) -> Result<()> {
        match self.variadic.take() {
            Some(VariadicType::Anim) => {
                self.serialize_imm_expr(-1)?;
            }
            Some(VariadicType::Call { start_offset, .. }) => {
                // Go back and fill in the command size
                let end_offset = self.writer.stream_position()?;
                assert!(end_offset > start_offset, "Invalid call command end offset");
                let command_size =
                    i16::try_from(end_offset - start_offset).expect("Call command size overflow");
                self.writer.seek(SeekFrom::Start(start_offset))?;
                self.serialize_i16(command_size)?;
                self.writer.seek(SeekFrom::Start(end_offset))?;
            }
            Some(VariadicType::Msg { start_offset, .. }) => {
                // Ensure we don't overflow the game's message buffer
                let end_offset = self.writer.stream_position()?;
                let msg_size = end_offset - start_offset;
                if msg_size > MAX_MSG_SIZE {
                    return Err(Error::MsgTooLarge { len: msg_size, max: MAX_MSG_SIZE });
                }
                // Now go back and fill in the end offset
                self.writer.seek(SeekFrom::Start(start_offset))?;
                self.writer.write_rel_offset((end_offset - start_offset).try_into().unwrap())?;
                self.writer.seek(SeekFrom::Start(end_offset))?;
            }
            Some(VariadicType::PtclLead { .. }) => (),
            None => return Err(Error::NoVariadicArgList),
        }
        Ok(())
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
}

/// Event deserializer for reading events from .bin files.
pub struct BinDeserializer<R: Read + Seek> {
    reader: R,
    command: Option<CmdOp>,
    variadic: Option<VariadicType>,
    expr_level: usize,
}

impl<R: Read + Seek> BinDeserializer<R> {
    /// Creates a new `BinDeserializer` which wraps `reader`.
    pub fn new(reader: R) -> Self {
        Self { reader, command: None, variadic: None, expr_level: 0 }
    }

    /// Consumes the deserializer, returning the inner reader.
    pub fn into_inner(self) -> R {
        self.reader
    }

    /// Deserializes an immediate value expression. Fails with `ExpectedImmediate` if the expression
    /// is not an immediate value.
    fn deserialize_imm_expr(&mut self) -> Result<i32> {
        let value = match self.begin_expr()? {
            ExprOp::Imm16 => self.deserialize_i16()?.into(),
            ExprOp::Imm32 => self.deserialize_i32()?,
            expr => return Err(Error::ExpectedImmediate(expr)),
        };
        self.end_expr()?;
        Ok(value)
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

    fn deserialize_atom(&mut self) -> Result<Atom> {
        let value = match self.deserialize_imm_expr() {
            Ok(x) => x,
            Err(Error::ExpectedImmediate(_)) => return Err(Error::ExpectedAtom),
            Err(e) => return Err(e),
        };
        Ggte::get(value).map_err(Error::UnrecognizedAtom)
    }

    fn deserialize_text(&mut self) -> Result<VecText> {
        Ok(CText::read_from(&mut self.reader)?.convert().unwrap())
    }

    fn deserialize_rgba(&mut self) -> Result<u32> {
        Ok(self.reader.read_u32::<BE>()?)
    }

    fn begin_expr(&mut self) -> Result<ExprOp> {
        let opcode = self.reader.read_u8()?;
        let expr = Ggte::get(opcode).map_err(Error::UnrecognizedExpr)?;
        self.expr_level += 1;
        if self.expr_level == 1 {
            if let Some(VariadicType::PtclLead { count }) = &mut self.variadic {
                assert!(*count > 0, "Too many ptcl(@lead) arguments");
                *count -= 1;
            }
        }
        Ok(expr)
    }

    fn end_expr(&mut self) -> Result<()> {
        assert!(self.expr_level > 0, "Not in an expression");
        self.expr_level -= 1;
        Ok(())
    }

    fn begin_command(&mut self) -> Result<CmdOp> {
        let opcode = self.reader.read_u8()?;
        let command = Ggte::get(opcode).map_err(Error::UnrecognizedCommand)?;
        self.command = Some(command);
        Ok(command)
    }

    fn end_command(&mut self) -> Result<()> {
        self.command = None;
        Ok(())
    }

    fn begin_variadic_args(&mut self) -> Result<()> {
        assert!(self.variadic.is_none(), "Cannot nest variadic argument lists");
        self.variadic = Some(match self.command {
            Some(CmdOp::Anim | CmdOp::Anim1 | CmdOp::Anim2) => VariadicType::Anim,
            Some(CmdOp::Call) => {
                let start_offset = self.reader.stream_position()?;
                let size = self.reader.read_i16::<LE>()? as u64;
                let end_offset = start_offset.wrapping_add(size);
                if end_offset <= start_offset {
                    return Err(Error::InvalidArgListSize(size));
                }
                VariadicType::Call { start_offset, size }
            }
            Some(CmdOp::Msg | CmdOp::Select) => {
                let end_offset = self.reader.read_i32::<LE>()? as u64;
                let start_offset = self.reader.stream_position()?;
                if end_offset <= start_offset {
                    return Err(Error::InvalidEndOffset { start: start_offset, end: end_offset });
                }
                VariadicType::Msg { start_offset, end_offset }
            }
            Some(CmdOp::Ptcl) => {
                let count = self.deserialize_imm_expr()?;
                if count < 0 {
                    return Err(Error::InvalidArgListSize(count as u64));
                }
                VariadicType::PtclLead { count: count as u64 }
            }
            _ => return Err(Error::VariadicArgsNotSupported(self.command.unwrap_or_default())),
        });
        Ok(())
    }

    fn have_variadic_arg(&mut self) -> Result<bool> {
        match self.variadic {
            Some(VariadicType::Anim) => {
                // Look ahead at the next expression
                // TODO: Can we do this without seeking?
                let start_offset = self.reader.stream_position()?;
                let value = match self.deserialize_imm_expr() {
                    Ok(x) => x,
                    Err(Error::ExpectedImmediate(_)) => 0,
                    Err(e) => return Err(e),
                };
                self.reader.seek(SeekFrom::Start(start_offset))?;
                Ok(value >= 0)
            }
            Some(VariadicType::Call { start_offset, size }) => {
                let end_offset = start_offset + size;
                Ok(self.reader.stream_position()? < end_offset)
            }
            Some(VariadicType::Msg { end_offset, .. }) => {
                Ok(self.reader.stream_position()? < end_offset)
            }
            // The count is decremented in begin_expr()
            Some(VariadicType::PtclLead { count }) => Ok(count > 0),
            None => Err(Error::NoVariadicArgList),
        }
    }

    fn end_variadic_args(&mut self) -> Result<()> {
        match self.variadic.take() {
            Some(VariadicType::Anim) => {
                let value = self.deserialize_imm_expr()?;
                assert!(value < 0, "Too many anim arguments");
            }
            Some(VariadicType::Call { start_offset, size }) => {
                let end_offset = start_offset + size;
                let offset = self.reader.stream_position()?;
                if offset != end_offset {
                    return Err(Error::PassedEndOfArgList { offset, end: end_offset });
                }
                assert_eq!(self.reader.stream_position()?, end_offset);
            }
            Some(VariadicType::Msg { end_offset, .. }) => {
                let offset = self.reader.stream_position()?;
                self.reader.seek(SeekFrom::Start(end_offset))?;
                if offset > end_offset {
                    return Err(Error::PassedEndOfArgList { offset, end: end_offset });
                }
            }
            Some(VariadicType::PtclLead { count }) => {
                assert_eq!(count, 0, "Too many ptcl(@lead) arguments");
            }
            None => return Err(Error::NoVariadicArgList),
        }
        Ok(())
    }

    fn deserialize_msg_char(&mut self) -> Result<MsgOp> {
        let b = self.reader.read_u8()?;
        Ggte::get(b).map_err(|b| Error::UnrecognizedMsgChar(b as u16))
    }
}
