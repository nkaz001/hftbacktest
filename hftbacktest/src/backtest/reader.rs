use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    fs::File,
    io::{Error as IoError, ErrorKind, Read},
    marker::PhantomData,
    mem::{forget, size_of},
    ops::{Index, IndexMut},
    ptr::null_mut,
    rc::Rc,
    slice::SliceIndex,
};

use uuid::Uuid;

use crate::backtest::BacktestError;

/// Data source for the [`Reader`].
#[derive(Clone, Debug)]
pub enum DataSource<D>
where
    D: POD + Clone,
{
    /// Data needs to be loaded from the specified  file. It will be loaded when needed and released
    /// when no processor is reading the data.
    File(String),
    /// Data is loaded and set by the user.
    Data(Data<D>),
}

/// Marker trait for plain old data.
pub unsafe trait POD: Sized {}

/// Marker trait that indicates it can be directly cast from the loaded npy file data.
pub unsafe trait NpyFile: POD {}

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

#[derive(Debug)]
pub struct DataPtr {
    ptr: *mut [u8],
    managed: bool,
}

impl DataPtr {
    pub fn new(size: usize) -> Self {
        let x = aligned_vec(size);
        Self {
            ptr: Box::into_raw(x),
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
        let x = self.ptr as *const u8;
        unsafe { x.add(index) }
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
            _d_marker: Default::default(),
        }
    }

    pub unsafe fn from_ptr(ptr: *mut [u8], offset: usize) -> Self {
        Self {
            ptr: Rc::new(DataPtr::from_ptr(ptr)),
            offset,
            _d_marker: Default::default(),
        }
    }

    /// Returns a reference to an element, without doing bounds checking.
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, index: usize) -> &D {
        let size = size_of::<D>();
        let i = self.offset + index * size;
        unsafe { &*(self.ptr.at(i) as *const D) }
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

/// Provides a data cache that allows both the local processor and exchange processor to access the
/// same or different data based on their timestamps without the need for reloading.
#[derive(Clone, Debug)]
pub struct Cache<D>(Rc<RefCell<HashMap<String, (Cell<usize>, Data<D>)>>>)
where
    D: POD + Clone;

impl<D> Cache<D>
where
    D: POD + Clone,
{
    /// Constructs an instance of `Cache`.
    pub fn new() -> Self {
        Self(Default::default())
    }

    /// Inserts a key-value pair into the `Cache`.
    pub fn insert(&mut self, key: String, data: Data<D>) {
        self.0.borrow_mut().insert(key, (Cell::new(0), data));
    }

    /// Removes the `Data` if all retrieved `Data` are released.
    pub fn remove(&mut self, data: Data<D>) {
        let mut remove = None;
        for (key, (ref_count, cached_data)) in self.0.borrow_mut().iter_mut() {
            if Rc::ptr_eq(&data.ptr, &cached_data.ptr) {
                *ref_count.get_mut() -= 1;
                if ref_count.get() == 0 {
                    remove = Some(key.clone());
                }
                break;
            }
        }
        if let Some(key) = remove {
            self.0.borrow_mut().remove(&key).unwrap();
        }
    }

    /// Returns `true` if the `Cache` contains the `Data` for the specified key.
    pub fn contains(&self, key: &str) -> bool {
        self.0.borrow().contains_key(key)
    }

    /// Returns the `Data` corresponding to the key.
    pub fn get(&mut self, key: &str) -> Data<D> {
        let mut borrowed = self.0.borrow_mut();
        let (ref_count, data) = borrowed.get_mut(key).unwrap();
        *ref_count.get_mut() += 1;
        data.clone()
    }
}

/// Provides `Data` reading based on the given sequence of data through `Cache`.
#[derive(Clone, Debug)]
pub struct Reader<D>
where
    D: NpyFile + Clone,
{
    file_list: Vec<String>,
    cache: Cache<D>,
    data_num: usize,
}

impl<D> Reader<D>
where
    D: NpyFile + Clone,
{
    /// Constructs an instance of `Reader` that utilizes the provided `Cache`.
    pub fn new(cache: Cache<D>) -> Self {
        Self {
            file_list: Vec::new(),
            cache,
            data_num: 0,
        }
    }

    /// Adds a `numpy` file to read. Additions should be made in the same order as the order you
    /// want to read.
    pub fn add_file(&mut self, filepath: String) {
        self.file_list.push(filepath);
    }

    /// Adds a `Data`. Additions should be made in the same order as the order you want to read.
    pub fn add_data(&mut self, data: Data<D>) {
        // todo: Data should not be removed from the cache.
        let id = Uuid::new_v4().to_string();
        self.file_list.push(id.clone());
        self.cache.insert(id, data);
    }

    /// Releases this `Data` from the `Cache`. The `Cache` will delete the `Data` if there are no
    /// readers accessing it.
    pub fn release(&mut self, data: Data<D>) {
        self.cache.remove(data);
    }

    /// Retrieves the next `Data` based on the order of your additions.
    pub fn next(&mut self) -> Result<Data<D>, BacktestError> {
        if self.data_num < self.file_list.len() {
            let filepath = self.file_list.get(self.data_num).unwrap();
            if !self.cache.contains(filepath) {
                if filepath.ends_with(".npy") {
                    let data = read_npy(filepath)?;
                    self.cache.insert(filepath.to_string(), data);
                } else if filepath.ends_with(".npz") {
                    let data = read_npz(filepath)?;
                    self.cache.insert(filepath.to_string(), data);
                } else {
                    return Err(BacktestError::DataError(IoError::new(
                        ErrorKind::InvalidData,
                        "unsupported data type",
                    )));
                }
            }
            let data = self.cache.get(filepath);
            self.data_num += 1;
            Ok(data)
        } else {
            Err(BacktestError::EndOfData)
        }
    }
}

#[repr(C, align(64))]
struct Align64([u8; 64]);

fn aligned_vec(size: usize) -> Box<[u8]> {
    let capacity = (size / size_of::<Align64>()) + 1;
    let mut aligned: Vec<Align64> = Vec::with_capacity(capacity);

    let ptr = aligned.as_mut_ptr();
    let cap = aligned.capacity();

    forget(aligned);

    unsafe {
        Vec::from_raw_parts(ptr as *mut u8, size, cap * size_of::<Align64>()).into_boxed_slice()
    }
}

/// Reads a structured array `numpy` file. Currently, it doesn't check if the data structure is the
/// same as what the file contains. Users should be cautious about this.
pub fn read_npy<D: NpyFile + Clone>(filepath: &str) -> Result<Data<D>, IoError> {
    let mut file = File::open(filepath)?;

    file.sync_all()?;
    let size = file.metadata()?.len() as usize;
    let mut buf = DataPtr::new(size);

    let mut read_size = 0;
    while read_size < size {
        read_size += file.read(&mut buf[read_size..])?;
    }

    let header_len = u16::from_le_bytes(buf[8..10].try_into().unwrap()) as usize;
    // todo: add a check to ensure the structure matches the structured array described in the
    //       header.
    // let header = String::from_utf8(buf[10..(10 + header_len)].to_vec()).unwrap().to_string().trim().to_string();

    Ok(Data {
        ptr: Rc::new(buf),
        offset: 10 + header_len,
        _d_marker: Default::default(),
    })
}

/// Reads a structured array `numpy` zip archived file. Currently, it doesn't check if the data
/// structure is the same as what the file contains. Users should be cautious about this.
pub fn read_npz<D: NpyFile + Clone>(filepath: &str) -> Result<Data<D>, IoError> {
    let mut archive = zip::ZipArchive::new(File::open(filepath)?)?;

    let mut file = archive.by_index(0)?;

    let size = file.size() as usize;
    let mut buf = DataPtr::new(size);

    let mut read_size = 0;
    while read_size < size {
        read_size += file.read(&mut buf[read_size..])?;
    }

    let header_len = u16::from_le_bytes(buf[8..10].try_into().unwrap()) as usize;
    // todo: add a check to ensure the structure matches the structured array described in the
    //       header.
    // let header = String::from_utf8(buf[10..(10 + header_len)].to_vec()).unwrap().to_string().trim().to_string();

    Ok(Data {
        ptr: Rc::new(buf),
        offset: 10 + header_len,
        _d_marker: Default::default(),
    })
}
