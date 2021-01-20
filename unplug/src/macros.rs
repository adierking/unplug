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

/// Generates an enum which binds to constant `Expr` values.
///
/// Each enum value can have an optional argument list attached to it, and the argument list can be
/// specified inline (if entirely composed of `Expr`s) or use an existing type.
///
/// The generated enum will implement `ReadFrom` and `WriteTo` for easy I/O.
///
/// # Examples
///
/// Basic usage:
/// ```
/// # use unplug::event::{expr, opcodes::{TYPE_MODULATE, TYPE_BLEND}};
/// # use unplug::expr_enum;
/// expr_enum! {
///     type Error = expr::Error;
///     pub enum ColorType {
///         Modulate => TYPE_MODULATE,
///         Blend => TYPE_BLEND,
///     }
/// }
/// ```
///
/// External argument list type:
/// ```
/// # use unplug::event::opcodes::{TYPE_ANIM, TYPE_BONE_X, TYPE_DISTANCE};
/// # use unplug::event::expr::{self, ObjExprObj, ObjExprBone, ObjExprPair};
/// # use unplug::expr_enum;
/// expr_enum! {
///     type Error = expr::Error;
///     pub enum ObjExpr {
///         Anim(ObjExprObj) => TYPE_ANIM,
///         BoneX(ObjExprBone) => TYPE_BONE_X,
///         Distance(ObjExprPair) => TYPE_DISTANCE,
///     }
/// }
/// ```
///
/// Inline argument lists:
/// ```
/// # use unplug::event::{expr, opcodes::{TYPE_POS, TYPE_COLOR}};
/// # use unplug::expr_enum;
/// expr_enum! {
///     type Error = expr::Error;
///     pub enum LightType {
///         Pos(LightPosArgs { x, y, z }) => TYPE_POS,
///         Color(LightColorArgs { r, g, b }) => TYPE_COLOR,
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
                $type:ident $( ( $args_type:ident $( { $($arg:ident),* } )? ) )? => $val:expr
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

        impl<R: ::std::io::Read> $crate::common::ReadFrom<R> for $name {
            type Error = $error;
            fn read_from(reader: &mut R) -> ::std::result::Result<Self, Self::Error> {
                let ty_expr = $crate::event::Expr::read_from(reader)?;
                let ty = match ty_expr.value() {
                    Some(x) => x,
                    None => return Err($crate::event::expr::Error::NonConstant(ty_expr.into()).into()),
                };
                Ok(match ty {
                    $(x if x == $val => Self::$type $( ( $args_type::read_from(reader)? ) )?,)*
                    _ => return Err($crate::event::expr::Error::UnrecognizedType(ty).into()),
                })
            }
        }

        impl<W: ::std::io::Write + $crate::event::block::WriteIp> $crate::common::WriteTo<W> for $name {
            type Error = $error;
            fn write_to(&self, writer: &mut W) -> ::std::result::Result<(), Self::Error> {
                match self {
                    $(
                        expr_enum!(@match $type $(, arg, $args_type)?) => {
                            let ty_expr = $crate::event::Expr::Imm32($val);
                            $crate::common::WriteTo::write_to(&ty_expr, writer)?;
                            expr_enum!(@write $(writer, arg, $args_type)?);
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

            impl<R: ::std::io::Read> $crate::common::ReadFrom<R> for $args_type {
                type Error = $crate::event::expr::Error;
                fn read_from(reader: &mut R) -> $crate::event::expr::Result<Self> {
                    Ok(Self {
                        $($arg: $crate::event::Expr::read_from(reader)?,)*
                    })
                }
            }

            impl<W: ::std::io::Write + $crate::event::block::WriteIp> $crate::common::WriteTo<W> for $args_type {
                type Error = $crate::event::expr::Error;
                fn write_to(&self, writer: &mut W) -> $crate::event::expr::Result<()> {
                    $($crate::common::WriteTo::write_to(&self.$arg, writer)?;)*
                    Ok(())
                }
            }
        )*)?)?
    };

    // Internal rules which let us match an arg object if $args_type is present
    (@match $type:ident, $args_var:ident, $args_type:ident) => {
        Self::$type($args_var)
    };
    (@match $type:ident) => {
        Self::$type
    };

    // Internal rules which let us write the arg object if $args_type is present
    (@write $writer:ident, $args_var:ident, $args_type:ident) => {
        $crate::common::WriteTo::write_to($args_var, $writer)?;
    };
    (@write) => {};
}
