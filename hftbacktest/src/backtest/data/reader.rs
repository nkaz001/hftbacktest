use std::{
    cell::RefCell,
    collections::HashMap,
    io::{Error as IoError, ErrorKind},
    rc::Rc,
    sync::mpsc::{channel, Receiver, Sender},
    thread,
};

use uuid::Uuid;

use crate::{
    backtest::{
        data::{
            npy::{read_npy_file, read_npz_file, NpyDTyped},
            Data,
            POD,
        },
        BacktestError,
    },
    types::Event,
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
    ready: bool,
    data: Data<D>,
}

impl<D> CachedData<D>
where
    D: POD + Clone,
{
    pub fn new(data: Data<D>) -> Self {
        Self {
            count: 0,
            ready: true,
            data,
        }
    }

    pub fn empty() -> Self {
        Self {
            count: 0,
            ready: false,
            data: Data::empty(),
        }
    }

    pub fn set(&mut self, data: Data<D>) {
        self.data = data;
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

    /// Prepares cached data by inserting a key-value pair with empty data into the `Cache`.
    /// This placeholder will be replaced when the actual data is ready.
    pub fn prepare(&mut self, key: String) {
        self.0.borrow_mut().insert(key, CachedData::empty());
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

    pub fn set(&mut self, key: &str, data: Data<D>) {
        let mut borrowed = self.0.borrow_mut();
        let cached_data = borrowed.get_mut(key).unwrap();
        cached_data.set(data);
        cached_data.ready = true;
    }

    pub fn is_ready(&self, key: &str) -> bool {
        self.0.borrow().get(key).unwrap().ready
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
#[derive(Clone)]
pub struct DefaultReader<D, Preprocessor = NullPreprocessor>
where
    D: NpyDTyped + Clone,
    Preprocessor: DataPreprocess<D> + Clone,
{
    file_list: Vec<String>,
    cache: Cache<D>,
    data_num: usize,
    preprocessor: Preprocessor,
}

impl<D, Preprocessor> DefaultReader<D, Preprocessor>
where
    D: NpyDTyped + Clone,
    Preprocessor: DataPreprocess<D> + Clone,
{
    /// Constructs an instance of `Reader` that utilizes the provided `Cache`.
    pub fn new(cache: Cache<D>, preprocessor: Preprocessor) -> Self {
        Self {
            file_list: Vec::new(),
            cache,
            data_num: 0,
            preprocessor,
        }
    }

    /// Adds a `numpy` file to read. Additions should be made in the same order as the order you
    /// want to read.
    pub fn add_file(&mut self, filepath: String) {
        self.file_list.push(filepath);
    }

    /// Adds a `Data`. Additions should be made in the same order as the order you want to read.
    pub fn add_data(&mut self, mut data: Data<D>) {
        // todo: Data should not be removed from the cache.
        let id = Uuid::new_v4().to_string();
        self.file_list.push(id.clone());
        // fixme: error handling.
        self.preprocessor.preprocess(&mut data).unwrap();
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
                    let mut data = read_npy_file(filepath)?;
                    self.preprocessor.preprocess(&mut data)?;
                    self.cache.insert(filepath.to_string(), data);
                } else if filepath.ends_with(".npz") {
                    let mut data = read_npz_file(filepath, "data")?;
                    self.preprocessor.preprocess(&mut data)?;
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

struct DataSend<D>(Data<D>)
where
    D: NpyDTyped + Clone;

impl<D> DataSend<D>
where
    D: NpyDTyped + Clone,
{
    pub fn unwrap(self) -> Data<D> {
        self.0
    }
}
unsafe impl<D> Send for DataSend<D> where D: NpyDTyped + Clone {}

pub struct LoadDataResult<D>
where
    D: NpyDTyped + Clone,
{
    filepath: String,
    result: Result<DataSend<D>, IoError>,
}

impl<D> LoadDataResult<D>
where
    D: NpyDTyped + Clone,
{
    pub fn ok(filepath: String, data: Data<D>) -> Self {
        Self {
            filepath,
            result: Ok(DataSend(data)),
        }
    }

    pub fn err(filepath: String, err: IoError) -> Self {
        Self {
            filepath,
            result: Err(err),
        }
    }
}

/// Provides `Data` reading based on the given sequence of data through `Cache`.
#[derive(Clone, Debug)]
pub struct ParallelReader<D>
where
    D: NpyDTyped + Clone,
{
    file_list: Vec<String>,
    cache: Cache<D>,
    data_num: usize,
    tx: Sender<LoadDataResult<D>>,
    rx: Rc<Receiver<LoadDataResult<D>>>,
}

impl<D> ParallelReader<D>
where
    D: NpyDTyped + Clone + Send + 'static,
{
    /// Constructs an instance of `Reader` that utilizes the provided `Cache`.
    pub fn new(cache: Cache<D>) -> Self {
        let (tx, rx) = channel();
        Self {
            file_list: Vec::new(),
            cache,
            data_num: 0,
            tx,
            rx: Rc::new(rx),
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
            let filepath = {
                let filepath = self.file_list.get(self.data_num).cloned().unwrap();
                let next_filepath = self.file_list.get(self.data_num + 1).cloned();

                self.load_data(&filepath)?;
                if let Some(filepath) = next_filepath {
                    self.load_data(&filepath)?;
                }
                filepath
            };

            while !self.cache.is_ready(&filepath) {
                match self.rx.recv().unwrap() {
                    LoadDataResult {
                        filepath,
                        result: Ok(data),
                    } => {
                        self.cache.set(&filepath, data.unwrap());
                    }
                    LoadDataResult {
                        result: Err(err), ..
                    } => {
                        return Err(BacktestError::DataError(err));
                    }
                }
            }

            let data = self.cache.get(&filepath);
            self.data_num += 1;
            Ok(data)
        } else {
            Err(BacktestError::EndOfData)
        }
    }

    fn load_data(&mut self, filepath: &str) -> Result<(), BacktestError> {
        if !self.cache.contains(filepath) {
            self.cache.prepare(filepath.to_string());

            if filepath.ends_with(".npy") {
                let tx = self.tx.clone();
                let filepath_ = filepath.to_string();
                let _ = thread::spawn(move || match read_npy_file::<D>(&filepath_) {
                    Ok(data) => {
                        tx.send(LoadDataResult::ok(filepath_, data)).unwrap();
                    }
                    Err(err) => {
                        tx.send(LoadDataResult::err(filepath_, err)).unwrap();
                    }
                });
            } else if filepath.ends_with(".npz") {
                let tx = self.tx.clone();
                let filepath_ = filepath.to_string();
                let _ = thread::spawn(move || match read_npz_file::<D>(&filepath_, "data") {
                    Ok(data) => {
                        tx.send(LoadDataResult::ok(filepath_, data)).unwrap();
                    }
                    Err(err) => {
                        tx.send(LoadDataResult::err(filepath_, err)).unwrap();
                    }
                });
            } else {
                return Err(BacktestError::DataError(IoError::new(
                    ErrorKind::InvalidData,
                    "unsupported data type",
                )));
            }
        }
        Ok(())
    }
}
              
/// DataPreprocess offers a function to preprocess data before it is fed into the backtesting. This
/// feature is primarily introduced to adjust timestamps, making it particularly useful when
/// backtesting the market from a location different from where your order latency was originally
/// collected.
///
/// For example, if you're backtesting an arbitrage strategy between Binance Futures and ByBit,
/// and your order latency data was collected in a colocated AWS region, you may need to adjust
/// for the geographical difference. If your strategy is running with a base in Tokyo
/// (where Binance Futures is located), you would need to account for the latency between
/// Singapore (where ByBit is located) and Tokyo by applying an appropriate offset.
pub trait DataPreprocess<D>
where
    D: POD + Clone,
{
    fn preprocess(&mut self, data: &mut Data<D>) -> Result<(), IoError>;
}

#[derive(Clone, Default)]
pub struct NullPreprocessor;

impl<D> DataPreprocess<D> for NullPreprocessor
where
    D: POD + Clone,
{
    fn preprocess(&mut self, _data: &mut Data<D>) -> Result<(), IoError> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct FeedLatencyAdjustment {
    latency_offset: i64,
}

impl FeedLatencyAdjustment {
    pub fn new(latency_offset: i64) -> Self {
        Self { latency_offset }
    }
}

impl DataPreprocess<Event> for FeedLatencyAdjustment {
    fn preprocess(&mut self, data: &mut Data<Event>) -> Result<(), IoError> {
        for i in 0..data.len() {
            data[i].local_ts += self.latency_offset;
            if data[i].local_ts <= data[i].exch_ts {
                return Err(IoError::new(
                    ErrorKind::InvalidData,
                    "`local_ts` became less than or \
                    equal to `exch_ts` after applying the latency offset",
                ));
            }
        }
        Ok(())
    }
}

#[cfg(feature = "unstable_parallel_load")]
pub type Reader<D> = ParallelReader<D>;
#[cfg(not(feature = "unstable_parallel_load"))]
pub type Reader<D> = DefaultReader<D>;
