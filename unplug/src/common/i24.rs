use byte_slice_cast::{Error as BscError, FromByteSlice, ToByteSlice, ToMutByteSlice};
use std::convert::TryFrom;
use std::fmt::{self, Debug, Display};
use std::ops::*;
use std::slice;
use thiserror::Error;

/// A 24-bit integer type which is represented as 3 bytes in the native byte order.
/// This type is incomplete and only provided for the purposes of working with 24-bit PCM samples.
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct I24([u8; 3]);

impl I24 {
    pub const MIN: I24 = Self::new(-0x800000);
    pub const MAX: I24 = Self::new(0x7fffff);
    pub const BITS: u32 = 24;
    const NAME: &'static str = "I24";

    /// Creates a new `I24` by truncating `val` to 24 bits.
    pub const fn new(val: i32) -> Self {
        let bytes = val.to_ne_bytes();
        if cfg!(target_endian = "little") {
            Self([bytes[0], bytes[1], bytes[2]])
        } else {
            Self([bytes[1], bytes[2], bytes[3]])
        }
    }

    /// Retrieves the value of this integer as an `i32`.
    pub const fn get(self) -> i32 {
        if cfg!(target_endian = "little") {
            let ext = ((self.0[2] as i8) >> 7) as u8;
            i32::from_ne_bytes([self.0[0], self.0[1], self.0[2], ext])
        } else {
            let ext = ((self.0[0] as i8) >> 7) as u8;
            i32::from_ne_bytes([ext, self.0[0], self.0[1], self.0[2]])
        }
    }

    /// Reverses the byte order of the integer.
    #[must_use]
    pub const fn swap_bytes(self) -> Self {
        Self([self.0[2], self.0[1], self.0[0]])
    }

    /// Return the memory representation of this integer as a byte array in native byte order.
    pub const fn to_ne_bytes(self) -> [u8; 3] {
        self.0
    }
}

impl Debug for I24 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Debug::fmt(&self.get(), f)
    }
}

impl Display for I24 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.get(), f)
    }
}

macro_rules! operator {
    ($type:ty, $trait:ident, $method:ident, $trait_mut:ident, $method_mut:ident, $op:tt) => {
        impl $trait for $type {
            type Output = Self;
            fn $method(self, rhs: Self) -> Self::Output {
                Self::new(self.get() $op rhs.get())
            }
        }
        impl $trait_mut for $type {
            fn $method_mut(&mut self, rhs: Self) {
                *self = Self::new(self.get() $op rhs.get())
            }
        }
    };
}

operator!(I24, Add, add, AddAssign, add_assign, +);
operator!(I24, Sub, sub, SubAssign, sub_assign, -);
operator!(I24, Mul, mul, MulAssign, mul_assign, *);
operator!(I24, Div, div, DivAssign, div_assign, /);
operator!(I24, Rem, rem, RemAssign, rem_assign, %);
operator!(I24, BitAnd, bitand, BitAndAssign, bitand_assign, &);
operator!(I24, BitOr, bitor, BitOrAssign, bitor_assign, |);
operator!(I24, BitXor, bitxor, BitXorAssign, bitxor_assign, ^);
operator!(I24, Shl, shl, ShlAssign, shl_assign, <<);
operator!(I24, Shr, shr, ShrAssign, shr_assign, >>);

impl Neg for I24 {
    type Output = Self;
    fn neg(self) -> Self::Output {
        Self::new(-self.get())
    }
}

impl Not for I24 {
    type Output = Self;
    fn not(self) -> Self::Output {
        Self::new(!self.get())
    }
}

macro_rules! from_primitive {
    ($type:ty, $from:ty) => {
        impl From<$from> for $type {
            fn from(val: $from) -> Self {
                Self::new(i32::from(val))
            }
        }
    };
}

from_primitive!(I24, i8);
from_primitive!(I24, u8);
from_primitive!(I24, i16);
from_primitive!(I24, u16);

macro_rules! into_primitive {
    ($type:ty, $into:ty) => {
        impl From<$type> for $into {
            fn from(val: $type) -> Self {
                Self::from(val.get())
            }
        }
    };
}

into_primitive!(I24, i32);
into_primitive!(I24, i64);
into_primitive!(I24, i128);

#[derive(Error, Debug)]
#[error("out of range integral type conversion attempted")]
#[non_exhaustive]
pub struct TryFromError;

macro_rules! try_from_signed {
    ($type:ty, $from:ty) => {
        impl TryFrom<$from> for $type {
            type Error = TryFromError;
            #[allow(trivial_numeric_casts)]
            fn try_from(value: $from) -> Result<Self, Self::Error> {
                if value >= (Self::MIN.get() as $from) && value <= (Self::MAX.get() as $from) {
                    Ok(Self::new(value as i32))
                } else {
                    Err(TryFromError)
                }
            }
        }
    };
}
macro_rules! try_from_unsigned {
    ($type:ty, $from:ty) => {
        impl TryFrom<$from> for $type {
            type Error = TryFromError;
            #[allow(trivial_numeric_casts)]
            fn try_from(value: $from) -> Result<Self, Self::Error> {
                if value <= (Self::MAX.get() as $from) {
                    Ok(Self::new(value as i32))
                } else {
                    Err(TryFromError)
                }
            }
        }
    };
}

try_from_signed!(I24, i32);
try_from_signed!(I24, i64);
try_from_signed!(I24, i128);
try_from_signed!(I24, isize);
try_from_unsigned!(I24, u32);
try_from_unsigned!(I24, u64);
try_from_unsigned!(I24, u128);
try_from_unsigned!(I24, usize);

// SAFETY: I24 can be converted to and from a byte slice
unsafe impl ToByteSlice for I24 {
    fn to_byte_slice<T: AsRef<[Self]> + ?Sized>(slice: &T) -> &[u8] {
        let slice = slice.as_ref();
        // SAFETY: I24 is represented as an array of 3 bytes
        unsafe { slice::from_raw_parts(slice.as_ptr() as *const u8, slice.len() * 3) }
    }
}

// SAFETY: I24 can be converted to and from a byte slice
unsafe impl ToMutByteSlice for I24 {
    fn to_mut_byte_slice<T: AsMut<[Self]> + ?Sized>(slice: &mut T) -> &mut [u8] {
        let slice = slice.as_mut();
        // SAFETY: I24 is represented as an array of 3 bytes
        unsafe { slice::from_raw_parts_mut(slice.as_mut_ptr() as *mut u8, slice.len() * 3) }
    }
}

// SAFETY: I24 can be converted to and from a byte slice
unsafe impl FromByteSlice for I24 {
    fn from_byte_slice<T: AsRef<[u8]> + ?Sized>(slice: &T) -> Result<&[Self], BscError> {
        let bytes = slice.as_ref();
        if bytes.len() % 3 == 0 {
            // SAFETY: I24 is represented as an array of 3 bytes and does not need alignment
            Ok(unsafe { slice::from_raw_parts(bytes.as_ptr() as *const I24, bytes.len() / 3) })
        } else {
            Err(BscError::LengthMismatch {
                dst_type: Self::NAME,
                src_slice_size: bytes.len(),
                dst_type_size: 3,
            })
        }
    }

    fn from_mut_byte_slice<T: AsMut<[u8]> + ?Sized>(
        slice: &mut T,
    ) -> Result<&mut [Self], BscError> {
        let bytes = slice.as_mut();
        if bytes.len() % 3 == 0 {
            // SAFETY: I24 is represented as an array of 3 bytes and does not need alignment
            Ok(unsafe {
                slice::from_raw_parts_mut(bytes.as_mut_ptr() as *mut I24, bytes.len() / 3)
            })
        } else {
            Err(BscError::LengthMismatch {
                dst_type: Self::NAME,
                src_slice_size: bytes.len(),
                dst_type_size: 3,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byte_slice_cast::{AsByteSlice, AsSliceOf};

    #[test]
    fn test_i24_conversion() {
        assert_eq!(i32::from(I24::from(123i16)), 123);
        assert_eq!(i32::from(I24::from(-123i16)), -123);

        assert_eq!(I24::try_from(I24::MAX.get() as i64).unwrap(), I24::MAX);
        assert_eq!(I24::try_from(I24::MIN.get() as i64).unwrap(), I24::MIN);
        assert!(I24::try_from(I24::MAX.get() as i64 + 1).is_err());
        assert!(I24::try_from(I24::MIN.get() as i64 - 1).is_err());

        assert_eq!(I24::try_from(I24::MAX.get() as u64).unwrap(), I24::MAX);
        assert!(I24::try_from(I24::MAX.get() as u64 + 1).is_err());
    }

    #[test]
    fn test_i24_ops() {
        assert_eq!(I24::new(1) + I24::new(2), I24::new(3));
        assert_eq!(I24::new(1) - I24::new(2), I24::new(-1));
        assert_eq!(I24::new(2) * I24::new(3), I24::new(6));
        assert_eq!(I24::new(15) / I24::new(3), I24::new(5));
        assert_eq!(I24::new(17) % I24::new(3), I24::new(2));
        assert_eq!(I24::new(0x1234) & I24::new(0xff00), I24::new(0x1200));
        assert_eq!(I24::new(0x1234) | I24::new(0xff00), I24::new(0xff34));
        assert_eq!(I24::new(0x1234) ^ I24::new(0xff00), I24::new(0xed34));
        assert_eq!(I24::new(3) << I24::new(1), I24::new(6));
        assert_eq!(I24::new(3) >> I24::new(1), I24::new(1));
        assert_eq!(-I24::new(1), I24::new(-1));
        assert_eq!(!I24::new(0x1234), I24::new(0xffedcb));

        // Overflow
        assert_eq!(I24::MAX + I24::new(1), I24::MIN);
        assert_eq!(I24::MIN - I24::new(1), I24::MAX);
    }

    #[test]
    fn test_i24_swap_bytes() {
        assert_eq!(I24::new(0x123456).swap_bytes(), I24::new(0x563412));
    }

    #[test]
    fn test_i24_as_byte_slice() {
        let values = &[I24::new(0x123456), I24::new(0xabcdef)];
        let bytes = values.as_byte_slice();
        if cfg!(target_endian = "little") {
            assert_eq!(bytes, &[0x56, 0x34, 0x12, 0xef, 0xcd, 0xab]);
        } else {
            assert_eq!(bytes, &[0x12, 0x34, 0x56, 0xab, 0xcd, 0xef]);
        }
    }

    #[test]
    fn test_i24_from_byte_slice() {
        let bytes: &[u8] = if cfg!(target_endian = "little") {
            &[0x56, 0x34, 0x12, 0xef, 0xcd, 0xab]
        } else {
            &[0x12, 0x34, 0x56, 0xab, 0xcd, 0xef]
        };
        let values = bytes.as_slice_of::<I24>().unwrap();
        assert_eq!(values, &[I24::new(0x123456), I24::new(0xabcdef)]);
    }

    #[test]
    fn test_i24_from_byte_slice_err() {
        let bytes = [0u8; 7];
        assert!(matches!(bytes.as_slice_of::<I24>(), Err(BscError::LengthMismatch { .. })));
        let bytes = [0u8; 8];
        assert!(matches!(bytes.as_slice_of::<I24>(), Err(BscError::LengthMismatch { .. })));
    }
}
