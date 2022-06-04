use serde_json::ser::Formatter;
use std::io::{self, Write};

/// A JSON formatter which only indents up to a maximum amount and then inlines objects and arrays
/// beyond it.
pub struct MaxIndentJsonFormatter {
    indent: usize,
    max_indent: usize,
}

impl MaxIndentJsonFormatter {
    /// Create a new formatter with the max indent set.
    pub fn new(max_indent: usize) -> Self {
        Self { indent: 0, max_indent }
    }
}

impl Formatter for MaxIndentJsonFormatter {
    fn begin_array<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + Write,
    {
        self.indent += 1;
        writer.write_all(b"[")
    }

    fn end_array<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + Write,
    {
        self.indent -= 1;
        if self.indent < self.max_indent {
            writer.write_all(b"\n")?;
            writer.write_all("  ".repeat(self.indent).as_bytes())?;
        }
        writer.write_all(b"]")
    }

    fn begin_array_value<W>(&mut self, writer: &mut W, first: bool) -> io::Result<()>
    where
        W: ?Sized + Write,
    {
        if !first {
            writer.write_all(b",")?;
        }
        if self.indent <= self.max_indent {
            writer.write_all(b"\n")?;
            writer.write_all("  ".repeat(self.indent).as_bytes())?;
        } else if !first {
            writer.write_all(b" ")?;
        }
        Ok(())
    }

    fn begin_object<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + Write,
    {
        self.indent += 1;
        writer.write_all(b"{")
    }

    fn end_object<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + Write,
    {
        self.indent -= 1;
        if self.indent < self.max_indent {
            writer.write_all(b"\n")?;
            writer.write_all("  ".repeat(self.indent).as_bytes())?;
        }
        writer.write_all(b"}")
    }

    fn begin_object_key<W>(&mut self, writer: &mut W, first: bool) -> io::Result<()>
    where
        W: ?Sized + Write,
    {
        if !first {
            writer.write_all(b",")?;
        }
        if self.indent <= self.max_indent {
            writer.write_all(b"\n")?;
            writer.write_all("  ".repeat(self.indent).as_bytes())?;
        } else if !first {
            writer.write_all(b" ")?;
        }
        Ok(())
    }

    fn begin_object_value<W>(&mut self, writer: &mut W) -> io::Result<()>
    where
        W: ?Sized + Write,
    {
        writer.write_all(b": ")
    }
}
