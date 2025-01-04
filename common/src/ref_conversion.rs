use crate::pointer::{AsFatPointer, AsFatPointerMut, FatPointer};

auto trait IsValue {}

impl<T> !IsValue for &T {}
impl<T> !IsValue for &mut T {}

/// This trait keeps T as T if T is not a reference
/// &T is converted to *const T
/// &mut T is converted to *mut T
pub trait RefToPointer<T> {
    type Out;
    fn to_pointer_if_ref(self) -> Self::Out;
}

impl<T: IsValue> RefToPointer<T> for T {
    type Out = T;

    fn to_pointer_if_ref(self) -> Self::Out {
        self
    }
}

impl<T> RefToPointer<T> for &T {
    type Out = *const T;

    fn to_pointer_if_ref(self) -> Self::Out {
        self
    }
}

impl<T> RefToPointer<T> for &mut T {
    type Out = *mut T;

    fn to_pointer_if_ref(self) -> Self::Out {
        self
    }
}

impl RefToPointer<&str> for &str {
    type Out = FatPointer<*const u8>;

    fn to_pointer_if_ref(self) -> Self::Out {
        self.as_fat_pointer()
    }
}

impl RefToPointer<&[u8]> for &[u8] {
    type Out = FatPointer<*const u8>;

    fn to_pointer_if_ref(self) -> Self::Out {
        self.as_fat_pointer()
    }
}

impl RefToPointer<&mut [u8]> for &mut [u8] {
    type Out = FatPointer<*mut u8>;

    fn to_pointer_if_ref(mut self) -> Self::Out {
        self.as_fat_pointer_mut()
    }
}
