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

pub use npy::{Field, NpyDTyped, NpyHeader, read_npy_file, read_npz_file, write_npy};
pub use reader::{Cache, DataPreprocess, DataSource, FeedLatencyAdjustment, Reader, ReaderBuilder};

use crate::utils::{AlignedArray, CACHE_LINE_SIZE};

/// Marker trait for C representation plain old data.
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

    /// Returns `true` if the `Data` is empty.
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

    pub fn from_data(data: &[D]) -> Self {
        let byte_len = size_of_val(data);
        let bytes = unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, byte_len) };

        let dest_data_ptr = DataPtr::new(byte_len);

        unsafe {
            let dest_data = dest_data_ptr.ptr.as_mut().unwrap();

            dest_data.copy_from_slice(bytes);
            Self::from_data_ptr(dest_data_ptr, 0)
        }
    }

    /// Constructs `Data` from [`DataPtr`] with the specified offset.
    ///
    /// # Safety
    /// The underlying memory layout must match the layout of type `D` and be aligned from the
    /// offset.
    pub unsafe fn from_data_ptr(ptr: DataPtr, offset: usize) -> Self {
        Self {
            ptr: Rc::new(ptr),
            offset,
            _d_marker: PhantomData,
        }
    }

    /// Returns a reference to an element, without doing bounds checking.
    ///
    /// # Safety
    /// Calling this method with an out-of-bounds index is undefined behavior even if the resulting
    /// reference is not used.
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, index: usize) -> &D {
        let size = size_of::<D>();
        let i = self.offset + index * size;
        unsafe { &*(self.ptr.at(i) as *const D) }
    }

    /// Returns `true` if the two `Data` point to the same data.
    pub fn data_eq(&self, other: &Self) -> bool {
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

impl<D> IndexMut<usize> for Data<D>
where
    D: POD + Clone,
{
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        let size = size_of::<D>();
        let i = self.offset + index * size;
        if i + size > self.ptr.len() {
            panic!("Out of the size.");
        }
        unsafe { &mut *(self.ptr.at(i) as *mut D) }
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

    /// Constructs a `DataPtr` from a fat pointer.
    ///
    /// Unlike other methods that construct an instance from a raw pointer, the raw pointer is not
    /// owned by the resulting `DataPtr`. Memory should still be managed by the caller.
    ///
    /// # Safety
    /// The fat pointer must remain valid for the lifetime of the resulting `DataPtr`.
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

    /// Returns a pointer offset by the given index.
    ///
    /// # Safety
    /// The `index` must be within the bounds of the array referenced by this pointer.
    /// Accessing an out-of-bounds offset is undefined behavior.
    #[inline]
    pub unsafe fn at(&self, index: usize) -> *const u8 {
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
