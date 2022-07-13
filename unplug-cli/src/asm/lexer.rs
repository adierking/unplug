use logos::{Filter, Lexer, Logos};
use smol_str::SmolStr;

/// Tokens which can appear in assembly source files.
#[derive(Logos, Debug, Clone, PartialEq, Eq)]
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

/// Number literal types.
///
/// All types use `u32` so that we don't have to worry about signed vs unsigned. The actual
/// conversion to the underlying types is done at codegen time so that we can handle auto values the
/// same as other types.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Number {
    /// An 8-bit integer
    Byte(u32),
    /// A 16-bit integer
    Word(u32),
    /// A 32-bit integer
    Dword(u32),
    /// Select the best storage class based on context
    Auto(u32),
}

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
    let mut value = u32::from_str_radix(&token[start..end], radix).ok()?;
    if negative {
        if value > i32::MIN as u32 {
            return None;
        }
        value = value.wrapping_neg();
    }
    match &token[end..] {
        "" => Some(Number::Auto(value)),
        ".b" => Some(Number::Byte(value)),
        ".w" => Some(Number::Word(value)),
        ".d" => Some(Number::Dword(value)),
        _ => None,
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
        assert_eq!(lex("0"), &[Token::Number(Number::Auto(0))]);
        assert_eq!(lex("123"), &[Token::Number(Number::Auto(123))]);
        assert_eq!(lex("1234567890"), &[Token::Number(Number::Auto(1234567890))]);

        assert_eq!(lex("123.b"), &[Token::Number(Number::Byte(123))]);
        assert_eq!(lex("123.w"), &[Token::Number(Number::Word(123))]);
        assert_eq!(lex("123.d"), &[Token::Number(Number::Dword(123))]);

        assert_eq!(lex("-0"), &[Token::Number(Number::Auto(0))]);
        assert_eq!(lex("-123"), &[Token::Number(Number::Auto(-123i32 as u32))]);
        assert_eq!(lex("-123.b"), &[Token::Number(Number::Byte(-123i32 as u32))]);

        assert_eq!(lex("4294967295"), &[Token::Number(Number::Auto(u32::MAX))]);
        assert_eq!(lex("4294967296"), &[Token::Error]);

        assert_eq!(lex("-2147483648"), &[Token::Number(Number::Auto(i32::MIN as u32))]);
        assert_eq!(lex("-2147483649"), &[Token::Error]);
    }

    #[test]
    fn test_hex_number() {
        assert_eq!(lex("0x0"), &[Token::Number(Number::Auto(0))]);
        assert_eq!(lex("0x1f"), &[Token::Number(Number::Auto(0x1f))]);
        assert_eq!(lex("0x01234567"), &[Token::Number(Number::Auto(0x01234567))]);
        assert_eq!(lex("0x89abcdef"), &[Token::Number(Number::Auto(0x89abcdef))]);
        assert_eq!(lex("0x89ABCDEF"), &[Token::Number(Number::Auto(0x89abcdef))]);

        assert_eq!(lex("0x1f.b"), &[Token::Number(Number::Byte(0x1f))]);
        assert_eq!(lex("0x1f.w"), &[Token::Number(Number::Word(0x1f))]);
        assert_eq!(lex("0x1f.d"), &[Token::Number(Number::Dword(0x1f))]);

        assert_eq!(lex("-0x0"), &[Token::Number(Number::Auto(0))]);
        assert_eq!(lex("-0x1f"), &[Token::Number(Number::Auto(-0x1fi32 as u32))]);
        assert_eq!(lex("-0x1f.b"), &[Token::Number(Number::Byte(-0x1fi32 as u32))]);

        assert_eq!(lex("0xffffffff"), &[Token::Number(Number::Auto(u32::MAX))]);
        assert_eq!(lex("0x100000000"), &[Token::Error]);

        assert_eq!(lex("-0x80000000"), &[Token::Number(Number::Auto(i32::MIN as u32))]);
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
                Token::Number(Number::Dword(123)),
                Token::CloseParen,
                Token::Comma,
                Token::Number(Number::Dword(1)),
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
