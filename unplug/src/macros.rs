/// Generates a `From` implementation for an error type which boxes another error type.
///
/// # Examples
/// ```
/// # use std::io;
/// # use thiserror::Error;
/// # use unplug::from_error_boxed;
/// #[derive(Error, Debug)]
/// enum MyError {
///     #[error(transparent)]
///     Io(Box<io::Error>),
/// }
///
/// from_error_boxed!(MyError::Io, io::Error);
/// ```
#[macro_export]
macro_rules! from_error_boxed {
    ($enum:ident :: $name:ident, $err:ty) => {
        impl ::std::convert::From<$err> for $enum {
            fn from(err: $err) -> Self {
                Self::$name(::std::boxed::Box::new(err))
            }
        }
    };
}

/// Builds up an `Expr` using a simple grammar. **There is no operator precedence** - tokens are
/// processed from left-to-right. Use parentheses to group expressions, and use `!(...)` for logical
/// negation.
///
/// # Examples
///
/// Basic usage:
/// ```
/// # use unplug::expr;
/// let a = expr![1];
/// let b = expr![1 + 2];
/// let c = expr![1 + 2 - 3 * 4 / 5 % 6];
/// let d = expr![!(1 == 0)];
/// let e = expr![1 && !(1 - 1)];
/// ```
///
/// Built-in arrays:
/// ```
/// # use unplug::expr;
/// # use unplug::data::{Atc, Item};
/// let a = expr![atc[1] != 0];
/// let b = expr![atc[Atc::Toothbrush] != 0];
/// let c = expr![item[42] != 0];
/// let d = expr![item[Item::HotRod] != 0];
/// let e = expr![atc[1] != 0 && flag[123]];
/// ```
///
/// Variable references:
/// ```
/// # use unplug::event::expr::{BinaryOp, Expr};
/// # use unplug::expr;
/// let a = expr![1];
/// let b = expr![2];
/// let c = expr![a + b];
/// assert_eq!(c, Expr::Add(BinaryOp::new(Expr::Imm16(1), Expr::Imm16(2)).into()));
/// ```
///
/// Embedded Rust code:
/// ```
/// # use unplug::event::expr::{BinaryOp, Expr};
/// # use unplug::expr;
/// fn foo() -> i32 {
///     41
/// }
///
/// let a = expr![var[{ foo() + 1 }]];
/// assert_eq!(a, Expr::Variable(Expr::Imm16(42).into()));
/// ```
#[macro_export]
macro_rules! expr {
    // Single value processing
    (@value $imm:literal) => { $crate::event::Expr::imm($imm) };
    (@value !($($group:tt)+)) => { $crate::event::Expr::Not(expr![$($group)+].into()) };
    (@value ($($group:tt)+)) => { expr![$($group)+] };
    (@value {$($group:tt)+}) => { expr!(@rust {$($group)+}) };
    (@value $var:ident) => { expr!(@rust $var) };
    (@value $($tail:tt)*) => { compile_error!("invalid expression"); };

    // Rust expression embedding
    (@rust $e:expr) => { $crate::event::Expr::from($e) };
    (@rust $($tail:tt)*) => { compile_error("invalid Rust expression"); };

    // Array access processing
    (@array atc[$id:path]) => { $crate::event::Expr::Atc(::std::boxed::Box::new($id.into())) };
    (@array atc[$($index:tt)+]) => { $crate::event::Expr::Atc(expr![$($index)+].into()) };
    (@array battery[$($index:tt)+]) => { $crate::event::Expr::Battery(expr![$($index)+].into()) };
    (@array flag[$($index:tt)+]) => { $crate::event::Expr::Flag(expr![$($index)+].into()) };
    (@array item[$id:path]) => { $crate::event::Expr::Item(::std::boxed::Box::new($id.into())) };
    (@array item[$($index:tt)+]) => { $crate::event::Expr::Item(expr![$($index)+].into()) };
    (@array map[$($index:tt)+]) => { $crate::event::Expr::Map(expr![$($index)+].into()) };
    (@array pad[$($index:tt)+]) => { $crate::event::Expr::Pad(expr![$($index)+].into()) };
    (@array time[$($index:tt)+]) => { $crate::event::Expr::Time(expr![$($index)+].into()) };
    (@array var[$($index:tt)+]) => { $crate::event::Expr::Variable(expr![$($index)+].into()) };
    (@array $($tail:tt)*) => { compile_error!("invalid array") };

    // Binary operator helper
    (@binop ($op:ident, $lhs:expr, $array:ident[$($index:tt)+] $($tail:tt)*)) => {{
        let op = $crate::event::expr::BinaryOp::new($lhs, expr!(@array $array[$($index)+]));
        let acc = $crate::event::Expr::$op(op.into());
        expr!(@op (acc) $($tail)*)
    }};
    (@binop ($op:ident, $lhs:expr, !($($rhs:tt)+) $($tail:tt)*)) => {{
        let op = $crate::event::expr::BinaryOp::new($lhs, expr!(@value !($($rhs)+)));
        let acc = $crate::event::Expr::$op(op.into());
        expr!(@op (acc) $($tail)*)
    }};
    (@binop ($op:ident, $lhs:expr, $rhs:tt $($tail:tt)*)) => {{
        let op = $crate::event::expr::BinaryOp::new($lhs, expr!(@value $rhs));
        let acc = $crate::event::Expr::$op(op.into());
        expr!(@op (acc) $($tail)*)
    }};

    // Operators
    (@op ($result:expr)) => { $result };
    (@op ($lhs:expr) + $($rhs:tt)+) => { expr!(@binop (Add, $lhs, $($rhs)+)) };
    (@op ($lhs:expr) - $($rhs:tt)+) => { expr!(@binop (Subtract, $lhs, $($rhs)+)) };
    (@op ($lhs:expr) * $($rhs:tt)+) => { expr!(@binop (Multiply, $lhs, $($rhs)+)) };
    (@op ($lhs:expr) / $($rhs:tt)+) => { expr!(@binop (Divide, $lhs, $($rhs)+)) };
    (@op ($lhs:expr) % $($rhs:tt)+) => { expr!(@binop (Modulo, $lhs, $($rhs)+)) };
    (@op ($lhs:expr) == $($rhs:tt)+) => { expr!(@binop (Equal, $lhs, $($rhs)+)) };
    (@op ($lhs:expr) != $($rhs:tt)+) => { expr!(@binop (NotEqual, $lhs, $($rhs)+)) };
    (@op ($lhs:expr) <= $($rhs:tt)+) => { expr!(@binop (LessEqual, $lhs, $($rhs)+)) };
    (@op ($lhs:expr) < $($rhs:tt)+) => { expr!(@binop (Less, $lhs, $($rhs)+)) };
    (@op ($lhs:expr) >= $($rhs:tt)+) => { expr!(@binop (GreaterEqual, $lhs, $($rhs)+)) };
    (@op ($lhs:expr) > $($rhs:tt)+) => { expr!(@binop (Greater, $lhs, $($rhs)+)) };
    (@op ($lhs:expr) && $($rhs:tt)+) => { expr!(@binop (BitAnd, $lhs, $($rhs)+)) };
    (@op ($lhs:expr) & $($rhs:tt)+) => { expr!(@binop (BitAnd, $lhs, $($rhs)+)) };
    (@op ($lhs:expr) || $($rhs:tt)+) => { expr!(@binop (BitOr, $lhs, $($rhs)+)) };
    (@op ($lhs:expr) | $($rhs:tt)+) => { expr!(@binop (BitOr, $lhs, $($rhs)+)) };
    (@op ($lhs:expr) ^ $($rhs:tt)+) => { expr!(@binop (BitXor, $lhs, $($rhs)+)) };
    (@op ($lhs:expr) $($tail:tt)+) => { compile_error!("invalid operator") };

    // Entry point: array as first operand
    [$array:ident[$($index:tt)+] $($tail:tt)*] => {{
        let acc = expr!(@array $array[$($index)+]);
        expr!(@op (acc) $($tail)*)
    }};

    // Entry point: negation as first operand
    [!($($lhs:tt)+) $($tail:tt)*] => {{
        let acc = expr!(@value !($($lhs)+));
        expr!(@op (acc) $($tail)*)
    }};

    // Entry point: all other cases
    [$lhs:tt $($tail:tt)*] => {{
        let acc = expr!(@value $lhs);
        expr!(@op (acc) $($tail)*)
    }};
}

/// Generates an enum which binds to constant `Expr` values. The values can either be `Atom`s or
/// 32-bit integers.
///
/// Each enum value can have an optional argument list attached to it, and the argument list can be
/// specified inline (if entirely composed of `Expr`s) or use an existing type.
///
/// The generated enum will implement `SerializeEvent` and `DeserializeEvent` for easy I/O.
///
/// # Examples
///
/// Basic usage:
/// ```
/// # use unplug::event::expr;
/// # use unplug::event::opcodes::Atom;
/// # use unplug::expr_enum;
/// expr_enum! {
///     type Error = expr::Error;
///     pub enum ColorType {
///         Modulate => Atom::Modulate,
///         Blend => Atom::Blend,
///     }
/// }
/// ```
///
/// External argument list type:
/// ```
/// # use unplug::event::expr::{self, ObjExprObj, ObjExprBone, ObjExprPair};
/// # use unplug::event::opcodes::Atom;
/// # use unplug::expr_enum;
/// expr_enum! {
///     type Error = expr::Error;
///     pub enum ObjExpr {
///         Anim(ObjExprObj) => Atom::Anim,
///         BoneX(ObjExprBone) => Atom::BoneX,
///         Distance(ObjExprPair) => Atom::Distance,
///     }
/// }
/// ```
///
/// Inline argument lists:
/// ```
/// # use unplug::event::expr;
/// # use unplug::event::opcodes::Atom;
/// # use unplug::expr_enum;
/// expr_enum! {
///     type Error = expr::Error;
///     pub enum LightType {
///         Pos(LightPosArgs { x, y, z }) => Atom::Pos,
///         Color(LightColorArgs { r, g, b }) => Atom::Color,
///     }
/// }
/// ```
#[macro_export]
macro_rules! expr_enum {
    {
        type Error = $error:path;
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $(
                $type:ident $( ( $args_type:ident $( { $($arg:ident),* } )? ) )?
                    => $($val:literal)? $($op:path)?
            ),*
            $(,)*
        }
    } => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq)]
        $vis enum $name {
            $(
                $type $( ($args_type) )?,
            )*
        }

        impl $crate::event::serialize::DeserializeEvent for $name {
            type Error = $error;
            fn deserialize(
                de: &mut dyn $crate::event::serialize::EventDeserializer
            ) -> ::std::result::Result<Self, Self::Error> {
                let atom = de.deserialize_atom();
                match atom {
                    $($(Err($crate::event::serialize::Error::UnrecognizedAtom($val)))?
                    $(Ok($op))? => Ok(Self::$type $( ( $args_type::deserialize(de)? ) )?),)*
                    Ok(atom) => Err($crate::event::serialize::Error::UnsupportedAtom(atom).into()),
                    Err(e) => Err(e.into()),
                }
            }
        }

        impl $crate::event::serialize::SerializeEvent for $name {
            type Error = $error;
            fn serialize(
                &self,
                ser: &mut dyn $crate::event::serialize::EventSerializer
            ) -> ::std::result::Result<(), Self::Error> {
                match self {
                    $(
                        expr_enum!(@match $type $(, arg, $args_type)?) => {
                            $($crate::event::serialize::SerializeEvent::serialize(
                                &$crate::event::Expr::Imm32($val), ser)?;)?
                            $(ser.serialize_atom($op)?;)?
                            expr_enum!(@serialize $(ser, arg, $args_type)?);
                        }
                    )*
                }
                Ok(())
            }
        }

        $($($(
            #[derive(Debug, Clone, PartialEq, Eq)]
            $vis struct $args_type {
                $(pub $arg: $crate::event::Expr,)*
            }

            impl $crate::event::serialize::DeserializeEvent for $args_type {
                type Error = $crate::event::expr::Error;
                fn deserialize(
                    de: &mut dyn $crate::event::serialize::EventDeserializer
                ) -> $crate::event::expr::Result<Self> {
                    Ok(Self {
                        $($arg: $crate::event::serialize::DeserializeEvent::deserialize(de)?,)*
                    })
                }
            }

            impl $crate::event::serialize::SerializeEvent for $args_type {
                type Error = $crate::event::expr::Error;
                fn serialize(
                    &self,
                    ser: &mut dyn $crate::event::serialize::EventSerializer
                ) -> $crate::event::expr::Result<()> {
                    $($crate::event::serialize::SerializeEvent::serialize(&self.$arg, ser)?;)*
                    Ok(())
                }
            }
        )?)?)*
    };

    // Internal rules which let us match an arg object if $args_type is present
    (@match $type:ident, $args_var:ident, $args_type:ident) => {
        Self::$type($args_var)
    };
    (@match $type:ident) => {
        Self::$type
    };

    // Internal rules which let us write the arg object if $args_type is present
    (@serialize $ser:ident, $args_var:ident, $args_type:ident) => {
        $crate::event::serialize::SerializeEvent::serialize($args_var, $ser)?
    };
    (@serialize) => {};
}

#[cfg(test)]
mod tests {
    use crate::data::{Atc, Item};
    use crate::event::expr::{BinaryOp, Expr};

    #[test]
    fn test_expr_immediate() {
        assert_eq!(expr![1], Expr::Imm16(1));
    }

    #[test]
    fn test_expr_operators() {
        assert_eq!(expr![1 + 2], Expr::Add(BinaryOp::new(1.into(), 2.into()).into()));
        assert_eq!(expr![1 - 2], Expr::Subtract(BinaryOp::new(1.into(), 2.into()).into()));
        assert_eq!(expr![1 * 2], Expr::Multiply(BinaryOp::new(1.into(), 2.into()).into()));
        assert_eq!(expr![1 / 2], Expr::Divide(BinaryOp::new(1.into(), 2.into()).into()));
        assert_eq!(expr![1 % 2], Expr::Modulo(BinaryOp::new(1.into(), 2.into()).into()));
        assert_eq!(expr![1 == 2], Expr::Equal(BinaryOp::new(1.into(), 2.into()).into()));
        assert_eq!(expr![1 != 2], Expr::NotEqual(BinaryOp::new(1.into(), 2.into()).into()));
        assert_eq!(expr![1 < 2], Expr::Less(BinaryOp::new(1.into(), 2.into()).into()));
        assert_eq!(expr![1 <= 2], Expr::LessEqual(BinaryOp::new(1.into(), 2.into()).into()));
        assert_eq!(expr![1 > 2], Expr::Greater(BinaryOp::new(1.into(), 2.into()).into()));
        assert_eq!(expr![1 >= 2], Expr::GreaterEqual(BinaryOp::new(1.into(), 2.into()).into()));
        assert_eq!(expr![1 & 2], Expr::BitAnd(BinaryOp::new(1.into(), 2.into()).into()));
        assert_eq!(expr![1 && 2], Expr::BitAnd(BinaryOp::new(1.into(), 2.into()).into()));
        assert_eq!(expr![1 | 2], Expr::BitOr(BinaryOp::new(1.into(), 2.into()).into()));
        assert_eq!(expr![1 || 2], Expr::BitOr(BinaryOp::new(1.into(), 2.into()).into()));
        assert_eq!(expr![1 ^ 2], Expr::BitXor(BinaryOp::new(1.into(), 2.into()).into()));
    }

    #[test]
    fn test_operator_chaining() {
        let add = Expr::Add(BinaryOp::new(1.into(), 2.into()).into());
        let sub = Expr::Subtract(BinaryOp::new(add, 3.into()).into());
        let mul = Expr::Multiply(BinaryOp::new(sub, 4.into()).into());
        assert_eq!(expr![1 + 2 - 3 * 4], mul);
    }

    #[test]
    fn test_grouping() {
        let lhs = Expr::Add(BinaryOp::new(1.into(), 2.into()).into());
        let rhs = Expr::Subtract(BinaryOp::new(4.into(), 1.into()).into());
        assert_eq!(expr![(1 + 2) == (4 - 1)], Expr::Equal(BinaryOp::new(lhs, rhs).into()));
    }

    #[test]
    fn test_negate() {
        assert_eq!(expr![!(0)], Expr::Not(Expr::Imm16(0).into()));
        assert_eq!(expr![!(!(0))], Expr::Not(Expr::Not(Expr::Imm16(0).into()).into()));

        let lhs = Expr::Not(Expr::Add(BinaryOp::new(1.into(), 2.into()).into()).into());
        let rhs = Expr::Not(Expr::Add(BinaryOp::new(3.into(), 4.into()).into()).into());
        assert_eq!(expr![!(1 + 2) == !(3 + 4)], Expr::Equal(BinaryOp::new(lhs, rhs).into()));
    }

    #[test]
    fn test_arrays() {
        assert_eq!(expr![atc[1]], Expr::Atc(Expr::Imm16(1).into()));
        assert_eq!(expr![battery[1]], Expr::Battery(Expr::Imm16(1).into()));
        assert_eq!(expr![flag[1]], Expr::Flag(Expr::Imm16(1).into()));
        assert_eq!(expr![item[1]], Expr::Item(Expr::Imm16(1).into()));
        assert_eq!(expr![map[1]], Expr::Map(Expr::Imm16(1).into()));
        assert_eq!(expr![pad[1]], Expr::Pad(Expr::Imm16(1).into()));
        assert_eq!(expr![time[1]], Expr::Time(Expr::Imm16(1).into()));
        assert_eq!(expr![var[1]], Expr::Variable(Expr::Imm16(1).into()));

        assert_eq!(expr![atc[Atc::Toothbrush]], Expr::Atc(Box::new(Atc::Toothbrush.into())));
        assert_eq!(expr![item[Item::HotRod]], Expr::Item(Box::new(Item::HotRod.into())));

        assert_eq!(
            expr![1 + item[1]],
            Expr::Add(BinaryOp::new(1.into(), Expr::Item(Expr::Imm16(1).into())).into())
        );
    }
}
