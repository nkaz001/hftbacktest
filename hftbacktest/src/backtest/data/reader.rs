use std::{
    any::TypeId,
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

pub use crate::data::TardisStreamDayConfig;

use crate::{
    backtest::{
        BacktestError,
        data::{
            Data, POD,
            npy::{NpyDTyped, read_npy_file, read_npz_file},
        },
    },
    data::{StreamingTardisReader, StreamingTardisReaderConfig},
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
    /// Data is streamed from raw Tardis CSV.gz files.
    TardisStream(TardisStreamDayConfig),
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
    payload: LoadDataPayload<D>,
}

impl<D> LoadDataResult<D>
where
    D: NpyDTyped + Clone,
{
    pub fn ok(key: String, data: Data<D>) -> Self {
        Self {
            payload: LoadDataPayload::Data {
                key,
                result: Ok(DataSend(data)),
            },
        }
    }

    pub fn err(key: String, error: IoError) -> Self {
        Self {
            payload: LoadDataPayload::Data {
                key,
                result: Err(error),
            },
        }
    }

    pub fn stream_chunk(stream_key: String, chunk_key: String, data: Data<D>) -> Self {
        Self {
            payload: LoadDataPayload::StreamChunk {
                stream_key,
                chunk_key,
                result: Ok(DataSend(data)),
            },
        }
    }

    pub fn stream_error(stream_key: String, error: IoError) -> Self {
        Self {
            payload: LoadDataPayload::StreamError { stream_key, error },
        }
    }

    pub fn stream_end(stream_key: String) -> Self {
        Self {
            payload: LoadDataPayload::StreamEnd { stream_key },
        }
    }
}

enum LoadDataPayload<D>
where
    D: NpyDTyped + Clone,
{
    Data {
        key: String,
        result: Result<DataSend<D>, IoError>,
    },
    StreamChunk {
        stream_key: String,
        chunk_key: String,
        result: Result<DataSend<D>, IoError>,
    },
    StreamError {
        stream_key: String,
        error: IoError,
    },
    StreamEnd {
        stream_key: String,
    },
}

#[derive(Default)]
struct StreamState {
    started: bool,
    finished: bool,
    /// Append-only list of chunk cache keys, in production order.
    chunk_keys: Vec<String>,
    /// Per-chunk counter: how many Reader clones have consumed this chunk.
    chunk_consumed: Vec<usize>,
    /// Index of the next chunk to ack (all consumers must have consumed it first).
    ack_cursor: usize,
    ack_tx: Option<Sender<()>>,
    error: Option<IoError>,
}

/// A builder for constructing [`Reader`].
pub struct ReaderBuilder<D>
where
    D: NpyDTyped + POD + Clone,
{
    data_key_list: Vec<String>,
    cache: Cache<D>,
    temporary_data: HashMap<String, Data<D>>,
    stream_sources: HashMap<String, TardisStreamDayConfig>,
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
            stream_sources: Default::default(),
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
        let mut stream_sources = self.stream_sources;
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
                DataSource::TardisStream(config) => {
                    let key = format!("tardis-stream://{}", Uuid::new_v4());
                    data_key_list.push(key.clone());
                    stream_sources.insert(key, config);
                }
            }
        }
        Self {
            data_key_list,
            temporary_data,
            stream_sources,
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
            stream_sources: self.stream_sources,
            stream_states: Default::default(),
            stream_cursors: HashMap::new(),
            tx,
            rx: Rc::new(rx),
            parallel_load: self.parallel_load,
            preprocessor: self.preprocessor.clone(),
        })
    }
}

/// Provides `Data` reading based on the given sequence of data through `Cache`.
pub struct Reader<D>
where
    D: NpyDTyped + Clone,
{
    data_key_list: Vec<String>,
    cache: Cache<D>,
    data_num: usize,
    stream_sources: HashMap<String, TardisStreamDayConfig>,
    stream_states: Rc<RefCell<HashMap<String, StreamState>>>,
    /// Per-clone cursor: tracks how far this clone has read in each stream.
    stream_cursors: HashMap<String, usize>,
    tx: Sender<LoadDataResult<D>>,
    rx: Rc<Receiver<LoadDataResult<D>>>,
    parallel_load: bool,
    preprocessor: Option<Arc<Box<dyn DataPreprocess<D> + Sync + Send + 'static>>>,
}

impl<D> Clone for Reader<D>
where
    D: NpyDTyped + Clone,
{
    fn clone(&self) -> Self {
        Self {
            data_key_list: self.data_key_list.clone(),
            cache: self.cache.clone(),
            data_num: self.data_num,
            stream_sources: self.stream_sources.clone(),
            stream_states: self.stream_states.clone(),
            // Each clone starts with fresh cursors so it reads all chunks from the beginning.
            stream_cursors: HashMap::new(),
            tx: self.tx.clone(),
            rx: self.rx.clone(),
            parallel_load: self.parallel_load,
            preprocessor: self.preprocessor.clone(),
        }
    }
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
        while self.data_num < self.data_key_list.len() {
            let key = self.data_key_list.get(self.data_num).cloned().unwrap();
            self.load_data(&key)?;

            if self.parallel_load {
                let next_key = self.data_key_list.get(self.data_num + 1).cloned();
                if let Some(next_key) = next_key {
                    self.load_data(&next_key)?;
                }
            }

            if self.stream_sources.contains_key(&key) {
                // Check for prior errors.
                {
                    let borrowed = self.stream_states.borrow();
                    let state = borrowed.get(&key).unwrap();
                    if let Some(err) = &state.error {
                        return Err(BacktestError::DataError(IoError::new(
                            err.kind(),
                            format!("Failed to stream data '{key}': {err}"),
                        )));
                    }
                }

                let cursor = *self.stream_cursors.get(&key).unwrap_or(&0);

                // Wait until a chunk is available at our cursor position, or stream is finished.
                while {
                    let borrowed = self.stream_states.borrow();
                    borrowed
                        .get(&key)
                        .is_some_and(|state| state.chunk_keys.len() <= cursor && !state.finished)
                } {
                    let result = self.rx.recv().unwrap();
                    self.handle_load_result(result)?;
                }

                let (chunk_key, should_advance) = {
                    let borrowed = self.stream_states.borrow();
                    let state = borrowed.get(&key).unwrap();
                    if let Some(err) = &state.error {
                        return Err(BacktestError::DataError(IoError::new(
                            err.kind(),
                            format!("Failed to stream data '{key}': {err}"),
                        )));
                    }
                    if cursor < state.chunk_keys.len() {
                        let chunk_key = state.chunk_keys[cursor].clone();
                        let should_advance =
                            state.finished && cursor + 1 >= state.chunk_keys.len();
                        (Some(chunk_key), should_advance)
                    } else {
                        (None, false)
                    }
                };

                if let Some(chunk_key) = chunk_key {
                    let data = self.cache.get(&chunk_key);

                    // Advance this clone's cursor.
                    self.stream_cursors.insert(key.clone(), cursor + 1);

                    // Mark chunk as consumed by this clone; send ack when all clones are done.
                    {
                        let mut borrowed = self.stream_states.borrow_mut();
                        let state = borrowed.get_mut(&key).unwrap();
                        let num_consumers = Rc::strong_count(&self.stream_states);

                        state.chunk_consumed[cursor] += 1;

                        // Advance ack cursor for all fully-consumed chunks.
                        while state.ack_cursor < state.chunk_consumed.len()
                            && state.chunk_consumed[state.ack_cursor] >= num_consumers
                        {
                            if let Some(ack_tx) = &state.ack_tx {
                                let _ = ack_tx.send(());
                            }
                            state.ack_cursor += 1;
                        }
                    }

                    if should_advance {
                        self.data_num += 1;
                    }
                    return Ok(data);
                }

                {
                    let borrowed = self.stream_states.borrow();
                    if let Some(err) = borrowed.get(&key).and_then(|state| state.error.as_ref()) {
                        return Err(BacktestError::DataError(IoError::new(
                            err.kind(),
                            format!("Failed to stream data '{key}': {err}"),
                        )));
                    }
                }

                self.data_num += 1;
                continue;
            }

            while !self.cache.is_ready(&key) {
                let result = self.rx.recv().unwrap();
                self.handle_load_result(result)?;
            }

            let data = self.cache.get(&key);
            self.data_num += 1;
            return Ok(data);
        }

        Err(BacktestError::EndOfData)
    }

    fn handle_load_result(&mut self, result: LoadDataResult<D>) -> Result<(), BacktestError> {
        match result.payload {
            LoadDataPayload::Data {
                key,
                result: Ok(data),
            } => {
                self.cache.set(&key, data.unwrap());
                Ok(())
            }
            LoadDataPayload::Data {
                key,
                result: Err(err),
            } => Err(BacktestError::DataError(IoError::new(
                err.kind(),
                format!("Failed to read file '{key}': {err}"),
            ))),
            LoadDataPayload::StreamChunk {
                stream_key,
                chunk_key,
                result: Ok(data),
            } => {
                self.cache.insert(chunk_key.clone(), data.unwrap());
                let mut borrowed = self.stream_states.borrow_mut();
                let state = borrowed.get_mut(&stream_key).unwrap();
                state.chunk_keys.push(chunk_key);
                state.chunk_consumed.push(0);
                Ok(())
            }
            LoadDataPayload::StreamChunk {
                stream_key,
                result: Err(err),
                ..
            } => {
                let mut borrowed = self.stream_states.borrow_mut();
                let state = borrowed.get_mut(&stream_key).unwrap();
                state.error = Some(err);
                state.finished = true;
                Ok(())
            }
            LoadDataPayload::StreamError { stream_key, error } => {
                let mut borrowed = self.stream_states.borrow_mut();
                let state = borrowed.get_mut(&stream_key).unwrap();
                state.error = Some(error);
                state.finished = true;
                Ok(())
            }
            LoadDataPayload::StreamEnd { stream_key } => {
                self.stream_states.borrow_mut().get_mut(&stream_key).unwrap().finished = true;
                Ok(())
            }
        }
    }

    fn load_data(&mut self, key: &str) -> Result<(), BacktestError> {
        if let Some(stream_config) = self.stream_sources.get(key).cloned() {
            {
                let mut borrowed = self.stream_states.borrow_mut();
                let state = borrowed.entry(key.to_string()).or_default();
                if state.started {
                    return Ok(());
                }
                if TypeId::of::<D>() != TypeId::of::<Event>() {
                    return Err(BacktestError::DataError(IoError::new(
                        ErrorKind::InvalidData,
                        "TardisStream sources are only supported for Event data",
                    )));
                }

                state.started = true;
                let (ack_tx, ack_rx) = channel();
                state.ack_tx = Some(ack_tx);

                let tx = self.tx.clone();
                let stream_key = key.to_string();
                let preprocessor = self.preprocessor.clone();
                drop(borrowed);
                let _ = thread::spawn(move || {
                    let stream_result = (|| -> Result<(), IoError> {
                        let mut reader = StreamingTardisReader::new(
                            stream_config,
                            StreamingTardisReaderConfig::default(),
                        )
                        .map_err(|err| IoError::new(ErrorKind::InvalidData, err))?;
                        let mut chunk_no = 0usize;

                        while let Some(chunk) = reader
                            .next_data()
                            .map_err(|err| IoError::new(ErrorKind::InvalidData, err))?
                        {
                            let mut data = cast_event_data::<D>(chunk);
                            if let Some(preprocessor) = &preprocessor {
                                preprocessor.preprocess(&mut data)?;
                            }

                            let chunk_key = format!("{stream_key}#chunk:{chunk_no:08}");
                            chunk_no += 1;
                            if tx
                                .send(LoadDataResult::stream_chunk(
                                    stream_key.clone(),
                                    chunk_key,
                                    data,
                                ))
                                .is_err()
                            {
                                return Ok(());
                            }
                            if ack_rx.recv().is_err() {
                                return Ok(());
                            }
                        }

                        let _ = tx.send(LoadDataResult::stream_end(stream_key.clone()));
                        Ok(())
                    })();

                    if let Err(err) = stream_result {
                        let _ = tx.send(LoadDataResult::stream_error(stream_key, err));
                    }
                });
                return Ok(());
            }
        }

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

fn cast_event_data<D>(data: Data<Event>) -> Data<D>
where
    D: NpyDTyped + Clone + 'static,
{
    debug_assert_eq!(TypeId::of::<D>(), TypeId::of::<Event>());
    unsafe { std::mem::transmute::<Data<Event>, Data<D>>(data) }
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
