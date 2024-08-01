use std::{alloc, fmt};
use std::borrow::{Borrow, BorrowMut};
use std::fmt::{Debug, Formatter};
use std::fmt::Pointer;
use std::mem::{forget, size_of};
use std::ops::{Deref, DerefMut, Index, IndexMut};
use std::ptr::{NonNull, slice_from_raw_parts_mut};
use std::slice::SliceIndex;

pub const CACHE_LINE_SIZE: usize = 64;

pub struct AlignedArray<T, const ALIGNMENT: usize> {
    ptr: NonNull<[T]>,
    len: usize,
}

impl<T, const ALIGNMENT: usize> Drop for AlignedArray<T, ALIGNMENT> {
    #[inline]
    fn drop(&mut self) {
        unsafe {
            let layout = alloc::Layout::from_size_align_unchecked(self.len * size_of::<T>(), ALIGNMENT);
            alloc::dealloc(self.ptr.as_ptr() as _, layout);
        }
    }
}

impl<T, const ALIGNMENT: usize> AlignedArray<T, ALIGNMENT> {
    /// Dictated by the requirements of
    /// [`alloc::Layout`](https://doc.rust-lang.org/alloc/alloc/struct.Layout.html).
    /// "`size`, when rounded up to the nearest multiple of `align`, must not overflow `isize`
    /// (i.e. the rounded value must be less than or equal to `isize::MAX`)".
    pub const MAX_CAPACITY: usize = isize::MAX as usize - (ALIGNMENT - 1);

    #[inline]
    pub fn new(len: usize) -> Self {
        if len == 0 {
            panic!();
        } else {
            assert!(
                len * size_of::<T>() <= Self::MAX_CAPACITY,
                "`len * size_of::<T>()` cannot exceed isize::MAX - (ALIGNMENT - 1)"
            );
            let ptr = unsafe {
                let layout = alloc::Layout::from_size_align_unchecked(len * size_of::<T>(), ALIGNMENT);
                let ptr = alloc::alloc(layout);
                if ptr.is_null() {
                    alloc::handle_alloc_error(layout);
                }
                NonNull::new_unchecked(slice_from_raw_parts_mut(ptr as *mut T, len))
            };
            Self {
                ptr,
                len,
            }
        }
    }

    #[inline]
    pub fn as_mut_ptr(&mut self) -> *mut [T] {
        self.ptr.as_ptr()
    }

    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        unsafe { self.ptr.as_mut() }
    }

    #[inline]
    pub fn as_ptr(&self) -> *const [T] {
        self.ptr.as_ptr()
    }

    #[inline]
    pub fn as_slice(&self) -> &[T] {
        unsafe { self.ptr.as_ref() }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn into_ptr(mut self) -> *mut [T] {
        let ptr = self.as_mut_ptr();
        forget(self);
        ptr
    }

    pub unsafe fn from_raw(ptr: *mut [T]) -> Self {
        Self {
            ptr: NonNull::new_unchecked(ptr),
            len: ptr.len(),
        }
    }
}

impl<T, const ALIGNMENT: usize> AsMut<[T]> for AlignedArray<T, ALIGNMENT> {
    #[inline]
    fn as_mut(&mut self) -> &mut [T] {
        self.as_mut_slice()
    }
}

impl<T, const ALIGNMENT: usize> AsRef<[T]> for AlignedArray<T, ALIGNMENT> {
    #[inline]
    fn as_ref(&self) -> &[T] {
        self.as_slice()
    }
}

impl<T, const ALIGNMENT: usize> Borrow<[T]> for AlignedArray<T, ALIGNMENT> {
    #[inline]
    fn borrow(&self) -> &[T] {
        self.as_slice()
    }
}

impl<T, const ALIGNMENT: usize> BorrowMut<[T]> for AlignedArray<T, ALIGNMENT> {
    #[inline]
    fn borrow_mut(&mut self) -> &mut [T] {
        self.as_mut_slice()
    }
}

impl<T, const ALIGNMENT: usize> Debug for AlignedArray<T, ALIGNMENT> {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.as_slice().fmt(f)
    }
}

impl<T, const ALIGNMENT: usize> Deref for AlignedArray<T, ALIGNMENT> {
    type Target = [T];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<T, const ALIGNMENT: usize> DerefMut for AlignedArray<T, ALIGNMENT> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

impl<T, const ALIGNMENT: usize, Idx: SliceIndex<[T]>> Index<Idx> for AlignedArray<T, ALIGNMENT> {
    type Output = <Idx as SliceIndex<[T]>>::Output;

    #[inline]
    fn index(&self, index: Idx) -> &Self::Output {
        &self.as_slice()[index]
    }
}

impl<T, const ALIGNMENT: usize, Idx: SliceIndex<[T]>> IndexMut<Idx> for AlignedArray<T, ALIGNMENT> {
    #[inline]
    fn index_mut(&mut self, index: Idx) -> &mut Self::Output {
        &mut self.as_mut_slice()[index]
    }
}