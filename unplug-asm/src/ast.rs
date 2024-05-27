use crate::lexer::Token;
use crate::span::{Span, Spanned};
use smol_str::SmolStr;
use std::fmt::{self, Debug, Display, Formatter};

/// Prefix for directive identifiers.
const DIRECTIVE_PREFIX: char = '.';
/// Prefix for atom identifiers.
const ATOM_PREFIX: char = '@';

/// Vertical tab character (`\v`).
const VT: &str = "\x0b";

/// Trait for an AST node which represents a token with no data.
pub trait SimpleToken {
    /// Creates a new instance of this token with the given span.
    fn new(span: Span) -> Self;

    /// Returns true if a token matches this node.
    fn matches(token: &Token) -> bool;
}

/// Macro for generating token types that have spans associated with them.
macro_rules! declare_tokens {
    {
        $($name:ident,)*
        $(,)*
    } => {
        $(
            #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
            pub struct $name {
                span: Span,
            }
            impl SimpleToken for $name {
                fn new(span: Span) -> Self {
                    Self { span }
                }

                fn matches(token: &Token) -> bool {
                    matches!(token, Token::$name)
                }
            }
            impl Spanned for $name {
                fn span(&self) -> Span {
                    self.span
                }
            }
        )*
    };
}
declare_tokens! {
    Comma,
    LParen,
    RParen,
    Colon,
    Deref,
    Else,
}

/// Identifiers are grouped into classes based on their first character.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IdentClass {
    /// The identifier does not begin with any special character.
    Default,
    /// The identifier is a directive (`.`).
    Directive,
    /// The identifier is an atom (`@`).
    Atom,
}

/// An identifier.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Ident {
    name: SmolStr,
    span: Span,
}

impl Ident {
    /// Creates a new identifier named `name` with span `span`.
    pub fn new(name: impl Into<SmolStr>, span: Span) -> Self {
        // TODO: Validate the name?
        Self { name: name.into(), span }
    }

    /// Returns the name of the identifier as a string.
    pub fn as_str(&self) -> &str {
        &self.name
    }

    /// Returns the identifier's class.
    pub fn class(&self) -> IdentClass {
        match self.name.chars().next() {
            Some(DIRECTIVE_PREFIX) => IdentClass::Directive,
            Some(ATOM_PREFIX) => IdentClass::Atom,
            _ => IdentClass::Default,
        }
    }
}

impl Debug for Ident {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.name, f)
    }
}

impl Display for Ident {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

impl Spanned for Ident {
    fn span(&self) -> Span {
        self.span
    }
}

/// An integer value.
///
/// All types use `i32`/`u32` so that we don't have to worry about sizes for the most part. The
/// actual conversion to the underlying types is done at codegen time so that we can handle auto
/// values the same as other types.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum IntValue {
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
    /// An integer could not be represented
    Error,
}

impl Display for IntValue {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::I8(x) => write!(f, "{}.b", x),
            Self::U8(x) => write!(f, "{}.b", x),
            Self::I16(x) => write!(f, "{}.w", x),
            Self::U16(x) => write!(f, "{}.w", x),
            Self::I32(x) => write!(f, "{}.d", x),
            Self::U32(x) => write!(f, "{}.d", x),
            Self::IAuto(x) => write!(f, "{}", x),
            Self::UAuto(x) => write!(f, "{}", x),
            Self::Error => write!(f, "error"),
        }
    }
}

macro_rules! impl_int_from {
    ($from:ty, $variant:ident) => {
        impl From<$from> for IntValue {
            fn from(x: $from) -> Self {
                Self::$variant(x.into())
            }
        }
    };
}
impl_int_from!(i8, I8);
impl_int_from!(u8, U8);
impl_int_from!(i16, I16);
impl_int_from!(u16, U16);
impl_int_from!(i32, I32);
impl_int_from!(u32, U32);

/// An integer literal expression.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct IntLiteral {
    value: IntValue,
    span: Span,
}

impl IntLiteral {
    /// Creates a new integer literal with `value` and `span`.
    pub fn new(value: impl Into<IntValue>, span: Span) -> Self {
        Self { value: value.into(), span }
    }

    /// Returns the literal's value.
    pub fn value(self) -> IntValue {
        self.value
    }

    #[must_use]
    pub fn with_value(self, value: IntValue) -> Self {
        Self { value, ..self }
    }
}

impl Debug for IntLiteral {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.value, f)
    }
}

impl Display for IntLiteral {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.value, f)
    }
}

impl Spanned for IntLiteral {
    fn span(&self) -> Span {
        self.span
    }
}

/// A string literal expression.
///
/// String literals always hold escaped Unicode text and there are facilities for converting between
/// escaped and unescaped text.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct StrLiteral {
    escaped: SmolStr,
    span: Span,
}

impl StrLiteral {
    /// Creates a string literal with the escaped string `escaped`.
    pub fn with_escaped(escaped: impl Into<SmolStr>, span: Span) -> Self {
        Self { escaped: escaped.into(), span }
    }

    /// Creates a string literal by escaping the string `unescaped`.
    pub fn from_unescaped(unescaped: impl AsRef<str>, span: Span) -> Self {
        Self::from_unescaped_impl(unescaped.as_ref(), span)
    }

    fn from_unescaped_impl(unescaped: &str, span: Span) -> Self {
        // TODO: Make this better and actually tokenize + error-check the string
        let escaped = unescaped.replace('\n', r"\n").replace(VT, r"\v");
        Self::with_escaped(escaped, span)
    }

    /// Returns the escaped string contents.
    pub fn as_escaped(&self) -> &str {
        &self.escaped
    }

    /// Converts the literal to an unescaped string.
    pub fn to_unescaped(&self) -> String {
        self.escaped.replace(r"\n", "\n").replace(r"\v", VT)
    }
}

impl Debug for StrLiteral {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.escaped, f)
    }
}

impl Spanned for StrLiteral {
    fn span(&self) -> Span {
        self.span
    }
}

/// A label reference expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LabelRef {
    /// The `*` token.
    pub deref_token: Deref,
    /// The name of the referenced label.
    pub name: Ident,
}

impl Spanned for LabelRef {
    fn span(&self) -> Span {
        self.deref_token.span().join(self.name.span())
    }
}

/// An "else label" reference expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ElseLabel {
    /// The `else` token.
    pub else_token: Else,
    /// The `*` token.
    pub deref_token: Deref,
    /// The name of the referenced label.
    pub name: Ident,
}

impl Spanned for ElseLabel {
    fn span(&self) -> Span {
        self.else_token.span().join(self.deref_token.span()).join(self.name.span())
    }
}

/// An offset reference expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OffsetRef {
    /// The `*` token.
    pub deref_token: Deref,
    /// The offset value.
    pub offset: IntLiteral,
}

impl Spanned for OffsetRef {
    fn span(&self) -> Span {
        self.deref_token.span().join(self.offset.span())
    }
}

/// An operand which may be followed by a comma.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Operand {
    /// The expression.
    pub expr: Expr,
    /// The `,` token (if present).
    pub comma: Option<Comma>,
}

impl Spanned for Operand {
    fn span(&self) -> Span {
        let comma_span = self.comma.map(|c| c.span()).unwrap_or_default();
        self.expr.span().join(comma_span)
    }
}

/// A function call expression.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionCall {
    /// The identifier for the name of the function being called.
    pub name: Ident,
    /// The `(` token.
    pub lparen_token: LParen,
    /// The function operands.
    pub operands: Vec<Operand>,
    /// The `)` token.
    pub rparen_token: RParen,
}

impl Spanned for FunctionCall {
    fn span(&self) -> Span {
        let operands = self.operands.iter().fold(Span::EMPTY, |s, o| s.join(o.span()));
        self.name
            .span()
            .join(self.lparen_token.span())
            .join(operands)
            .join(self.rparen_token.span())
    }
}

/// Expressions that may appear inside operands.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Expr {
    /// An integer literal.
    IntLiteral(IntLiteral),
    /// A string literal.
    StrLiteral(StrLiteral),
    /// A variable reference.
    Variable(Ident),
    /// A label reference.
    LabelRef(LabelRef),
    /// A label reference indicating it is an "else" condition.
    ElseLabel(ElseLabel),
    /// A raw file offset reference.
    OffsetRef(OffsetRef),
    /// A function call expression.
    FunctionCall(FunctionCall),
    Error,
}

impl Spanned for Expr {
    fn span(&self) -> Span {
        match self {
            Expr::IntLiteral(e) => e.span(),
            Expr::StrLiteral(e) => e.span(),
            Expr::Variable(e) => e.span(),
            Expr::LabelRef(e) => e.span(),
            Expr::ElseLabel(e) => e.span(),
            Expr::OffsetRef(e) => e.span(),
            Expr::FunctionCall(e) => e.span(),
            Expr::Error => Span::EMPTY,
        }
    }
}

macro_rules! impl_expr_from {
    ($from:ty, $variant:ident) => {
        impl From<$from> for Expr {
            fn from(x: $from) -> Self {
                Self::$variant(x.into())
            }
        }
    };
}
impl_expr_from!(IntLiteral, IntLiteral);
impl_expr_from!(StrLiteral, StrLiteral);
impl_expr_from!(Ident, Variable);
impl_expr_from!(LabelRef, LabelRef);
impl_expr_from!(ElseLabel, ElseLabel);
impl_expr_from!(OffsetRef, OffsetRef);
impl_expr_from!(FunctionCall, FunctionCall);

/// A command or directive invocation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Command {
    /// The identifier for the name of the command.
    pub name: Ident,
    /// The command operands.
    pub operands: Vec<Operand>,
}

impl Spanned for Command {
    fn span(&self) -> Span {
        let operands = self.operands.iter().fold(Span::EMPTY, |s, a| s.join(a.span()));
        self.name.span().join(operands)
    }
}

/// A label declaration.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LabelDecl {
    /// The identifier for the name of the label.
    pub name: Ident,
    /// The `:` token.
    pub colon_token: Colon,
}

impl Spanned for LabelDecl {
    fn span(&self) -> Span {
        self.name.span().join(self.colon_token.span())
    }
}

/// A top-level item in a program.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Item {
    Command(Command),
    LabelDecl(LabelDecl),
    Error,
}

impl Spanned for Item {
    fn span(&self) -> Span {
        match self {
            Self::Command(i) => i.span(),
            Self::LabelDecl(i) => i.span(),
            Self::Error => Span::EMPTY,
        }
    }
}

impl From<Command> for Item {
    fn from(cmd: Command) -> Self {
        Self::Command(cmd)
    }
}

impl From<LabelDecl> for Item {
    fn from(label: LabelDecl) -> Self {
        Self::LabelDecl(label)
    }
}

/// An abstract syntax tree for a full program.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Ast {
    pub items: Vec<Item>,
}

impl Ast {
    /// Creates an empty AST.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an AST initialized from `items`.
    pub fn with_items(items: impl Into<Vec<Item>>) -> Self {
        Self { items: items.into() }
    }
}
