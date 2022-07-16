use super::{Number, Token};
use chumsky::prelude::*;
use smol_str::SmolStr;

/// The parser's error type.
pub type Error = Simple<Token>;

/// A value in an assembly program.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Value {
    /// A number literal.
    Number(Number),
    /// A string literal.
    Text(SmolStr),
    /// A label reference.
    Label(SmolStr),
    /// A label reference indicating it is an "else" condition.
    ElseLabel(SmolStr),
    /// A raw file offset reference.
    Offset(Number),
    /// A type expression.
    Type(SmolStr),
    /// A function call expression.
    Function(SmolStr, Vec<Value>),
}

impl From<Number> for Value {
    fn from(n: Number) -> Self {
        Self::Number(n)
    }
}

/// A label, command, or directive in a program.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Item {
    /// A label declaration.
    Label(SmolStr),
    /// A script command.
    Command(SmolStr, Vec<Value>),
    /// An assembler directive.
    Directive(SmolStr, Vec<Value>),
}

/// An abstract syntax tree of a program.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct Ast {
    pub items: Vec<Item>,
}

impl Ast {
    /// Creates an empty `Ast`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a `Parser` for parsing tokens into an AST.
    pub fn parser() -> BoxedParser<'static, Token, Ast, Error> {
        let identifier = select! { Token::Identifier(x) => x };
        let number = select! { Token::Number(x) => x };

        // NUMBER | STRING | TYPE
        let literal = select! {
            Token::Number(x) => Value::Number(x),
            Token::String(x) => Value::Text(x),
            Token::Type(x) => Value::Type(x),
        };

        // NL*
        let newlines = just(Token::Newline).ignored().repeated();
        // `,` NL*
        let comma = just(Token::Comma).ignore_then(newlines.clone());

        // else `*` IDENTIFIER
        let else_deref = just(Token::Else)
            .ignore_then(just(Token::Deref))
            .ignore_then(identifier)
            .map(Value::ElseLabel);

        // `*` IDENTIFIER
        let label_deref = just(Token::Deref).ignore_then(identifier).map(Value::Label);
        // `*` NUMBER
        let offset_deref = just(Token::Deref).ignore_then(number).map(Value::Offset);

        // else_deref | label_deref | offset_deref
        let deref = else_deref.or(label_deref).or(offset_deref);

        let operands = recursive(|operands| {
            // `(` operands `)`
            let args = operands.delimited_by(just(Token::OpenParen), just(Token::CloseParen));
            // IDENTIFIER args?
            let function = identifier
                .then(args.or_not())
                .map(|(i, o)| Value::Function(i, o.unwrap_or_default()));

            // literal | function | deref
            let operand = literal.or(function).or(deref);
            // (operand (comma operand)*)?
            operand.separated_by(comma)
        });

        // IDENTIFIER operands
        let command = identifier.then(operands.clone()).map(|(i, o)| Item::Command(i, o));

        // DIRECTIVE operands
        let directive =
            select! { Token::Directive(x) => x }.then(operands).map(|(i, o)| Item::Directive(i, o));

        // (command | directive) (NL | $)
        let required_newline = just(Token::Newline).ignored().or(end());
        let op = command.or(directive).then_ignore(required_newline);

        // IDENTIFIER `:`
        let label = identifier.then_ignore(just(Token::Colon)).map(Item::Label);

        // op | label
        let item = op.or(label);
        // (item (NL* item)*)?
        let items = item.separated_by(newlines.clone());
        // NL* items NL* $
        items.padded_by(newlines).then_ignore(end()).map(Self::from).boxed()
    }
}

impl From<Vec<Item>> for Ast {
    fn from(items: Vec<Item>) -> Self {
        Self { items }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: impl Into<SmolStr>) -> Token {
        Token::Identifier(s.into())
    }

    fn dirid(s: impl Into<SmolStr>) -> Token {
        Token::Directive(s.into())
    }

    fn num(val: i32) -> Number {
        Number::Auto(val as u32)
    }

    fn func(opcode: impl Into<SmolStr>, operands: Vec<Value>) -> Value {
        Value::Function(opcode.into(), operands)
    }

    fn cmd(opcode: impl Into<SmolStr>, operands: Vec<Value>) -> Item {
        Item::Command(opcode.into(), operands)
    }

    fn label(name: impl Into<SmolStr>) -> Item {
        Item::Label(name.into())
    }

    fn dir(name: impl Into<SmolStr>, operands: Vec<Value>) -> Item {
        Item::Directive(name.into(), operands)
    }

    fn parse(tokens: Vec<Token>) -> Vec<Item> {
        Ast::parser().parse(tokens).unwrap().items
    }

    #[test]
    fn test_parse_nothing() {
        assert_eq!(parse(vec![]), &[]);
    }

    #[test]
    fn test_parse_newlines() {
        assert_eq!(parse(vec![Token::Newline, Token::Newline, Token::Newline]), &[]);
    }

    #[test]
    fn test_parse_command_no_operands() {
        assert_eq!(parse(vec![id("return")]), &[cmd("return", vec![])]);
    }

    #[test]
    fn test_parse_command_one_operand() {
        assert_eq!(parse(vec![id("lib"), num(123).into()]), &[cmd("lib", vec![num(123).into()])]);
    }

    #[test]
    fn test_parse_command_two_operands() {
        assert_eq!(
            parse(vec![id("disp"), num(20000).into(), Token::Comma, num(1).into()]),
            &[cmd("disp", vec![num(20000).into(), num(1).into()])]
        );
    }

    #[test]
    fn test_parse_command_multiline() {
        assert_eq!(
            parse(vec![
                id("disp"),
                num(20000).into(),
                Token::Comma,
                Token::Newline,
                Token::Newline,
                num(1).into()
            ]),
            &[cmd("disp", vec![num(20000).into(), num(1).into()])]
        );
    }

    #[test]
    fn test_parse_multiple_commands() {
        assert_eq!(
            parse(vec![id("lib"), num(123).into(), Token::Newline, id("return")]),
            &[cmd("lib", vec![num(123).into()]), cmd("return", vec![])]
        );
    }

    #[test]
    fn test_parse_multiple_commands_empty_lines() {
        assert_eq!(
            parse(vec![
                Token::Newline,
                Token::Newline,
                id("lib"),
                num(123).into(),
                Token::Newline,
                Token::Newline,
                id("return"),
                Token::Newline,
                Token::Newline,
            ]),
            &[cmd("lib", vec![num(123).into()]), cmd("return", vec![])]
        );
    }

    #[test]
    fn test_parse_string() {
        assert_eq!(
            parse(vec![id("msg"), Token::String("foo".into())]),
            &[cmd("msg", vec![Value::Text("foo".into())])]
        );
    }

    #[test]
    fn test_parse_type() {
        assert_eq!(
            parse(vec![id("wait"), Token::Type("read".into())]),
            &[cmd("wait", vec![Value::Type("read".into())])]
        );
    }

    #[test]
    fn test_parse_deref() {
        assert_eq!(
            parse(vec![
                id("if"),
                num(1).into(),
                Token::Comma,
                Token::Else,
                Token::Deref,
                id("loc_0"),
                Token::Newline,
                id("if"),
                num(1).into(),
                Token::Comma,
                Token::Deref,
                id("loc_1"),
                Token::Newline,
                id("if"),
                num(1).into(),
                Token::Comma,
                Token::Deref,
                num(2).into(),
            ]),
            &[
                cmd("if", vec![num(1).into(), Value::ElseLabel("loc_0".into())]),
                cmd("if", vec![num(1).into(), Value::Label("loc_1".into())]),
                cmd("if", vec![num(1).into(), Value::Offset(num(2))]),
            ]
        );
    }

    #[test]
    fn test_parse_function() {
        assert_eq!(
            parse(vec![
                id("if"),
                id("not"),
                Token::OpenParen,
                id("eq"),
                Token::OpenParen,
                id("hold"),
                Token::Comma,
                num(0).into(),
                Token::CloseParen,
                Token::CloseParen,
            ]),
            &[cmd(
                "if",
                vec![func("not", vec![func("eq", vec![func("hold", vec![]), num(0).into()])])]
            )]
        );
    }

    #[test]
    fn test_parse_label() {
        assert_eq!(parse(vec![id("loc_0"), Token::Colon]), vec![Item::Label("loc_0".into())]);
    }

    #[test]
    fn test_parse_multiple_labels() {
        assert_eq!(
            parse(vec![
                id("loc_0"),
                Token::Colon,
                id("loc_1"),
                Token::Colon,
                Token::Newline,
                id("loc_2"),
                Token::Colon
            ]),
            vec![label("loc_0"), label("loc_1"), label("loc_2")]
        );
    }

    #[test]
    fn test_parse_directive() {
        assert_eq!(parse(vec![dirid("db"), num(0).into()]), vec![dir("db", vec![num(0).into()])]);
    }

    #[test]
    fn test_parse_mixed_items() {
        assert_eq!(
            parse(vec![
                id("loc_0"),
                Token::Colon,
                id("return"),
                Token::Newline,
                id("loc_1"),
                Token::Colon,
                id("loc_2"),
                Token::Colon,
                dirid("db"),
                num(0).into(),
            ]),
            vec![
                label("loc_0"),
                cmd("return", vec![]),
                label("loc_1"),
                label("loc_2"),
                dir("db", vec![num(0).into()]),
            ]
        );
    }
}
