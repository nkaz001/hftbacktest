use std::{
    marker::PhantomData,
    mem::{forget, size_of},
    ops::{Index, IndexMut},
    ptr::null_mut,
    rc::Rc,
    slice::SliceIndex,
};

/// Marker trait for plain old data.
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
        let vec = aligned_heap_array(size);
        Self {
            ptr: Box::into_raw(vec),
            managed: true,
        }
    }

    pub fn from_ptr(ptr: *mut [u8]) -> Self {
        Self {
            ptr,
            managed: false,
        }
    }

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
            let _ = unsafe { Box::from_raw(self.ptr) };
        }
    }
}

#[repr(C, align(64))]
struct Align64([u8; 64]);

fn aligned_heap_array(size: usize) -> Box<[u8]> {
    let capacity = (size / size_of::<Align64>()) + 1;
    let mut aligned: Vec<Align64> = Vec::with_capacity(capacity);
    unsafe {
        aligned.set_len(capacity);
    }

    let ptr = aligned.as_mut_ptr();
    let len = aligned.len();
    let cap = aligned.capacity();

    forget(aligned);

    let vec = unsafe {
        Vec::from_raw_parts(
            ptr.cast(),
            len * size_of::<Align64>(),
            cap * size_of::<Align64>(),
        )
    };
    vec.into_boxed_slice()
}
