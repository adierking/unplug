use std::convert::TryInto;
use std::ops::{Add, BitAnd, Not, Sub};

/// Rounds `value` up to a multiple of `align`, where `align` is a power of two.
pub fn align<T, U>(value: T, align: U) -> U
where
    T: Into<U>,
    U: From<u8> + Add<Output = U> + Sub<Output = U> + BitAnd<Output = U> + Not<Output = U> + Copy,
{
    (value.into() + align - 1.into()) & !(align - 1.into())
}

/// Casts `value` to `U`, clamping it to `[min, max]` if it is out-of-range.
pub fn clamp_cast<T, U>(value: T, min: U, max: U) -> U
where
    T: Ord + TryInto<U> + Copy,
    U: Ord + Into<T> + Copy,
{
    match value.try_into() {
        Ok(x) => x.max(min).min(max),
        Err(_) => {
            if value > max.into() {
                max
            } else {
                min
            }
        }
    }
}

/// Casts `value` to `i16`, clamping it to `[i16::MIN, i16::MAX]` if it is out-of-range.
pub fn clamp_i16<T>(value: T) -> i16
where
    T: Ord + TryInto<i16> + From<i16> + Copy,
{
    clamp_cast(value, i16::MIN, i16::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_align() {
        assert_eq!(align(0, 16), 0);
        assert_eq!(align(1, 16), 16);
        assert_eq!(align(16, 16), 16);
        assert_eq!(align(17, 16), 32);
    }

    #[test]
    fn test_clamp_i16() {
        assert_eq!(clamp_i16(123), 123);
        assert_eq!(clamp_i16(-123), -123);
        assert_eq!(clamp_i16(i16::MAX), i16::MAX);
        assert_eq!(clamp_i16(i16::MAX as i32 + 1), i16::MAX);
        assert_eq!(clamp_i16(i16::MIN), i16::MIN);
        assert_eq!(clamp_i16(i16::MIN as i32 - 1), i16::MIN);
    }
}
