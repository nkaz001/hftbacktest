use std::{
    cell::RefCell,
    collections::HashMap,
    io::{Error as IoError, ErrorKind},
    rc::Rc,
    sync::{
        Arc,
        mpsc::{Receiver, Sender, channel},
    },
    thread,
};

use uuid::Uuid;

use crate::{
    backtest::{
        BacktestError,
        data::{
            Data,
            POD,
            npy::{NpyDTyped, read_npy_file, read_npz_file},
        },
    },
    types::Event,
};

/// Data source for the [`Reader`].
#[derive(Clone, Debug)]
pub enum DataSource<D>
where
    D: POD + Clone,
{
    /// Data needs to be loaded from the specified file. This should be a `numpy` file.
    ///
    /// It will be loaded when needed and released
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

    /// Sets the [`Data`] for the specified key and marks it as ready.
    pub fn set(&mut self, key: &str, data: Data<D>) {
        let mut borrowed = self.0.borrow_mut();
        let cached_data = borrowed.get_mut(key).unwrap();
        cached_data.set(data);
        cached_data.ready = true;
    }

    /// Returns `true` if the [`Data`] for the specified key is ready.
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

/// Directly implementing `Send` for `Data` may lead to unsafe sharing between threads. To mitigate
/// this risk, `DataSend` is used to wrap `Data`, which implements the `Send` marker trait. This
/// transfer ownership between threads while requiring careful consideration.
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

struct LoadDataResult<D>
where
    D: NpyDTyped + Clone,
{
    key: String,
    result: Result<DataSend<D>, IoError>,
}

impl<D> LoadDataResult<D>
where
    D: NpyDTyped + Clone,
{
    pub fn ok(key: String, data: Data<D>) -> Self {
        Self {
            key,
            result: Ok(DataSend(data)),
        }
    }

    pub fn err(key: String, error: IoError) -> Self {
        Self {
            key,
            result: Err(error),
        }
    }
}

/// A builder for constructing [`Reader`].
pub struct ReaderBuilder<D>
where
    D: NpyDTyped + POD + Clone,
{
    data_key_list: Vec<String>,
    cache: Cache<D>,
    temporary_data: HashMap<String, Data<D>>,
    parallel_load: bool,
    preprocessor: Option<Arc<Box<dyn DataPreprocess<D> + Sync + Send + 'static>>>,
}

impl<D> Default for ReaderBuilder<D>
where
    D: NpyDTyped + POD + Clone,
{
    fn default() -> Self {
        Self {
            data_key_list: Default::default(),
            cache: Default::default(),
            temporary_data: Default::default(),
            parallel_load: false,
            preprocessor: None,
        }
    }
}

impl<D> ReaderBuilder<D>
where
    D: NpyDTyped + POD + Clone,
{
    /// Constructs a `ReaderBuilder`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets whether to load the next data in parallel. This allows [`Reader`] to not only load the
    /// next data but also preload subsequent data, ensuring it is ready in advance.
    ///
    /// Loading is performed by spawning a separate thread.
    ///
    /// The default value is `true`.
    pub fn parallel_load(self, parallel_load: bool) -> Self {
        Self {
            parallel_load,
            ..self
        }
    }

    /// Sets a [`DataPreprocess`].
    pub fn preprocessor<Preprocessor>(self, preprocessor: Preprocessor) -> Self
    where
        Preprocessor: DataPreprocess<D> + Sync + Send + 'static,
    {
        Self {
            preprocessor: Some(Arc::new(Box::new(preprocessor))),
            ..self
        }
    }

    /// Sets the data to be read by [`Reader`]. The items in the `data` vector should be arranged in
    /// the chronological order.
    pub fn data(self, data: Vec<DataSource<D>>) -> Self {
        let mut data_key_list = self.data_key_list;
        let mut temporary_data = self.temporary_data;
        for item in data {
            match item {
                DataSource::File(filepath) => {
                    data_key_list.push(filepath);
                }
                DataSource::Data(data) => {
                    let key = Uuid::new_v4().to_string();
                    data_key_list.push(key.clone());
                    temporary_data.insert(key, data);
                }
            }
        }
        Self {
            data_key_list,
            temporary_data,
            ..self
        }
    }

    /// Builds a [`Reader`].
    pub fn build(self) -> Result<Reader<D>, IoError> {
        let mut cache = self.cache.clone();
        for (key, mut data) in self.temporary_data {
            if let Some(p) = &self.preprocessor {
                p.preprocess(&mut data)?;
            }
            cache.insert(key, data)
        }

        let (tx, rx) = channel();
        Ok(Reader {
            data_key_list: self.data_key_list.clone(),
            cache,
            data_num: 0,
            tx,
            rx: Rc::new(rx),
            parallel_load: self.parallel_load,
            preprocessor: self.preprocessor.clone(),
        })
    }
}

/// Provides `Data` reading based on the given sequence of data through `Cache`.
#[derive(Clone)]
pub struct Reader<D>
where
    D: NpyDTyped + Clone,
{
    data_key_list: Vec<String>,
    cache: Cache<D>,
    data_num: usize,
    tx: Sender<LoadDataResult<D>>,
    rx: Rc<Receiver<LoadDataResult<D>>>,
    parallel_load: bool,
    preprocessor: Option<Arc<Box<dyn DataPreprocess<D> + Sync + Send + 'static>>>,
}

impl<D> Reader<D>
where
    D: NpyDTyped + Clone + 'static,
{
    /// Returns a [`ReaderBuilder`].
    pub fn builder() -> ReaderBuilder<D> {
        ReaderBuilder::default()
    }

    /// Releases this [`Data`] from the `Cache`. The `Cache` will delete the [`Data`] if there are
    /// no readers accessing it.
    pub fn release(&mut self, data: Data<D>) {
        self.cache.remove(data);
    }

    /// Retrieves the next [`Data`] based on the order of your additions.
    pub fn next_data(&mut self) -> Result<Data<D>, BacktestError> {
        if self.data_num < self.data_key_list.len() {
            let key = self.data_key_list.get(self.data_num).cloned().unwrap();
            self.load_data(&key)?;

            if self.parallel_load {
                let next_key = self.data_key_list.get(self.data_num + 1).cloned();
                if let Some(next_key) = next_key {
                    self.load_data(&next_key)?;
                }
            }

            while !self.cache.is_ready(&key) {
                match self.rx.recv().unwrap() {
                    LoadDataResult {
                        key,
                        result: Ok(data),
                    } => {
                        self.cache.set(&key, data.unwrap());
                    }
                    LoadDataResult {
                        result: Err(err), ..
                    } => {
                        return Err(BacktestError::DataError(std::io::Error::new(
                            err.kind(),
                            format!("Failed to read file '{key}': {err}"),
                        )));
                    }
                }
            }

            let data = self.cache.get(&key);
            self.data_num += 1;
            Ok(data)
        } else {
            Err(BacktestError::EndOfData)
        }
    }

    fn load_data(&mut self, key: &str) -> Result<(), BacktestError> {
        if !self.cache.contains(key) {
            self.cache.prepare(key.to_string());

            if key.ends_with(".npy") {
                let tx = self.tx.clone();
                let filepath = key.to_string();
                let preprocessor = self.preprocessor.clone();

                let _ = thread::spawn(move || {
                    let load_data = |filepath: &str| {
                        let mut data = read_npy_file::<D>(filepath)?;
                        if let Some(preprocessor) = &preprocessor {
                            preprocessor.preprocess(&mut data)?;
                        }
                        Ok(data)
                    };
                    // SendError occurs only if Reader is already destroyed. Since no data is needed
                    // once the Reader is destroyed, SendError is safely suppressed.
                    match load_data(&filepath) {
                        Ok(data) => {
                            let _ = tx.send(LoadDataResult::ok(filepath, data));
                        }
                        Err(err) => {
                            let _ = tx.send(LoadDataResult::err(filepath, err));
                        }
                    }
                });
            } else if key.ends_with(".npz") {
                let tx = self.tx.clone();
                let filepath = key.to_string();
                let preprocessor = self.preprocessor.clone();

                let _ = thread::spawn(move || {
                    let load_data = |filepath: &str| {
                        let mut data = read_npz_file::<D>(filepath, "data")?;
                        if let Some(preprocessor) = &preprocessor {
                            preprocessor.preprocess(&mut data)?;
                        }
                        Ok(data)
                    };
                    // SendError occurs only if Reader is already destroyed. Since no data is needed
                    // once the Reader is destroyed, SendError is safely suppressed.
                    match load_data(&filepath) {
                        Ok(data) => {
                            let _ = tx.send(LoadDataResult::ok(filepath, data));
                        }
                        Err(err) => {
                            let _ = tx.send(LoadDataResult::err(filepath, err));
                        }
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

/// `DataPreprocess` offers a function to preprocess data before it is fed into the backtesting.
/// This feature is primarily introduced to adjust timestamps, making it particularly useful when
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
    fn preprocess(&self, data: &mut Data<D>) -> Result<(), IoError>;
}

/// Pre-processes the feed data to adjust for latency. `local_ts` is offset by the specified latency
/// offset.
#[derive(Clone)]
pub struct FeedLatencyAdjustment {
    latency_offset: i64,
}

impl FeedLatencyAdjustment {
    /// Constructs a `FeedLatencyAdjustment`.
    pub fn new(latency_offset: i64) -> Self {
        Self { latency_offset }
    }
}

impl DataPreprocess<Event> for FeedLatencyAdjustment {
    fn preprocess(&self, data: &mut Data<Event>) -> Result<(), IoError> {
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
