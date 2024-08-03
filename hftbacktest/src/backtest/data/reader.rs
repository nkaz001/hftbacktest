use std::{
    cell::RefCell,
    collections::HashMap,
    io::{Error as IoError, ErrorKind},
    rc::Rc,
};

use uuid::Uuid;

use crate::backtest::{
    data::{
        npy::{read_npy_file, read_npz_file, NpyDTyped},
        Data,
        POD,
    },
    BacktestError,
};

/// Data source for the [`Reader`].
#[derive(Clone, Debug)]
pub enum DataSource<D>
where
    D: POD + Clone,
{
    /// Data needs to be loaded from the specified file. It will be loaded when needed and released
    /// when no [Processor](`crate::backtest::proc::Processor`) is reading the data.
    File(String),
    /// Data is loaded and set by the user.
    Data(Data<D>),
}

#[derive(Debug)]
struct CachedData<D>
where
    D: POD + Clone,
{
    count: usize,
    data: Data<D>,
}

impl<D> CachedData<D>
where
    D: POD + Clone,
{
    pub fn new(data: Data<D>) -> Self {
        Self { count: 0, data }
    }

    pub fn checkout(&mut self) -> Data<D> {
        self.count += 1;
        self.data.clone()
    }

    pub fn turn_in(&mut self) -> bool {
        self.count -= 1;
        self.count == 0
    }
}

/// Provides a data cache that allows both the local processor and exchange processor to access the
/// same or different data based on their timestamps without the need for reloading.
#[derive(Clone, Debug)]
pub struct Cache<D>(Rc<RefCell<HashMap<String, CachedData<D>>>>)
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
        self.0.borrow_mut().insert(key, CachedData::new(data));
    }

    /// Removes the [`Data`] if all retrieved [`Data`] are released.
    pub fn remove(&mut self, data: Data<D>) {
        let mut remove = None;
        for (key, cached_data) in self.0.borrow_mut().iter_mut() {
            if data.data_eq(&cached_data.data) {
                if cached_data.turn_in() {
                    remove = Some(key.clone());
                }
                break;
            }
        }
        if let Some(key) = remove {
            self.0.borrow_mut().remove(&key).unwrap();
        }
    }

    /// Returns `true` if the `Cache` contains the [`Data`] for the specified key.
    pub fn contains(&self, key: &str) -> bool {
        self.0.borrow().contains_key(key)
    }

    /// Returns the [`Data`] corresponding to the key.
    pub fn get(&mut self, key: &str) -> Data<D> {
        let mut borrowed = self.0.borrow_mut();
        let cached_data = borrowed.get_mut(key).unwrap();
        cached_data.checkout()
    }
}

impl<D> Default for Cache<D>
where
    D: POD + Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Provides `Data` reading based on the given sequence of data through `Cache`.
#[derive(Clone, Debug)]
pub struct Reader<D>
where
    D: NpyDTyped + Clone,
{
    file_list: Vec<String>,
    cache: Cache<D>,
    data_num: usize,
}

impl<D> Reader<D>
where
    D: NpyDTyped + Clone,
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
    pub fn next_data(&mut self) -> Result<Data<D>, BacktestError> {
        if self.data_num < self.file_list.len() {
            let filepath = self.file_list.get(self.data_num).unwrap();
            if !self.cache.contains(filepath) {
                if filepath.ends_with(".npy") {
                    let data = read_npy_file(filepath)?;
                    self.cache.insert(filepath.to_string(), data);
                } else if filepath.ends_with(".npz") {
                    let data = read_npz_file(filepath, "data")?;
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
