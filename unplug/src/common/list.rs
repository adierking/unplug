use super::{ReadFrom, ReadOptionFrom, WriteOptionTo, WriteTo};
use std::borrow::Cow;
use std::io::{Read, Write};
use std::iter::IntoIterator;

/// A readable and writable list of objects which is terminated by a `None` value.
pub struct NonNoneList<'a, T: Clone>(pub Cow<'a, [T]>);

impl<T: Clone> NonNoneList<'_, T> {
    /// Converts this `NonNoneList<T>` into a `Vec<T>`.
    pub fn into_vec(self) -> Vec<T> {
        self.0.into_owned()
    }
}

impl<R: Read + ?Sized, T: Clone + ReadOptionFrom<R>> ReadFrom<R> for NonNoneList<'_, T> {
    type Error = T::Error;
    fn read_from(reader: &mut R) -> Result<Self, Self::Error> {
        let mut list = vec![];
        while let Some(item) = T::read_option_from(reader)? {
            list.push(item);
        }
        list.shrink_to_fit();
        Ok(Self(list.into()))
    }
}

impl<W: Write + ?Sized, T: Clone + WriteOptionTo<W>> WriteTo<W> for NonNoneList<'_, T> {
    type Error = T::Error;
    fn write_to(&self, writer: &mut W) -> Result<(), Self::Error> {
        for item in &*self.0 {
            Some(item).write_to(writer)?;
        }
        T::write_option_to(None, writer)?;
        Ok(())
    }
}

impl<T: Clone> IntoIterator for NonNoneList<'_, T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_owned().into_iter()
    }
}
