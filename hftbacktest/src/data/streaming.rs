use std::{
    cmp::Ordering,
    collections::{BinaryHeap, VecDeque},
    io::BufRead,
    sync::mpsc::{Receiver, SyncSender, sync_channel},
    thread,
};

use crate::{
    backtest::data::Data,
    data::tardis_fast::{
        BookTickerLayout, DepthConverterFast, open_file_fast, parse_bookticker_to_events,
        parse_depth_line, parse_trade_to_event,
    },
    types::{EXCH_EVENT, Event, LOCAL_EVENT},
};

use super::tardis::TardisConvertConfig;

const SOURCE_CHANNEL_CAPACITY: usize = 1024;
const SOURCE_BATCH_SIZE: usize = 65_536;
const DEFAULT_CHUNK_SIZE: usize = 100_000;
const DEFAULT_REORDER_WINDOW: usize = 8_192;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TardisStreamDayConfig {
    pub trades: String,
    pub depth: String,
    pub book_ticker: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum StreamLocalTimestampPolicy {
    #[default]
    Clamp,
    ErrorOnNegative,
}

#[derive(Clone, Debug)]
pub struct StreamingTardisReaderConfig {
    pub convert: TardisConvertConfig,
    pub chunk_size: usize,
    pub reorder_window: usize,
    pub local_timestamp_policy: StreamLocalTimestampPolicy,
}

impl Default for StreamingTardisReaderConfig {
    fn default() -> Self {
        Self {
            convert: TardisConvertConfig::default(),
            chunk_size: DEFAULT_CHUNK_SIZE,
            reorder_window: DEFAULT_REORDER_WINDOW,
            local_timestamp_policy: StreamLocalTimestampPolicy::Clamp,
        }
    }
}

pub struct StreamingTardisReader {
    sources: Vec<SourceState>,
    exch_heap: BinaryHeap<ExchHead>,
    pending_exch: VecDeque<PendingEvent>,
    local_heap: BinaryHeap<LocalHead>,
    chunk_size: usize,
    reorder_window: usize,
}

impl StreamingTardisReader {
    pub fn new(
        day: TardisStreamDayConfig,
        config: StreamingTardisReaderConfig,
    ) -> Result<Self, String> {
        if config.chunk_size == 0 {
            return Err("streaming chunk size must be greater than zero".to_string());
        }
        if config.reorder_window == 0 {
            return Err("streaming reorder window must be greater than zero".to_string());
        }

        let mut sources = vec![
            SourceState::spawn(
                0,
                SourceKind::Trades,
                day.trades,
                config.convert.clone(),
                config.local_timestamp_policy,
            ),
            SourceState::spawn(
                1,
                SourceKind::Depth,
                day.depth,
                config.convert.clone(),
                config.local_timestamp_policy,
            ),
        ];

        if let Some(path) = day.book_ticker {
            sources.push(SourceState::spawn(
                2,
                SourceKind::BookTicker,
                path,
                config.convert.clone(),
                config.local_timestamp_policy,
            ));
        }

        let mut reader = Self {
            sources,
            exch_heap: BinaryHeap::new(),
            pending_exch: VecDeque::new(),
            local_heap: BinaryHeap::new(),
            chunk_size: config.chunk_size,
            reorder_window: config.reorder_window,
        };

        for source_idx in 0..reader.sources.len() {
            if let Some(pending) = reader.sources[source_idx].recv_next()? {
                reader.exch_heap.push(ExchHead {
                    source_idx,
                    pending,
                });
            }
        }

        reader.replenish()?;
        Ok(reader)
    }

    pub fn next_data(&mut self) -> Result<Option<Data<Event>>, String> {
        let mut chunk = Vec::with_capacity(self.chunk_size);

        while chunk.len() < self.chunk_size {
            self.replenish()?;
            let Some(next_event) = self.next_event()? else {
                break;
            };
            chunk.push(next_event);
        }

        if chunk.is_empty() {
            Ok(None)
        } else {
            Ok(Some(Data::from_data(&chunk)))
        }
    }

    pub fn collect_all(&mut self) -> Result<Vec<Event>, String> {
        let mut events = Vec::new();
        while let Some(chunk) = self.next_data()? {
            for i in 0..chunk.len() {
                events.push(chunk[i].clone());
            }
        }
        Ok(events)
    }

    fn replenish(&mut self) -> Result<(), String> {
        while self.pending_exch.len() < self.reorder_window {
            let Some(head) = self.exch_heap.pop() else {
                break;
            };

            let source_idx = head.source_idx;
            let pending = head.pending;
            self.local_heap.push(LocalHead::new(pending.clone()));
            self.pending_exch.push_back(pending);

            if let Some(next_pending) = self.sources[source_idx].recv_next()? {
                self.exch_heap.push(ExchHead {
                    source_idx,
                    pending: next_pending,
                });
            }
        }
        Ok(())
    }

    fn next_event(&mut self) -> Result<Option<Event>, String> {
        let next_exch = self.pending_exch.front().cloned();
        let next_local = self.local_heap.peek().map(|head| head.pending.clone());

        let next = match (next_exch, next_local) {
            (Some(exch), Some(local)) => {
                if same_event_pair(&exch.event, &local.event) {
                    let mut merged = exch.event.clone();
                    merged.ev |= EXCH_EVENT | LOCAL_EVENT;
                    self.pending_exch.pop_front();
                    self.local_heap.pop();
                    Some(merged)
                } else if exch_first(&exch.event, &local.event) {
                    let mut event = exch.event.clone();
                    event.ev |= EXCH_EVENT;
                    self.pending_exch.pop_front();
                    Some(event)
                } else {
                    let mut event = local.event.clone();
                    event.ev |= LOCAL_EVENT;
                    self.local_heap.pop();
                    Some(event)
                }
            }
            (Some(exch), None) => {
                let mut event = exch.event.clone();
                event.ev |= EXCH_EVENT;
                self.pending_exch.pop_front();
                Some(event)
            }
            (None, Some(local)) => {
                let mut event = local.event.clone();
                event.ev |= LOCAL_EVENT;
                self.local_heap.pop();
                Some(event)
            }
            (None, None) => None,
        };

        Ok(next)
    }
}

pub fn collect_tardis_stream(
    day: TardisStreamDayConfig,
    config: StreamingTardisReaderConfig,
) -> Result<Vec<Event>, String> {
    StreamingTardisReader::new(day, config)?.collect_all()
}

#[derive(Clone)]
struct PendingEvent {
    event: Event,
    source_priority: u8,
    source_seq: u64,
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct StableKey {
    timestamp: i64,
    source_priority: u8,
    source_seq: u64,
}

impl Ord for StableKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.timestamp
            .cmp(&other.timestamp)
            .then_with(|| self.source_priority.cmp(&other.source_priority))
            .then_with(|| self.source_seq.cmp(&other.source_seq))
    }
}

impl PartialOrd for StableKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct ExchHead {
    source_idx: usize,
    pending: PendingEvent,
}

impl ExchHead {
    fn key(&self) -> StableKey {
        StableKey {
            timestamp: self.pending.event.exch_ts,
            source_priority: self.pending.source_priority,
            source_seq: self.pending.source_seq,
        }
    }
}

impl Ord for ExchHead {
    fn cmp(&self, other: &Self) -> Ordering {
        other.key().cmp(&self.key())
    }
}

impl PartialOrd for ExchHead {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for ExchHead {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl Eq for ExchHead {}

struct LocalHead {
    pending: PendingEvent,
}

impl LocalHead {
    fn new(pending: PendingEvent) -> Self {
        Self { pending }
    }

    fn key(&self) -> StableKey {
        StableKey {
            timestamp: self.pending.event.local_ts,
            source_priority: self.pending.source_priority,
            source_seq: self.pending.source_seq,
        }
    }
}

impl Ord for LocalHead {
    fn cmp(&self, other: &Self) -> Ordering {
        other.key().cmp(&self.key())
    }
}

impl PartialOrd for LocalHead {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for LocalHead {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl Eq for LocalHead {}

#[derive(Clone, Copy)]
enum SourceKind {
    Trades,
    Depth,
    BookTicker,
}

struct SourceState {
    source_priority: u8,
    source_seq: u64,
    rx: Receiver<Result<Option<Vec<Event>>, String>>,
    batch: Vec<Event>,
    batch_index: usize,
    exhausted: bool,
}

impl SourceState {
    fn spawn(
        source_priority: u8,
        kind: SourceKind,
        path: String,
        config: TardisConvertConfig,
        local_timestamp_policy: StreamLocalTimestampPolicy,
    ) -> Self {
        let (tx, rx) = sync_channel(SOURCE_CHANNEL_CAPACITY);
        let _ = thread::spawn(move || {
            let result = match kind {
                SourceKind::Trades => stream_trades_file(&path, &tx, local_timestamp_policy),
                SourceKind::Depth => {
                    stream_depth_file(&path, &tx, &config, local_timestamp_policy)
                }
                SourceKind::BookTicker => {
                    stream_book_ticker_file(&path, &tx, local_timestamp_policy)
                }
            };

            if let Err(err) = result {
                let _ = send_message(&tx, Err(err));
            }
        });

        Self {
            source_priority,
            source_seq: 0,
            rx,
            batch: Vec::new(),
            batch_index: 0,
            exhausted: false,
        }
    }

    fn recv_next(&mut self) -> Result<Option<PendingEvent>, String> {
        loop {
            if self.batch_index < self.batch.len() {
                let pending = PendingEvent {
                    event: self.batch[self.batch_index].clone(),
                    source_priority: self.source_priority,
                    source_seq: self.source_seq,
                };
                self.batch_index += 1;
                self.source_seq += 1;
                return Ok(Some(pending));
            }

            if self.exhausted {
                return Ok(None);
            }

            match self.rx.recv() {
                Ok(Ok(Some(batch))) => {
                    self.batch = batch;
                    self.batch_index = 0;
                }
                Ok(Ok(None)) | Err(_) => {
                    self.exhausted = true;
                    return Ok(None);
                }
                Ok(Err(err)) => {
                    self.exhausted = true;
                    return Err(err);
                }
            }
        }
    }
}

fn stream_trades_file(
    path: &str,
    tx: &SyncSender<Result<Option<Vec<Event>>, String>>,
    local_timestamp_policy: StreamLocalTimestampPolicy,
) -> Result<(), String> {
    let mut reader = open_file_fast(path.as_ref()).map_err(io_error)?;
    let mut header = String::new();
    reader.read_line(&mut header).map_err(io_error)?;

    let mut line = String::with_capacity(256);
    let mut batch = Vec::with_capacity(SOURCE_BATCH_SIZE);
    while reader.read_line(&mut line).map_err(io_error)? > 0 {
        if let Some(mut event) = parse_trade_to_event(line.trim_end()) {
            normalize_local_timestamp(&mut event, local_timestamp_policy)?;
            push_event(&tx, &mut batch, event)?;
        }
        line.clear();
    }

    flush_batch(&tx, &mut batch)?;
    send_message(&tx, Ok(None))
}

fn stream_depth_file(
    path: &str,
    tx: &SyncSender<Result<Option<Vec<Event>>, String>>,
    config: &TardisConvertConfig,
    local_timestamp_policy: StreamLocalTimestampPolicy,
) -> Result<(), String> {
    let mut reader = open_file_fast(path.as_ref()).map_err(io_error)?;
    let mut header = String::new();
    reader.read_line(&mut header).map_err(io_error)?;

    let mut depth_converter = DepthConverterFast::new(config.ss_buffer_size, config.snapshot_mode);
    let mut line = String::with_capacity(256);
    let mut output = Vec::with_capacity(64);
    let mut batch = Vec::with_capacity(SOURCE_BATCH_SIZE);

    while reader.read_line(&mut line).map_err(io_error)? > 0 {
        if let Some((ts, local_ts, is_snap, is_bid, px, qty)) = parse_depth_line(line.trim_end()) {
            output.clear();
            depth_converter.process(ts, local_ts, is_snap, is_bid, px, qty, &mut output);
            for mut event in output.drain(..) {
                normalize_local_timestamp(&mut event, local_timestamp_policy)?;
                push_event(&tx, &mut batch, event)?;
            }
        }
        line.clear();
    }

    flush_batch(&tx, &mut batch)?;
    send_message(&tx, Ok(None))
}

fn stream_book_ticker_file(
    path: &str,
    tx: &SyncSender<Result<Option<Vec<Event>>, String>>,
    local_timestamp_policy: StreamLocalTimestampPolicy,
) -> Result<(), String> {
    let mut reader = open_file_fast(path.as_ref()).map_err(io_error)?;
    let mut header = String::new();
    reader.read_line(&mut header).map_err(io_error)?;
    let layout = BookTickerLayout::from_header(&header)
        .ok_or_else(|| format!("Invalid book_ticker header: {}", header.trim()))?;

    let mut line = String::with_capacity(256);
    let mut batch = Vec::with_capacity(SOURCE_BATCH_SIZE);
    while reader.read_line(&mut line).map_err(io_error)? > 0 {
        if let Some((mut bid_event, mut ask_event)) =
            parse_bookticker_to_events(line.trim_end(), &layout)
        {
            normalize_local_timestamp(&mut bid_event, local_timestamp_policy)?;
            normalize_local_timestamp(&mut ask_event, local_timestamp_policy)?;
            push_event(&tx, &mut batch, bid_event)?;
            push_event(&tx, &mut batch, ask_event)?;
        }
        line.clear();
    }

    flush_batch(&tx, &mut batch)?;
    send_message(&tx, Ok(None))
}

fn send_message(
    tx: &SyncSender<Result<Option<Vec<Event>>, String>>,
    message: Result<Option<Vec<Event>>, String>,
) -> Result<(), String> {
    tx.send(message)
        .map_err(|_| "streaming reader dropped before source finished".to_string())
}

fn push_event(
    tx: &SyncSender<Result<Option<Vec<Event>>, String>>,
    batch: &mut Vec<Event>,
    event: Event,
) -> Result<(), String> {
    batch.push(event);
    if batch.len() >= SOURCE_BATCH_SIZE {
        flush_batch(tx, batch)?;
    }
    Ok(())
}

fn flush_batch(
    tx: &SyncSender<Result<Option<Vec<Event>>, String>>,
    batch: &mut Vec<Event>,
) -> Result<(), String> {
    if batch.is_empty() {
        return Ok(());
    }
    let out = std::mem::replace(batch, Vec::with_capacity(SOURCE_BATCH_SIZE));
    send_message(tx, Ok(Some(out)))
}

fn normalize_local_timestamp(
    event: &mut Event,
    policy: StreamLocalTimestampPolicy,
) -> Result<(), String> {
    if event.local_ts < event.exch_ts {
        match policy {
            StreamLocalTimestampPolicy::Clamp => {
                event.local_ts = event.exch_ts;
            }
            StreamLocalTimestampPolicy::ErrorOnNegative => {
                return Err(format!(
                    "negative feed latency detected: local_ts={} exch_ts={}",
                    event.local_ts, event.exch_ts
                ));
            }
        }
    }
    Ok(())
}

fn same_event_pair(a: &Event, b: &Event) -> bool {
    a.exch_ts == b.exch_ts && a.local_ts == b.local_ts
}

fn exch_first(exch: &Event, local: &Event) -> bool {
    (exch.exch_ts == local.exch_ts && exch.local_ts < local.local_ts)
        || exch.exch_ts < local.exch_ts
}

fn io_error(err: std::io::Error) -> String {
    format!("streaming read failed: {err}")
}
