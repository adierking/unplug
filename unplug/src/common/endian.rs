#![allow(unused_qualifications)]
use super::I24;
use byte_slice_cast::{self as bsc, AsByteSlice, AsMutSliceOf, FromByteSlice, ToByteSlice};
use byteorder::{ByteOrder, NativeEndian, BE, LE};
use std::convert::AsMut;
use std::io::{self, Read, Write};
use std::mem;

/// `ByteOrder` extension for checking whether an endianness matches the target system.
pub trait IsNative {
    /// Returns whether this is the target system's native endianness.
    fn is_native() -> bool;
}

impl IsNative for LE {
    fn is_native() -> bool {
        cfg!(target_endian = "little")
    }
}

impl IsNative for BE {
    fn is_native() -> bool {
        cfg!(target_endian = "big")
    }
}

/// Trait for a mutable slice that can be converted between endians.
pub trait ConvertEndian<T>: AsMut<[T]> {
    /// Converts the values from the native endianness. Use `convert_endian()` instead.
    fn convert_from_native<E: ByteOrder + IsNative>(&mut self);

    /// Converts the values from the `From` endianness to the `To` endianness.
    fn convert_endian<From, To>(&mut self)
    where
        From: ByteOrder + IsNative,
        To: ByteOrder + IsNative,
    {
        if From::is_native() {
            self.convert_from_native::<To>();
        } else if To::is_native() {
            self.convert_from_native::<From>();
        }
    }
}

macro_rules! impl_convert {
    ($ty:ty, $fn:ident) => {
        impl<T: AsMut<[$ty]> + ?Sized> ConvertEndian<$ty> for T {
            fn convert_from_native<E: ByteOrder + IsNative>(&mut self) {
                E::$fn(self.as_mut())
            }
        }
    };
}
impl_convert!(i16, from_slice_i16);
impl_convert!(u16, from_slice_u16);
impl_convert!(i32, from_slice_i32);
impl_convert!(u32, from_slice_u32);
impl_convert!(i64, from_slice_i64);
impl_convert!(u64, from_slice_u64);
impl_convert!(i128, from_slice_i128);
impl_convert!(u128, from_slice_u128);
impl_convert!(f32, from_slice_f32);
impl_convert!(f64, from_slice_f64);

impl<T: AsMut<[u8]>> ConvertEndian<u8> for T {
    fn convert_from_native<E: ByteOrder + IsNative>(&mut self) {}
}
impl<T: AsMut<[i8]>> ConvertEndian<i8> for T {
    fn convert_from_native<E: ByteOrder + IsNative>(&mut self) {}
}

impl<T: AsMut<[I24]>> ConvertEndian<I24> for T {
    fn convert_from_native<E: ByteOrder + IsNative>(&mut self) {
        if !E::is_native() {
            for val in self.as_mut() {
                *val = val.swap_bytes();
            }
        }
    }
}

/// `Read` extension for reading values with an endianness selected at runtime.
pub trait ReadValuesExt: Read {
    /// Reads to the end of the stream using the endianness specified by `E` and then returns a
    /// `Vec<T>` of the values.
    fn read_values_to_end<E, T>(&mut self) -> io::Result<Vec<T>>
    where
        T: Clone + Copy + FromByteSlice,
        E: ByteOrder + IsNative,
        for<'a> &'a mut [T]: ConvertEndian<T>,
    {
        let mut bytes = vec![];
        self.read_to_end(&mut bytes)?;
        let mut bytes = bytes.into_boxed_slice();
        let mut values = match bytes.as_mut_slice_of::<T>() {
            Ok(v) => v,
            Err(bsc::Error::LengthMismatch { .. }) => {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
            }
            Err(e) => panic!("slice cast failed: {:?}", e),
        };
        values.convert_endian::<E, NativeEndian>();
        // SAFETY: We know the pointer was allocated by a Vec, and as_mut_slice_of() already
        // checked that the pointer is safe
        let v = unsafe { Vec::from_raw_parts(values.as_mut_ptr(), values.len(), values.len()) };
        mem::forget(bytes); // Allocation is managed by the Vec now
        Ok(v)
    }
}
impl<R: Read> ReadValuesExt for R {}

/// `Write` extension for writing values with an endianness selected at runtime.
pub trait WriteValuesExt: Write {
    /// Writes all of the values in `values` using the endianness specified by `E`.
    fn write_all_values<E, T>(&mut self, values: &[T]) -> io::Result<()>
    where
        T: Clone + Copy + ToByteSlice,
        E: ByteOrder + IsNative,
        for<'a> &'a mut [T]: ConvertEndian<T>,
    {
        const BUFFER_SIZE: usize = 0x100000; // 1 MB
        if E::is_native() {
            self.write_all(values.as_byte_slice())?;
        } else {
            let chunk_size = values.len().min(BUFFER_SIZE / mem::size_of::<T>());
            let mut buffer = Vec::with_capacity(chunk_size);
            for chunk in values.chunks(chunk_size) {
                if buffer.is_empty() {
                    buffer.extend(chunk);
                } else {
                    buffer[..chunk.len()].copy_from_slice(chunk);
                }
                let mut converted = &mut buffer[..chunk.len()];
                converted.convert_endian::<NativeEndian, E>();
                self.write_all(converted.as_byte_slice())?;
            }
        }
        Ok(())
    }
}
impl<W: Write> WriteValuesExt for W {}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_read_values<E, T, I, F>(values: I, to_bytes: F)
    where
        E: ByteOrder + IsNative,
        T: Clone + Copy + PartialEq + FromByteSlice,
        for<'a> &'a mut [T]: ConvertEndian<T>,
        I: IntoIterator<Item = T>,
        F: FnOnce(&[T], &mut [u8]),
    {
        let expected = values.into_iter().collect::<Vec<_>>();
        let mut bytes = vec![0u8; expected.len() * mem::size_of::<T>()];
        to_bytes(&expected, &mut bytes);

        let actual = bytes.as_slice().read_values_to_end::<E, T>().unwrap();
        assert!(expected == actual);
    }

    fn test_write_values<E, T, I, F>(values: I, to_bytes: F)
    where
        E: ByteOrder + IsNative,
        T: Clone + Copy + ToByteSlice,
        for<'a> &'a mut [T]: ConvertEndian<T>,
        I: IntoIterator<Item = T>,
        F: FnOnce(&[T], &mut [u8]),
    {
        let mut bytes = vec![];
        let values = values.into_iter().collect::<Vec<_>>();
        bytes.write_all_values::<E, T>(&values).unwrap();

        let mut expected = vec![0u8; values.len() * mem::size_of::<T>()];
        to_bytes(&values, &mut expected);
        assert!(bytes == expected);
    }

    fn write_i24_into<E: ByteOrder + IsNative>(values: &[I24], bytes: &mut [u8]) {
        assert_eq!(bytes.len(), values.len() * 3);
        for (v, c) in values.iter().copied().zip(bytes.chunks_exact_mut(3)) {
            let b = &v.to_ne_bytes();
            if E::is_native() {
                c.copy_from_slice(b);
            } else {
                c.copy_from_slice(&[b[2], b[1], b[0]]);
            }
        }
    }

    macro_rules! do_test {
        ($fn:ident, $et:ty, $ee:expr) => {{
            type E = $et;
            $fn::<E, _, _, _>(u16::MIN..u16::MAX, E::write_u16_into);
            $fn::<E, _, _, _>(i16::MIN..i16::MAX, E::write_i16_into);
            $fn::<E, _, _, _>(0..1000000u32, E::write_u32_into);
            $fn::<E, _, _, _>(0..1000000i32, E::write_i32_into);
            $fn::<E, _, _, _>(0..1000000u64, E::write_u64_into);
            $fn::<E, _, _, _>(0..1000000i64, E::write_i64_into);
            $fn::<E, _, _, _>(0..1000000u128, E::write_u128_into);
            $fn::<E, _, _, _>(0..1000000i128, E::write_i128_into);
            $fn::<E, _, _, _>((0..1000000i32).map(|i| i as f32), E::write_f32_into);
            $fn::<E, _, _, _>((0..1000000i32).map(|i| i as f64), E::write_f64_into);
            $fn::<E, _, _, _>((0..1000000).map(I24::new), write_i24_into::<E>);
        }};
    }

    #[test]
    fn test_read_values_le() {
        do_test!(test_read_values, LE, Endian::Little);
    }

    #[test]
    fn test_read_values_be() {
        do_test!(test_read_values, BE, Endian::Big);
    }

    #[test]
    fn test_write_values_le() {
        do_test!(test_write_values, LE, Endian::Little);
    }

    #[test]
    fn test_write_values_be() {
        do_test!(test_write_values, BE, Endian::Big);
    }
}
