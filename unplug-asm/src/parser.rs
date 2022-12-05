use crate::ast::*;
use crate::lexer::{Token, TokenStream};
use crate::span::Span;
use chumsky::prelude::*;
use chumsky::Parser as _;
use chumsky::Stream;

/// The parser's error type.
pub type Error = Simple<Token, Span>;

pub struct Parser {
    parser: BoxedParser<'static, Token, Ast, Error>,
}

impl Parser {
    /// Builds a `Parser` for parsing tokens into an AST.
    pub fn new() -> Self {
        // NL*
        let newlines = just(Token::Newline).ignored().repeated();
        // NL | $
        let required_newline = just(Token::Newline).ignored().or(end());
        // `,` NL*
        let comma =
            just(Token::Comma).map_with_span(|_, s| Comma::new(s)).then_ignore(newlines.clone());

        // `(`
        let lparen = just(Token::LParen).map_with_span(|_, s| LParen::new(s));
        // `)`
        let rparen = just(Token::RParen).map_with_span(|_, s| RParen::new(s));

        // `:`
        let colon = just(Token::Colon).map_with_span(|_, s| Colon::new(s));
        // `*`
        let deref_token = just(Token::Deref).map_with_span(|_, s| Deref::new(s));
        // `else`
        let else_token = just(Token::Else).map_with_span(|_, s| Else::new(s));

        // IDENTIFIER
        let identifier = filter_map(|s, t: Token| match t {
            Token::Identifier(x) => Ok(Ident::new(x, s)),
            _ => Err(Error::custom(s, "expected an identifier")),
        });

        // INTEGER
        let integer = filter_map(|s, t: Token| match t {
            Token::Integer(x) => Ok(IntLiteral::new(x, s)),
            _ => Err(Error::custom(s, "expected an integer")),
        });

        // NUMBER | STRING | IDENTIFIER
        let value = filter_map(|s, t: Token| match t {
            Token::Integer(x) => Ok(Expr::IntLiteral(IntLiteral::new(x, s))),
            Token::String(x) => Ok(Expr::StrLiteral(StrLiteral::with_escaped(x, s))),
            Token::Identifier(x) => Ok(Expr::Variable(Ident::new(x, s))),
            _ => Err(Error::custom(s, "expected a value")),
        });

        // else `*` IDENTIFIER
        let else_deref = else_token
            .then(deref_token.clone())
            .then(identifier)
            .map(|((e, d), i)| ElseLabel { else_token: e, deref_token: d, name: i })
            .map(Expr::ElseLabel);

        // `*` IDENTIFIER
        let label_deref = deref_token
            .clone()
            .then(identifier)
            .map(|(d, i)| LabelRef { deref_token: d, name: i })
            .map(Expr::LabelRef);

        // `*` NUMBER
        let offset_deref = deref_token
            .then(integer)
            .map(|(d, n)| OffsetRef { deref_token: d, offset: n })
            .map(Expr::OffsetRef);

        // else_deref | label_deref | offset_deref
        let deref = else_deref.or(label_deref).or(offset_deref);

        let operands = recursive(|operands| {
            // IDENTIFIER `(` operands `)`
            let call = identifier
                .then(lparen)
                .then(operands)
                .then(rparen)
                .map(|(((i, l), o), r)| FunctionCall {
                    name: i,
                    lparen_token: l,
                    operands: o,
                    rparen_token: r,
                })
                .map(Expr::FunctionCall);

            // function | literal | deref
            let operand = call.or(value).or(deref);
            // ((operand comma)* operand))?
            operand
                .clone()
                .then(comma)
                .map(|(o, c)| Operand { expr: o.into(), comma: Some(c) })
                .repeated()
                .then(operand.map(|o| Operand { expr: o.into(), comma: None }))
                .or_not()
                .map(|o| match o {
                    Some((mut front, back)) => {
                        front.push(back);
                        front
                    }
                    None => vec![],
                })
        });

        // IDENTIFIER operands (NL | $)
        let command = identifier
            .then(operands.clone())
            .then_ignore(required_newline)
            .map(|(i, o)| Item::Command(Command { name: i, operands: o }));

        // IDENTIFIER `:`
        let label = identifier
            .then(colon)
            .map(|(i, c)| LabelDecl { name: i, colon_token: c })
            .map(Item::LabelDecl);

        // command | label
        let item = command.or(label);
        // (item (NL* item)*)?
        let items = item.separated_by(newlines.clone());
        // NL* items NL* $
        let program = items.padded_by(newlines).then_ignore(end());
        Self { parser: program.map(Ast::with_items).boxed() }
    }

    /// Parses an AST from `tokens`.
    pub fn parse<'s, S: TokenStream<'s>>(&self, tokens: S) -> Result<Ast, Vec<Error>> {
        let len = tokens.source_len();
        let stream = Stream::from_iter(Span::new(len, len + 1), tokens.into_tokens());
        self.parser.parse(stream)
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::TokenIterator;
    use crate::span::SourceOffset;
    use smol_str::SmolStr;

    fn id_token(name: impl Into<SmolStr>) -> Token {
        Token::Identifier(name.into())
    }

    fn id_node(name: impl Into<SmolStr>) -> Ident {
        Ident::new(name, Span::EMPTY)
    }

    fn str_token(val: impl Into<SmolStr>) -> Token {
        Token::String(val.into())
    }

    fn str_node(val: impl Into<SmolStr>) -> StrLiteral {
        StrLiteral::with_escaped(val, Span::EMPTY)
    }

    fn int_token(val: impl Into<IntValue>) -> Token {
        Token::Integer(val.into())
    }

    fn int_node(val: impl Into<IntValue>) -> IntLiteral {
        IntLiteral::new(val, Span::EMPTY)
    }

    fn label_node(name: impl Into<SmolStr>) -> LabelRef {
        LabelRef { deref_token: Deref::new(Span::EMPTY), name: id_node(name) }
    }

    fn else_label_node(name: impl Into<SmolStr>) -> ElseLabel {
        ElseLabel {
            else_token: Else::new(Span::EMPTY),
            deref_token: Deref::new(Span::EMPTY),
            name: id_node(name),
        }
    }

    fn offset_node(offset: impl Into<IntValue>) -> OffsetRef {
        OffsetRef { deref_token: Deref::new(Span::EMPTY), offset: int_node(offset.into()) }
    }

    fn operand_comma(value: impl Into<Expr>) -> Operand {
        Operand { expr: Box::new(value.into()), comma: Some(Comma::new(Span::EMPTY)) }
    }

    fn operand_end(value: impl Into<Expr>) -> Operand {
        Operand { expr: Box::new(value.into()), comma: None }
    }

    fn call_node(name: impl Into<SmolStr>, operands: Vec<Operand>) -> FunctionCall {
        FunctionCall {
            name: id_node(name),
            lparen_token: LParen::new(Span::EMPTY),
            operands,
            rparen_token: RParen::new(Span::EMPTY),
        }
    }

    fn cmd(opcode: impl Into<SmolStr>, operands: Vec<Operand>) -> Item {
        Command { name: id_node(opcode), operands }.into()
    }

    fn label_decl(name: impl Into<SmolStr>) -> Item {
        LabelDecl { name: id_node(name), colon_token: Colon::new(Span::EMPTY) }.into()
    }

    struct VecTokenStream(Vec<Token>);

    impl TokenStream<'static> for VecTokenStream {
        fn source_len(&self) -> SourceOffset {
            0
        }

        fn into_tokens(self) -> Box<TokenIterator<'static>> {
            Box::new(self.0.into_iter().map(|t| (t, Span::EMPTY)))
        }
    }

    fn parse(tokens: Vec<Token>) -> Vec<Item> {
        Parser::new().parse(VecTokenStream(tokens)).unwrap().items
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
        assert_eq!(parse(vec![id_token("return")]), &[cmd("return", vec![])]);
    }

    #[test]
    fn test_parse_command_one_operand() {
        assert_eq!(
            parse(vec![id_token("lib"), int_token(123)]),
            &[cmd("lib", vec![operand_end(int_node(123))])]
        );
    }

    #[test]
    fn test_parse_command_two_operands() {
        assert_eq!(
            parse(vec![id_token("disp"), int_token(20000), Token::Comma, int_token(1)]),
            &[cmd("disp", vec![operand_comma(int_node(20000)), operand_end(int_node(1))])]
        );
    }

    #[test]
    fn test_parse_command_multiline() {
        assert_eq!(
            parse(vec![
                id_token("disp"),
                int_token(20000),
                Token::Comma,
                Token::Newline,
                Token::Newline,
                int_token(1),
            ]),
            &[cmd("disp", vec![operand_comma(int_node(20000)), operand_end(int_node(1))])]
        );
    }

    #[test]
    fn test_parse_multiple_commands() {
        assert_eq!(
            parse(vec![id_token("lib"), int_token(123), Token::Newline, id_token("return")]),
            &[cmd("lib", vec![operand_end(int_node(123))]), cmd("return", vec![])]
        );
    }

    #[test]
    fn test_parse_multiple_commands_empty_lines() {
        assert_eq!(
            parse(vec![
                Token::Newline,
                Token::Newline,
                id_token("lib"),
                int_token(123),
                Token::Newline,
                Token::Newline,
                id_token("return"),
                Token::Newline,
                Token::Newline,
            ]),
            &[cmd("lib", vec![operand_end(int_node(123))]), cmd("return", vec![])]
        );
    }

    #[test]
    fn test_parse_string() {
        assert_eq!(
            parse(vec![id_token("msg"), str_token("foo")]),
            &[cmd("msg", vec![operand_end(str_node("foo"))])]
        );
    }

    #[test]
    fn test_parse_type() {
        assert_eq!(
            parse(vec![id_token("wait"), id_token("@read")]),
            &[cmd("wait", vec![operand_end(id_node("@read"))])]
        );
    }

    #[test]
    fn test_parse_deref() {
        assert_eq!(
            parse(vec![
                id_token("if"),
                int_token(1),
                Token::Comma,
                Token::Else,
                Token::Deref,
                id_token("loc_0"),
                Token::Newline,
                id_token("if"),
                int_token(1),
                Token::Comma,
                Token::Deref,
                id_token("loc_1"),
                Token::Newline,
                id_token("if"),
                int_token(1),
                Token::Comma,
                Token::Deref,
                int_token(2),
            ]),
            &[
                cmd("if", vec![operand_comma(int_node(1)), operand_end(else_label_node("loc_0"))]),
                cmd("if", vec![operand_comma(int_node(1)), operand_end(label_node("loc_1"))]),
                cmd("if", vec![operand_comma(int_node(1)), operand_end(offset_node(2))]),
            ]
        );
    }

    #[test]
    fn test_parse_function() {
        assert_eq!(
            parse(vec![
                id_token("if"),
                id_token("not"),
                Token::LParen,
                id_token("eq"),
                Token::LParen,
                id_token("hold"),
                Token::Comma,
                int_token(0),
                Token::RParen,
                Token::RParen,
            ]),
            &[cmd(
                "if",
                vec![operand_end(call_node(
                    "not",
                    vec![operand_end(call_node(
                        "eq",
                        vec![operand_comma(id_node("hold")), operand_end(int_node(0))]
                    ))]
                ))]
            )]
        );
    }

    #[test]
    fn test_parse_label_decl() {
        assert_eq!(parse(vec![id_token("loc_0"), Token::Colon]), vec![label_decl("loc_0")]);
    }

    #[test]
    fn test_parse_multiple_label_decls() {
        assert_eq!(
            parse(vec![
                id_token("loc_0"),
                Token::Colon,
                id_token("loc_1"),
                Token::Colon,
                Token::Newline,
                id_token("loc_2"),
                Token::Colon
            ]),
            vec![label_decl("loc_0"), label_decl("loc_1"), label_decl("loc_2")]
        );
    }

    #[test]
    fn test_parse_mixed_items() {
        assert_eq!(
            parse(vec![
                id_token("loc_0"),
                Token::Colon,
                id_token("return"),
                Token::Newline,
                id_token("loc_1"),
                Token::Colon,
                id_token("loc_2"),
                Token::Colon,
                id_token(".db"),
                int_token(0),
            ]),
            vec![
                label_decl("loc_0"),
                cmd("return", vec![]),
                label_decl("loc_1"),
                label_decl("loc_2"),
                cmd(".db", vec![operand_end(int_node(0))]),
            ]
        );
    }
}
