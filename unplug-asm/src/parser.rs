use crate::ast::*;
use crate::diagnostics::{CompileOutput, Diagnostic};
use crate::lexer::{Token, TokenStream};
use crate::span::{Span, Spanned};

/// Parses a token stream into an AST with automatic error recovery.
pub struct Parser<'s> {
    /// The token stream.
    stream: Box<dyn TokenStream + 's>,
    /// The current token, or None if at EOF.
    token: Option<Token>,
    /// The current span, or empty if at EOF.
    span: Span,
    /// The next token and span, if known.
    next: Option<(Token, Span)>,
    /// The current diagnostic list.
    diagnostics: Vec<Diagnostic>,
}

impl<'s> Parser<'s> {
    /// Creates a new parser which reads from `stream`.
    pub fn new(stream: impl TokenStream + 's) -> Self {
        let mut parser = Self {
            stream: Box::from(stream),
            token: Some(Token::Error),
            span: Span::EMPTY,
            next: None,
            diagnostics: vec![],
        };
        parser.eat();
        parser
    }

    /// Parse the entire stream into an AST.
    pub fn parse(mut self) -> CompileOutput<Ast> {
        let mut items = vec![];
        while let Some(token) = &self.token {
            match token {
                // Ignore top-level newlines
                Token::Newline => self.eat(),
                _ => items.push(self.parse_item()),
            }
        }
        self.diagnostics.append(&mut self.stream.take_diagnostics());
        if self.diagnostics.is_empty() {
            let ast = Ast::with_items(items);
            CompileOutput::with_result(ast, self.diagnostics)
        } else {
            CompileOutput::err(self.diagnostics)
        }
    }

    /// Parses either a label declaration or a command.
    fn parse_item(&mut self) -> Item {
        match self.token {
            Some(Token::Identifier(_)) => {
                if self.peek(|t| *t == Token::Colon) {
                    Item::LabelDecl(self.parse_label_decl())
                } else {
                    Item::Command(self.parse_command())
                }
            }
            Some(Token::Error) => panic!("unfiltered error token"),
            _ => {
                self.report(Diagnostic::expected_item(self.span));
                // Recover by going to the next line
                self.skip_thru(|t| matches!(t, Token::Newline));
                Item::Error
            }
        }
    }

    /// Parses a label declaration.
    fn parse_label_decl(&mut self) -> LabelDecl {
        let name = self.parse_ident();
        let colon = self.parse_simple().unwrap();
        LabelDecl { name, colon_token: colon }
    }

    /// Parses a command.
    fn parse_command(&mut self) -> Command {
        let name = self.parse_ident();
        let operands = self.parse_operands();
        let command = Command { name, operands };

        // If the current token is not a newline, something is wrong.
        if self.have(|t| *t != Token::Newline) {
            if let Some(last) = command.operands.last() {
                // The command had at least one operand. Try to recover by parsing more operands in
                // case a comma was missing.
                let extra_operands = self.parse_operands();
                if !extra_operands.is_empty() {
                    // Yep, we got more operands, so a comma was probably missing.
                    self.report(Diagnostic::missing_comma(last.span().at_end(0)));
                } else {
                    // Nope, the command just needs to be followed by a newline.
                    self.report(Diagnostic::expected_newline(self.span));
                    self.skip_thru(|t| matches!(t, Token::Newline));
                }
            } else {
                // The command has no operands, so trying again wouldn't help.
                self.report(Diagnostic::expected_newline(self.span));
                self.skip_thru(|t| matches!(t, Token::Newline));
            }
        }
        command
    }

    /// Parses a list of zero or more operands separated by commas.
    fn parse_operands(&mut self) -> Vec<Operand> {
        let mut operands: Vec<Operand> = vec![];
        while let Some(token) = &self.token {
            if let Token::Newline | Token::RParen = token {
                // There's either a dangling comma or there are no operands
                break;
            }
            let operand = self.parse_operand();
            let done = operand.comma.is_none();
            operands.push(operand);
            if done {
                // No comma, this is the last operand
                break;
            }
        }

        // Detect a trailing comma
        if let Some(last) = operands.last() {
            if let Some(last_comma) = last.comma {
                self.report(Diagnostic::unexpected_token(&Token::Comma, last_comma));
            }
        }
        operands
    }

    /// Parses an operand optionally followed by a comma.
    fn parse_operand(&mut self) -> Operand {
        let expr = self.parse_expr();
        let comma = self.parse_simple();
        if comma.is_some() {
            // Newlines are permitted after commas because we know there's more left
            while let Some(Token::Newline) = self.token {
                self.eat();
            }
        }
        Operand { expr, comma }
    }

    /// Parses an expression.
    fn parse_expr(&mut self) -> Expr {
        loop {
            match &self.token {
                Some(Token::Newline) | None => {
                    // Always stop at newlines or EOF.
                    self.report(Diagnostic::expected_expr(self.span));
                    break Expr::Error;
                }

                Some(Token::Deref) => match self.parse_ref() {
                    Some(expr) => break expr,
                    // Continue on if nothing matched
                    None => self.eat(),
                },

                Some(Token::Else) => {
                    break Expr::ElseLabel(self.parse_else_label());
                }

                Some(Token::Identifier(_)) => {
                    // If there is a '(' ahead, this is a function call
                    if self.peek(|t| matches!(t, Token::LParen)) {
                        break Expr::FunctionCall(self.parse_call());
                    } else {
                        break Expr::Variable(self.parse_ident());
                    }
                }

                Some(Token::String(_)) => {
                    break Expr::StrLiteral(self.parse_str_literal());
                }

                Some(Token::Integer(_)) => {
                    break Expr::IntLiteral(self.parse_int_literal());
                }

                Some(t @ Token::Comma)
                | Some(t @ Token::LParen)
                | Some(t @ Token::RParen)
                | Some(t @ Token::Colon) => {
                    self.report(Diagnostic::unexpected_token(t, self.span));
                    // Ignore the bad token
                    self.eat();
                }

                Some(Token::Error) => panic!("unfiltered error token"),
            }
        }
    }

    /// Parses a reference.
    fn parse_ref(&mut self) -> Option<Expr> {
        if self.peek(|t| matches!(t, Token::Integer(_))) {
            Some(Expr::OffsetRef(self.parse_offset_ref()))
        } else if self.peek(|t| matches!(t, Token::Identifier(_))) {
            Some(Expr::LabelRef(self.parse_label_ref()))
        } else {
            self.report(Diagnostic::missing_deref_target(self.span));
            None
        }
    }

    /// Parses a label reference.
    fn parse_label_ref(&mut self) -> LabelRef {
        let deref_token = self.parse_simple::<Deref>().unwrap();
        let name = self.parse_ident();
        LabelRef { deref_token, name }
    }

    /// Parses an offset reference.
    fn parse_offset_ref(&mut self) -> OffsetRef {
        let deref_token = self.parse_simple::<Deref>().unwrap();
        let offset = self.parse_int_literal();
        OffsetRef { deref_token, offset }
    }

    /// Parses an "else label" reference.
    fn parse_else_label(&mut self) -> ElseLabel {
        let else_token = self.parse_simple::<Else>().unwrap();
        let deref_token = match self.parse_simple::<Deref>() {
            Some(t) => t,
            None => {
                self.report(Diagnostic::missing_deref(else_token));
                Deref::new(Span::EMPTY)
            }
        };
        let name = self.parse_ident();
        ElseLabel { else_token, deref_token, name }
    }

    /// Parses a function call.
    fn parse_call(&mut self) -> FunctionCall {
        let name = self.parse_ident();
        let lparen_token = self.parse_simple::<LParen>().unwrap();
        let operands = self.parse_operands();
        let rparen_token = match self.parse_simple::<RParen>() {
            Some(p) => p,
            None => {
                // TODO: Pick a better spot for the parenthesis depending on the function name. This
                // will suggest it at the end, which is not always right.
                self.report(Diagnostic::unclosed_parenthesis(lparen_token, self.span.with_len(0)));
                RParen::new(self.span)
            }
        };
        FunctionCall { name, lparen_token, operands, rparen_token }
    }

    /// Parses an identifier.
    fn parse_ident(&mut self) -> Ident {
        let token = self.token.take();
        if let Some(Token::Identifier(name)) = token {
            self.take(|_, span| Ident::new(name, span))
        } else {
            self.token = token;
            self.report(Diagnostic::expected_ident(self.span));
            Ident::new("", self.span)
        }
    }

    /// Parses an integer literal.
    fn parse_int_literal(&mut self) -> IntLiteral {
        if let Some(Token::Integer(i)) = self.token {
            self.take(|_, span| IntLiteral::new(i, span))
        } else {
            self.report(Diagnostic::expected_integer(self.span));
            IntLiteral::new(IntValue::U32(0), self.span)
        }
    }

    /// Parses a string literal.
    fn parse_str_literal(&mut self) -> StrLiteral {
        let token = self.token.take();
        if let Some(Token::String(s)) = token {
            self.take(|_, span| StrLiteral::with_escaped(s, span))
        } else {
            self.token = token;
            self.report(Diagnostic::expected_integer(self.span));
            StrLiteral::with_escaped("", self.span)
        }
    }

    /// Parses an optional `SimpleToken`.
    fn parse_simple<T: SimpleToken>(&mut self) -> Option<T> {
        if self.have(T::matches) {
            Some(self.take(|_, span| T::new(span)))
        } else {
            None
        }
    }

    /// Passes the current token and its span to `func`, eats the token, and returns the new value.
    ///
    /// If there is no current token, the function will be passed `Token::Error`.
    fn take<F, T>(&mut self, func: F) -> T
    where
        F: FnOnce(Token, Span) -> T,
    {
        let result = func(self.token.take().unwrap_or(Token::Error), self.span);
        self.eat();
        result
    }

    /// Consumes tokens until EOF is reached or `predicate` returns true. The token that passed the
    /// predicate will also be consumed.
    fn skip_thru<F>(&mut self, predicate: F)
    where
        F: Fn(&Token) -> bool,
    {
        let mut done = false;
        while self.token.is_some() && !done {
            done = predicate(self.token.as_ref().unwrap());
            self.eat();
        }
    }

    /// Consumes the current token and moves to the next one.
    fn eat(&mut self) {
        (self.token, self.span) = self
            .next
            .take()
            .or_else(|| self.read())
            .map_or((None, Span::EMPTY), |(t, s)| (Some(t), s));
    }

    /// Matches the current token against a predicate. Returns true if the predicate matches, false
    /// if it doesn't or EOF is reached.
    fn have<F>(&mut self, predicate: F) -> bool
    where
        F: FnOnce(&Token) -> bool,
    {
        self.token.as_ref().map_or(false, predicate)
    }

    /// Matches the lookahead token against a predicate. Returns true if the predicate matches,
    /// false if it doesn't or EOF is reached.
    fn peek<F>(&mut self, predicate: F) -> bool
    where
        F: FnOnce(&Token) -> bool,
    {
        if self.next.is_none() {
            self.next = self.read();
        }
        self.next.as_ref().map_or(false, |(token, _)| predicate(token))
    }

    /// Reads the next token from the stream until a non-error token or EOF is encountered.
    fn read(&mut self) -> Option<(Token, Span)> {
        let mut next = self.stream.next();
        while let Some((Token::Error, span)) = next {
            self.diagnostics.push(Diagnostic::invalid_token(span));
            next = self.stream.next();
        }
        next
    }

    /// Reports a diagnostic.
    fn report(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smol_str::SmolStr;
    use std::iter::FusedIterator;

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
        Operand { expr: value.into(), comma: Some(Comma::new(Span::EMPTY)) }
    }

    fn operand_end(value: impl Into<Expr>) -> Operand {
        Operand { expr: value.into(), comma: None }
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

    struct IterTokenStream<I: Iterator<Item = Token>>(I);

    impl<I: Iterator<Item = Token> + FusedIterator> Iterator for IterTokenStream<I> {
        type Item = (Token, Span);
        fn next(&mut self) -> Option<Self::Item> {
            self.0.next().map(|t| (t, Span::EMPTY))
        }
    }

    impl<I: Iterator<Item = Token> + FusedIterator> FusedIterator for IterTokenStream<I> {}

    impl<I: Iterator<Item = Token> + FusedIterator> TokenStream for IterTokenStream<I> {
        fn take_diagnostics(&mut self) -> Vec<Diagnostic> {
            vec![]
        }
    }

    fn parse(tokens: Vec<Token>) -> Vec<Item> {
        Parser::new(IterTokenStream(tokens.into_iter())).parse().unwrap().items
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
