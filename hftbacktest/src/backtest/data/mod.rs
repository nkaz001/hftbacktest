mod npy;
mod reader;

use std::{
    marker::PhantomData,
    mem::size_of,
    ops::{Index, IndexMut},
    ptr::null_mut,
    rc::Rc,
    slice::SliceIndex,
};

pub use npy::{read_npy_file, read_npz_file, write_npy, Field, NpyDTyped, NpyHeader};
pub use reader::{Cache, DataSource, Reader};

use crate::utils::{AlignedArray, CACHE_LINE_SIZE};

/// Marker trait for plain old data.
///
/// # Safety
/// This marker trait should be implemented only if the struct has a C representation and contains
/// only plain old data.
pub unsafe trait POD: Sized {}

/// Provides access to an array of structs from the buffer.
#[derive(Clone, Debug)]
pub struct Data<D>
where
    D: POD + Clone,
{
    ptr: Rc<DataPtr>,
    offset: usize,
    _d_marker: PhantomData<D>,
}

impl<D> Data<D>
where
    D: POD + Clone,
{
    /// Returns the length of the array.
    #[inline(always)]
    pub fn len(&self) -> usize {
        let size = size_of::<D>();
        (self.ptr.len() - self.offset) / size
    }

    /// Returns ``true`` if the ``Data`` is empty.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.ptr.len() == 0
    }

    /// Constructs an empty `Data`.
    pub fn empty() -> Self {
        Self {
            ptr: Default::default(),
            offset: 0,
            _d_marker: PhantomData,
        }
    }

    pub fn from_data_ptr(ptr: DataPtr, offset: usize) -> Self {
        Self {
            ptr: Rc::new(ptr),
            offset,
            _d_marker: PhantomData,
        }
    }

    /// Constructs ``Data`` from a fat pointer with the specified offset.
    ///
    /// ``Data`` uses a [`DataPtr`], which is constructed from the given fat pointer. Please refer
    /// to the safety guidelines in [`DataPtr::from_ptr`].
    ///
    /// # Safety
    /// The underlying memory layout must match the layout of type ``D``.
    pub unsafe fn from_ptr(ptr: *mut [u8], offset: usize) -> Self {
        Self::from_data_ptr(DataPtr::from_ptr(ptr), offset)
    }

    /// Returns a reference to an element, without doing bounds checking.
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, index: usize) -> &D {
        let size = size_of::<D>();
        let i = self.offset + index * size;
        unsafe { &*(self.ptr.at(i) as *const D) }
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.ptr, &other.ptr)
    }
}

impl<D> Index<usize> for Data<D>
where
    D: POD + Clone,
{
    type Output = D;

    #[inline(always)]
    fn index(&self, index: usize) -> &Self::Output {
        let size = size_of::<D>();
        let i = self.offset + index * size;
        if i + size > self.ptr.len() {
            panic!("Out of the size.");
        }
        unsafe { &*(self.ptr.at(i) as *const D) }
    }
}

#[derive(Debug)]
pub struct DataPtr {
    ptr: *mut [u8],
    managed: bool,
}

impl DataPtr {
    pub fn new(size: usize) -> Self {
        let arr = AlignedArray::<u8, CACHE_LINE_SIZE>::new(size);
        Self {
            ptr: arr.into_raw(),
            managed: true,
        }
    }

    /// Constructs ``DataPtr`` from a fat pointer.
    ///
    /// Unlike other methods that construct an instance from a raw pointer, the raw pointer is not
    /// owned by the resulting ``DataPtr``. Memory should still be managed by the caller.
    ///
    /// # Safety
    /// The fat pointer must remain valid for the lifetime of the resulting ``DataPtr``.
    pub unsafe fn from_ptr(ptr: *mut [u8]) -> Self {
        Self {
            ptr,
            managed: false,
        }
    }

    #[allow(clippy::len_without_is_empty)]
    #[inline]
    pub fn len(&self) -> usize {
        self.ptr.len()
    }

    #[inline]
    pub fn at(&self, index: usize) -> *const u8 {
        let ptr = self.ptr as *const u8;
        unsafe { ptr.add(index) }
    }
}

impl Default for DataPtr {
    fn default() -> Self {
        Self {
            ptr: null_mut::<[u8; 0]>() as *mut [u8],
            managed: false,
        }
    }
}

impl<Idx> Index<Idx> for DataPtr
where
    Idx: SliceIndex<[u8]>,
{
    type Output = Idx::Output;

    #[inline]
    fn index(&self, index: Idx) -> &Self::Output {
        let arr = unsafe { &*self.ptr };
        &arr[index]
    }
}

impl<Idx> IndexMut<Idx> for DataPtr
where
    Idx: SliceIndex<[u8]>,
{
    #[inline]
    fn index_mut(&mut self, index: Idx) -> &mut Self::Output {
        let arr = unsafe { &mut *self.ptr };
        &mut arr[index]
    }
}

impl Drop for DataPtr {
    fn drop(&mut self) {
        if self.managed {
            let _ = unsafe { AlignedArray::<u8, CACHE_LINE_SIZE>::from_raw(self.ptr) };
        }
    }
}
