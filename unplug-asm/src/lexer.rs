use crate::ast::IntValue;
use crate::span::Span;
use logos::{Filter, Lexer as LogosLexer, Logos, SpannedIter};
use smol_str::SmolStr;
use std::fmt::{self, Display, Formatter};
use std::iter::FusedIterator;

/// Tokens which can appear in assembly source files.
#[derive(Logos, Debug, Clone, PartialEq, Eq, Hash)]
pub enum Token {
    #[regex(r"\n")]
    Newline,

    #[regex(r",")]
    Comma,

    #[regex(r"\(")]
    LParen,

    #[regex(r"\)")]
    RParen,

    #[regex(r":")]
    Colon,

    #[regex(r"\*")]
    Deref,

    #[regex(r"else")]
    Else,

    #[regex(r"[.@]?[A-Za-z_][A-Za-z0-9_]*", identifier)]
    Identifier(SmolStr),

    #[regex(r#""[^"\n]*""#, string)]
    String(SmolStr),

    #[regex(r"-?[0-9]+", |lex| integer(lex, 10, 0, 0))]
    #[regex(r"-?[0-9]+\.[bwd]", |lex| integer(lex, 10, 0, 2))]
    #[regex(r"-?0x[0-9A-Fa-f]+", |lex| integer(lex, 16, 2, 0))]
    #[regex(r"-?0x[0-9A-Fa-f]+\.[bwd]", |lex| integer(lex, 16, 2, 2))]
    Integer(IntValue),

    #[regex(r";[^\n]*", logos::skip)] // Skip line comments
    #[regex(r"/\*", block_comment)] // Skip block comments
    #[regex(r"[^\S\n]+", logos::skip)] // Skip whitespace
    #[error]
    Error,
}

impl From<IntValue> for Token {
    fn from(n: IntValue) -> Self {
        Self::Integer(n)
    }
}

impl Display for Token {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Token::Newline => f.write_str("newline"),
            Token::Comma => f.write_str("','"),
            Token::LParen => f.write_str("'('"),
            Token::RParen => f.write_str("')'"),
            Token::Colon => f.write_str("':'"),
            Token::Deref => f.write_str("'*'"),
            Token::Else => f.write_str("'else'"),
            Token::Identifier(s) => f.write_str(s.as_str()),
            Token::String(s) => f.write_str(s.as_str()),
            Token::Integer(n) => n.fmt(f),
            Token::Error => f.write_str("error"),
        }
    }
}

/// Callback for identifiers
fn identifier(lex: &mut LogosLexer<'_, Token>) -> SmolStr {
    SmolStr::new(lex.slice())
}

/// Callback for string literals
fn string(lex: &mut LogosLexer<'_, Token>) -> SmolStr {
    let s = lex.slice();
    SmolStr::new(&s[1..s.len() - 1])
}

/// Callback for integer literals
fn integer(
    lex: &mut LogosLexer<'_, Token>,
    radix: u32,
    prefix: usize,
    suffix: usize,
) -> Option<IntValue> {
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
            "" => Some(IntValue::IAuto(signed)),
            ".b" => Some(IntValue::I8(signed)),
            ".w" => Some(IntValue::I16(signed)),
            ".d" => Some(IntValue::I32(signed)),
            _ => None,
        }
    } else {
        match &token[end..] {
            "" => Some(IntValue::UAuto(value)),
            ".b" => Some(IntValue::U8(value)),
            ".w" => Some(IntValue::U16(value)),
            ".d" => Some(IntValue::U32(value)),
            _ => None,
        }
    }
}

/// Callback to skip block comments
fn block_comment(lex: &mut LogosLexer<'_, Token>) -> Filter<()> {
    if let Some(end) = lex.remainder().find("*/") {
        lex.bump(end + 2);
        Filter::Skip
    } else {
        Filter::Emit(())
    }
}

/// A trait for iterators which iterate over tokens and their spans.
pub trait TokenIterator: Iterator<Item = (Token, Span)> + FusedIterator {}
impl<I> TokenIterator for I where I: Iterator<Item = (Token, Span)> + FusedIterator {}

/// Trait for a stream of tokens.
pub trait TokenStream<'s> {
    /// Converts the stream into a token iterator.
    fn into_tokens(self) -> Box<dyn TokenIterator + 's>;
}

/// Tokenizes source code.
pub struct Lexer<'s> {
    inner: SpannedIter<'s, Token>,
}

impl<'s> Lexer<'s> {
    /// Creates a new `Lexer` which reads from `source`.
    pub fn new(source: &'s str) -> Self {
        Self { inner: Token::lexer(source).spanned() }
    }
}

impl<'s> TokenStream<'s> for Lexer<'s> {
    fn into_tokens(self) -> Box<dyn TokenIterator + 's> {
        Box::new(self.inner.fuse().map(|(t, s)| (t, s.try_into().unwrap())))
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
                Token::LParen,
                Token::RParen,
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
        assert_eq!(lex(".foo_123"), &[Token::Identifier(".foo_123".into())]);
        assert_eq!(lex("@foo_123"), &[Token::Identifier("@foo_123".into())]);
        assert_eq!(lex("else_"), &[Token::Identifier("else_".into())]);
    }

    #[test]
    fn test_string() {
        assert_eq!(lex("\"Hello, world!\""), &[Token::String("Hello, world!".into())]);
    }

    #[test]
    fn test_dec_number() {
        assert_eq!(lex("0"), &[Token::Integer(IntValue::UAuto(0))]);
        assert_eq!(lex("123"), &[Token::Integer(IntValue::UAuto(123))]);
        assert_eq!(lex("1234567890"), &[Token::Integer(IntValue::UAuto(1234567890))]);

        assert_eq!(lex("123.b"), &[Token::Integer(IntValue::U8(123))]);
        assert_eq!(lex("123.w"), &[Token::Integer(IntValue::U16(123))]);
        assert_eq!(lex("123.d"), &[Token::Integer(IntValue::U32(123))]);

        assert_eq!(lex("-0"), &[Token::Integer(IntValue::IAuto(0))]);
        assert_eq!(lex("-123"), &[Token::Integer(IntValue::IAuto(-123))]);
        assert_eq!(lex("-123.b"), &[Token::Integer(IntValue::I8(-123))]);
        assert_eq!(lex("-123.w"), &[Token::Integer(IntValue::I16(-123))]);
        assert_eq!(lex("-123.d"), &[Token::Integer(IntValue::I32(-123))]);

        assert_eq!(lex("4294967295"), &[Token::Integer(IntValue::UAuto(u32::MAX))]);
        assert_eq!(lex("4294967296"), &[Token::Error]);

        assert_eq!(lex("-2147483648"), &[Token::Integer(IntValue::IAuto(i32::MIN))]);
        assert_eq!(lex("-2147483649"), &[Token::Error]);
    }

    #[test]
    fn test_hex_number() {
        assert_eq!(lex("0x0"), &[Token::Integer(IntValue::UAuto(0))]);
        assert_eq!(lex("0x1f"), &[Token::Integer(IntValue::UAuto(0x1f))]);
        assert_eq!(lex("0x01234567"), &[Token::Integer(IntValue::UAuto(0x01234567))]);
        assert_eq!(lex("0x89abcdef"), &[Token::Integer(IntValue::UAuto(0x89abcdef))]);
        assert_eq!(lex("0x89ABCDEF"), &[Token::Integer(IntValue::UAuto(0x89abcdef))]);

        assert_eq!(lex("0x1f.b"), &[Token::Integer(IntValue::U8(0x1f))]);
        assert_eq!(lex("0x1f.w"), &[Token::Integer(IntValue::U16(0x1f))]);
        assert_eq!(lex("0x1f.d"), &[Token::Integer(IntValue::U32(0x1f))]);

        assert_eq!(lex("-0x0"), &[Token::Integer(IntValue::IAuto(0))]);
        assert_eq!(lex("-0x1f"), &[Token::Integer(IntValue::IAuto(-0x1f))]);
        assert_eq!(lex("-0x1f.b"), &[Token::Integer(IntValue::I8(-0x1f))]);
        assert_eq!(lex("-0x1f.w"), &[Token::Integer(IntValue::I16(-0x1f))]);
        assert_eq!(lex("-0x1f.d"), &[Token::Integer(IntValue::I32(-0x1f))]);

        assert_eq!(lex("0xffffffff"), &[Token::Integer(IntValue::UAuto(u32::MAX))]);
        assert_eq!(lex("0x100000000"), &[Token::Error]);

        assert_eq!(lex("-0x80000000"), &[Token::Integer(IntValue::IAuto(i32::MIN))]);
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
                Token::LParen,
                Token::Identifier("flag".into()),
                Token::LParen,
                Token::Integer(IntValue::U32(123)),
                Token::RParen,
                Token::Comma,
                Token::Integer(IntValue::U32(1)),
                Token::RParen,
                Token::Comma,
                Token::Else,
                Token::Deref,
                Token::Identifier("loc_1".into()),
                Token::Newline,
            ]
        );
    }
}
