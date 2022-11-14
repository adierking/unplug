// Re-export so consumers don't have to depend on logos
pub use logos::Logos;

use logos::{Filter, Lexer};
use smol_str::SmolStr;
use std::fmt::{self, Display, Formatter};

/// Tokens which can appear in assembly source files.
#[derive(Logos, Debug, Clone, PartialEq, Eq, Hash)]
pub enum Token {
    #[regex(r"\n")]
    Newline,

    #[regex(r",")]
    Comma,

    #[regex(r"\(")]
    OpenParen,

    #[regex(r"\)")]
    CloseParen,

    #[regex(r":")]
    Colon,

    #[regex(r"\*")]
    Deref,

    #[regex(r"else")]
    Else,

    #[regex(r"[A-Za-z_][A-Za-z0-9_]*", identifier)]
    Identifier(SmolStr),

    #[regex(r"\.[A-Za-z_][A-Za-z0-9_]*", identifier_with_prefix)]
    Directive(SmolStr),

    #[regex(r"@[A-Za-z_][A-Za-z0-9_]*", identifier_with_prefix)]
    Type(SmolStr),

    #[regex(r#""[^"\n]*""#, string)]
    String(SmolStr),

    #[regex(r"-?[0-9]+", |lex| number(lex, 10, 0, 0))]
    #[regex(r"-?[0-9]+\.[bwd]", |lex| number(lex, 10, 0, 2))]
    #[regex(r"-?0x[0-9A-Fa-f]+", |lex| number(lex, 16, 2, 0))]
    #[regex(r"-?0x[0-9A-Fa-f]+\.[bwd]", |lex| number(lex, 16, 2, 2))]
    Number(Number),

    #[regex(r";[^\n]*", logos::skip)] // Skip line comments
    #[regex(r"/\*", block_comment)] // Skip block comments
    #[regex(r"[^\S\n]+", logos::skip)] // Skip whitespace
    #[error]
    Error,
}

impl From<Number> for Token {
    fn from(n: Number) -> Self {
        Self::Number(n)
    }
}

impl Display for Token {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Token::Newline => f.write_str("newline"),
            Token::Comma => f.write_str(","),
            Token::OpenParen => f.write_str("("),
            Token::CloseParen => f.write_str(")"),
            Token::Colon => f.write_str(":"),
            Token::Deref => f.write_str("*"),
            Token::Else => f.write_str("else"),
            Token::Identifier(s) => f.write_str(s.as_str()),
            Token::Directive(s) => f.write_str(s.as_str()),
            Token::Type(s) => f.write_str(s.as_str()),
            Token::String(s) => f.write_str(s.as_str()),
            Token::Number(n) => n.fmt(f),
            Token::Error => f.write_str("<error>"),
        }
    }
}

/// Number literal types.
///
/// All types use `i32`/`u32` so that we don't have to worry about sizes for the most part. The
/// actual conversion to the underlying types is done at codegen time so that we can handle auto
/// values the same as other types.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Number {
    /// An 8-bit signed integer
    I8(i32),
    /// An 8-bit unsigned integer
    U8(u32),
    /// A 16-bit signed integer
    I16(i32),
    /// A 16-bit unsigned integer
    U16(u32),
    /// A 32-bit signed integer
    I32(i32),
    /// A 32-bit unsigned integer
    U32(u32),
    /// Select the best storage class based on context (signed)
    IAuto(i32),
    /// Select the best storage class based on context (unsigned)
    UAuto(u32),
}

impl Display for Number {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Number::I8(x) => write!(f, "{}.b", x),
            Number::U8(x) => write!(f, "{}.b", x),
            Number::I16(x) => write!(f, "{}.w", x),
            Number::U16(x) => write!(f, "{}.w", x),
            Number::I32(x) => write!(f, "{}.d", x),
            Number::U32(x) => write!(f, "{}.d", x),
            Number::IAuto(x) => write!(f, "{}", x),
            Number::UAuto(x) => write!(f, "{}", x),
        }
    }
}

macro_rules! impl_number_from {
    ($from:ty, $variant:ident) => {
        impl From<$from> for Number {
            fn from(x: $from) -> Self {
                Self::$variant(x.into())
            }
        }
    };
}
impl_number_from!(i8, I8);
impl_number_from!(u8, U8);
impl_number_from!(i16, I16);
impl_number_from!(u16, U16);
impl_number_from!(i32, I32);
impl_number_from!(u32, U32);

/// Callback for identifiers
fn identifier(lex: &mut Lexer<'_, Token>) -> SmolStr {
    SmolStr::new(lex.slice())
}

/// Callback for identifiers with a 1-char prefix
fn identifier_with_prefix(lex: &mut Lexer<'_, Token>) -> SmolStr {
    SmolStr::new(&lex.slice()[1..])
}

/// Callback for string literals
fn string(lex: &mut Lexer<'_, Token>) -> SmolStr {
    let s = lex.slice();
    SmolStr::new(&s[1..s.len() - 1])
}

/// Callback for number literals
fn number(lex: &mut Lexer<'_, Token>, radix: u32, prefix: usize, suffix: usize) -> Option<Number> {
    // General format of a number literal is [-][prefix]<number>[suffix]
    // We need to extract the number, parse it, negate it if necessary, and then check the suffix
    let token = lex.slice();
    let negative = token.starts_with('-');
    let start = if negative { 1 + prefix } else { prefix };
    let end = token.len() - suffix;
    let value = u32::from_str_radix(&token[start..end], radix).ok()?;
    if negative {
        // Negative numbers are signed, nonnegative numbers are unsigned
        if value > i32::MIN as u32 {
            return None;
        }
        let signed = value.wrapping_neg() as i32;
        match &token[end..] {
            "" => Some(Number::IAuto(signed)),
            ".b" => Some(Number::I8(signed)),
            ".w" => Some(Number::I16(signed)),
            ".d" => Some(Number::I32(signed)),
            _ => None,
        }
    } else {
        match &token[end..] {
            "" => Some(Number::UAuto(value)),
            ".b" => Some(Number::U8(value)),
            ".w" => Some(Number::U16(value)),
            ".d" => Some(Number::U32(value)),
            _ => None,
        }
    }
}

/// Callback to skip block comments
fn block_comment(lex: &mut Lexer<'_, Token>) -> Filter<()> {
    if let Some(end) = lex.remainder().find("*/") {
        lex.bump(end + 2);
        Filter::Skip
    } else {
        Filter::Emit(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(s: &str) -> Vec<Token> {
        Token::lexer(s).collect()
    }

    #[test]
    fn test_basic() {
        assert_eq!(
            lex("(),:* else \r\n"),
            &[
                Token::OpenParen,
                Token::CloseParen,
                Token::Comma,
                Token::Colon,
                Token::Deref,
                Token::Else,
                Token::Newline,
            ]
        );
    }

    #[test]
    fn test_identifier() {
        assert_eq!(lex("foo_123"), &[Token::Identifier("foo_123".into())]);
        assert_eq!(lex(".foo_123"), &[Token::Directive("foo_123".into())]);
        assert_eq!(lex("@foo_123"), &[Token::Type("foo_123".into())]);
        assert_eq!(lex("else_"), &[Token::Identifier("else_".into())]);
    }

    #[test]
    fn test_string() {
        assert_eq!(lex("\"Hello, world!\""), &[Token::String("Hello, world!".into())]);
    }

    #[test]
    fn test_dec_number() {
        assert_eq!(lex("0"), &[Token::Number(Number::UAuto(0))]);
        assert_eq!(lex("123"), &[Token::Number(Number::UAuto(123))]);
        assert_eq!(lex("1234567890"), &[Token::Number(Number::UAuto(1234567890))]);

        assert_eq!(lex("123.b"), &[Token::Number(Number::U8(123))]);
        assert_eq!(lex("123.w"), &[Token::Number(Number::U16(123))]);
        assert_eq!(lex("123.d"), &[Token::Number(Number::U32(123))]);

        assert_eq!(lex("-0"), &[Token::Number(Number::IAuto(0))]);
        assert_eq!(lex("-123"), &[Token::Number(Number::IAuto(-123))]);
        assert_eq!(lex("-123.b"), &[Token::Number(Number::I8(-123))]);
        assert_eq!(lex("-123.w"), &[Token::Number(Number::I16(-123))]);
        assert_eq!(lex("-123.d"), &[Token::Number(Number::I32(-123))]);

        assert_eq!(lex("4294967295"), &[Token::Number(Number::UAuto(u32::MAX))]);
        assert_eq!(lex("4294967296"), &[Token::Error]);

        assert_eq!(lex("-2147483648"), &[Token::Number(Number::IAuto(i32::MIN))]);
        assert_eq!(lex("-2147483649"), &[Token::Error]);
    }

    #[test]
    fn test_hex_number() {
        assert_eq!(lex("0x0"), &[Token::Number(Number::UAuto(0))]);
        assert_eq!(lex("0x1f"), &[Token::Number(Number::UAuto(0x1f))]);
        assert_eq!(lex("0x01234567"), &[Token::Number(Number::UAuto(0x01234567))]);
        assert_eq!(lex("0x89abcdef"), &[Token::Number(Number::UAuto(0x89abcdef))]);
        assert_eq!(lex("0x89ABCDEF"), &[Token::Number(Number::UAuto(0x89abcdef))]);

        assert_eq!(lex("0x1f.b"), &[Token::Number(Number::U8(0x1f))]);
        assert_eq!(lex("0x1f.w"), &[Token::Number(Number::U16(0x1f))]);
        assert_eq!(lex("0x1f.d"), &[Token::Number(Number::U32(0x1f))]);

        assert_eq!(lex("-0x0"), &[Token::Number(Number::IAuto(0))]);
        assert_eq!(lex("-0x1f"), &[Token::Number(Number::IAuto(-0x1f))]);
        assert_eq!(lex("-0x1f.b"), &[Token::Number(Number::I8(-0x1f))]);
        assert_eq!(lex("-0x1f.w"), &[Token::Number(Number::I16(-0x1f))]);
        assert_eq!(lex("-0x1f.d"), &[Token::Number(Number::I32(-0x1f))]);

        assert_eq!(lex("0xffffffff"), &[Token::Number(Number::UAuto(u32::MAX))]);
        assert_eq!(lex("0x100000000"), &[Token::Error]);

        assert_eq!(lex("-0x80000000"), &[Token::Number(Number::IAuto(i32::MIN))]);
        assert_eq!(lex("-0x80000001"), &[Token::Error]);
    }

    #[test]
    fn test_line_comment() {
        assert_eq!(
            lex("abc ; def\nghi;"),
            &[Token::Identifier("abc".into()), Token::Newline, Token::Identifier("ghi".into())]
        )
    }

    #[test]
    fn test_block_comment() {
        assert_eq!(
            lex("abc /* def */ ghi /* j\nkl */ pqr\n"),
            &[
                Token::Identifier("abc".into()),
                Token::Identifier("ghi".into()),
                Token::Identifier("pqr".into()),
                Token::Newline,
            ]
        )
    }

    #[test]
    fn test_block_comment_unterminated() {
        assert_eq!(
            lex("abc /* def\nghi jkl\n"),
            &[
                Token::Identifier("abc".into()),
                Token::Error,
                Token::Identifier("def".into()),
                Token::Newline,
                Token::Identifier("ghi".into()),
                Token::Identifier("jkl".into()),
                Token::Newline,
            ]
        )
    }

    #[test]
    fn test_complex() {
        assert_eq!(
            lex("loc_0:\n\tif\teq(flag(123.d), 1.d), else *loc_1\n"),
            &[
                Token::Identifier("loc_0".into()),
                Token::Colon,
                Token::Newline,
                Token::Identifier("if".into()),
                Token::Identifier("eq".into()),
                Token::OpenParen,
                Token::Identifier("flag".into()),
                Token::OpenParen,
                Token::Number(Number::U32(123)),
                Token::CloseParen,
                Token::Comma,
                Token::Number(Number::U32(1)),
                Token::CloseParen,
                Token::Comma,
                Token::Else,
                Token::Deref,
                Token::Identifier("loc_1".into()),
                Token::Newline,
            ]
        );
    }
}
