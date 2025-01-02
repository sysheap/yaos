use core::ops::{Deref, DerefMut};

use common::constructable::Constructable;

#[derive(Debug)]
pub enum ValidationError {}

pub struct UserspaceArgument<T> {
    inner: T,
}

impl<T> Constructable<T> for UserspaceArgument<T> {
    fn new(inner: T) -> Self {
        UserspaceArgument { inner }
    }
}

pub trait Validatable<T: Sized> {
    fn validate(&mut self) -> Result<T, ValidationError>;
}

impl<'a> Validatable<&'a str> for UserspaceArgument<&'a str> {
    fn validate(&mut self) -> Result<&'a str, ValidationError> {
        todo!()
    }
}

impl<'a> Validatable<&'a [u8]> for UserspaceArgument<&'a [u8]> {
    fn validate(&mut self) -> Result<&'a [u8], ValidationError> {
        todo!()
    }
}

impl<'a> Validatable<&'a mut [u8]> for UserspaceArgument<&'a mut [u8]> {
    fn validate(&mut self) -> Result<&'a mut [u8], ValidationError> {
        todo!()
    }
}

macro_rules! simple_type {
    ($ty:ty) => {
        impl Deref for UserspaceArgument<$ty> {
            type Target = $ty;

            fn deref(&self) -> &Self::Target {
                &self.inner
            }
        }

        impl DerefMut for UserspaceArgument<$ty> {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.inner
            }
        }
    };
}

simple_type!(char);

simple_type!(u8);
simple_type!(u16);
simple_type!(u32);
simple_type!(u64);
simple_type!(usize);

simple_type!(i8);
simple_type!(i16);
simple_type!(i32);
simple_type!(i64);
simple_type!(isize);
