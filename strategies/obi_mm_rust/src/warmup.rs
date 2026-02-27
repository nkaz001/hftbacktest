//! Warmup module for pre-loading factor state from historical data.
//!
//! This module provides functionality to warm up the strategy's factors
//! by replaying historical market data before starting live trading.
//!
//! Supports two data formats:
//! - `.jsonl` files: Current IPC collector format (can read while being written)
//! - `.gz` files: Compressed JSON format from IPC collector (previous days)
//! - `.npz` files: Numpy format from data converters

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use flate2::read::GzDecoder;
use hftbacktest::{
    backtest::data::read_npz_file,
    depth::HashMapMarketDepth,
    prelude::{ApplySnapshot, L2MarketDepth, MarketDepth, BUY_EVENT, DEPTH_EVENT, DEPTH_SNAPSHOT_EVENT},
    types::Event,
};
use serde::Deserialize;
use tracing::{info, warn};

use crate::ObiMmStrategy;

/// Result of warmup attempt
#[derive(Debug)]
pub enum WarmupResult {
    /// Warmup succeeded with the given number of events processed
    Success {
        events_processed: usize,
        data_age_seconds: i64,
    },
    /// No suitable data files found
    NoDataFound,
    /// Data is too old (older than max_age_seconds)
    DataTooOld { age_seconds: i64 },
    /// Error loading data
    Error(String),
}

/// Configuration for warmup
#[derive(Debug, Clone)]
pub struct WarmupConfig {
    /// Directory containing npz files
    pub data_dir: PathBuf,
    /// Symbol name (lowercase, e.g., "btcusdt")
    pub symbol: String,
    /// Maximum age of data in seconds (default: 2 hours)
    pub max_age_seconds: i64,
    /// Minimum required data duration in seconds for warmup
    pub min_duration_seconds: i64,
}

impl WarmupConfig {
    pub fn new(data_dir: impl Into<PathBuf>, symbol: impl Into<String>) -> Self {
        Self {
            data_dir: data_dir.into(),
            symbol: symbol.into(),
            max_age_seconds: 2 * 3600, // 2 hours
            min_duration_seconds: 3600, // 1 hour minimum
        }
    }

    pub fn with_max_age(mut self, seconds: i64) -> Self {
        self.max_age_seconds = seconds;
        self
    }

    pub fn with_min_duration(mut self, seconds: i64) -> Self {
        self.min_duration_seconds = seconds;
        self
    }
}

// JSON structures for parsing IPC collector .gz files
#[derive(Deserialize)]
#[allow(non_snake_case, dead_code)]
struct DepthUpdateJson {
    e: String,           // "depthUpdate" or "depthSnapshot"
    E: i64,              // event time (ms)
    s: String,           // symbol
    b: Vec<[String; 2]>, // bids
    a: Vec<[String; 2]>, // asks
}

#[derive(Deserialize)]
#[allow(non_snake_case, dead_code)]
struct TradeJson {
    e: String,  // "trade"
    E: i64,     // event time (ms)
    s: String,  // symbol
    p: String,  // price
    q: String,  // quantity
    m: bool,    // is buyer maker
}

/// Find the most recent data file (npz or gz) for the given symbol
fn find_recent_data_file(config: &WarmupConfig) -> Option<PathBuf> {
    let dir = &config.data_dir;
    if !dir.exists() {
        warn!("Warmup data directory does not exist: {:?}", dir);
        return None;
    }

    let symbol_pattern = config.symbol.to_lowercase();

    // Check if directory name contains the symbol (e.g., data/btcusdt/)
    let dir_contains_symbol = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase().contains(&symbol_pattern))
        .unwrap_or(false);

    // Find all matching data files (npz, gz, or jsonl)
    // Prefer .jsonl files as they are the current file being written and can be read in real-time
    let mut files: Vec<(PathBuf, SystemTime)> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            let name_matches = name.contains(&symbol_pattern) || dir_contains_symbol;
            let ext_matches = name.ends_with(".npz") || name.ends_with(".gz") || name.ends_with(".jsonl");
            name_matches && ext_matches
        })
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            let modified = metadata.modified().ok()?;
            Some((entry.path(), modified))
        })
        .collect();

    // Sort by modification time (most recent first)
    files.sort_by(|a, b| b.1.cmp(&a.1));

    if let Some((path, _)) = files.first() {
        info!(
            "Found {} data files, using most recent: {:?}",
            files.len(),
            path
        );
    }

    files.first().map(|(path, _)| path.clone())
}

/// Check if the npz data is fresh enough
fn check_data_freshness(events: &[Event], max_age_seconds: i64) -> Result<i64, i64> {
    if events.is_empty() {
        return Err(i64::MAX);
    }

    // Get the last event's exchange timestamp
    let last_event_ts = events.last().unwrap().exch_ts;
    let last_event_secs = last_event_ts / 1_000_000_000;

    // Get current time
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let age_seconds = now - last_event_secs;

    if age_seconds > max_age_seconds {
        Err(age_seconds)
    } else {
        Ok(age_seconds)
    }
}

/// Get the data duration in seconds
fn get_data_duration(events: &[Event]) -> i64 {
    if events.len() < 2 {
        return 0;
    }

    let first_ts = events.first().unwrap().exch_ts;
    let last_ts = events.last().unwrap().exch_ts;

    (last_ts - first_ts) / 1_000_000_000
}

const NANOSECONDS_PER_SECOND: i64 = 1_000_000_000;
const JSONL_INITIAL_TAIL_BYTES: u64 = 64 * 1024 * 1024; // 64 MiB

/// Sliding in-memory window that only keeps the last configured duration of events.
#[derive(Debug)]
struct EventWindow {
    window_ns: i64,
    events: VecDeque<Event>,
    max_exch_ts: Option<i64>,
    last_seen_exch_ts: Option<i64>,
    out_of_order: bool,
}

impl EventWindow {
    fn new(min_duration_seconds: i64) -> Self {
        Self {
            window_ns: min_duration_seconds
                .max(0)
                .saturating_mul(NANOSECONDS_PER_SECOND),
            events: VecDeque::new(),
            max_exch_ts: None,
            last_seen_exch_ts: None,
            out_of_order: false,
        }
    }

    fn push(&mut self, event: Event) {
        if let Some(last_seen) = self.last_seen_exch_ts {
            if event.exch_ts < last_seen {
                self.out_of_order = true;
            }
        }
        self.last_seen_exch_ts = Some(event.exch_ts);
        self.max_exch_ts = Some(self.max_exch_ts.map_or(event.exch_ts, |ts| ts.max(event.exch_ts)));
        self.events.push_back(event);

        if self.window_ns <= 0 {
            return;
        }

        if let Some(max_ts) = self.max_exch_ts {
            let cutoff = max_ts.saturating_sub(self.window_ns);
            while let Some(front) = self.events.front() {
                if front.exch_ts < cutoff {
                    self.events.pop_front();
                } else {
                    break;
                }
            }
        }
    }

    fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    fn len(&self) -> usize {
        self.events.len()
    }

    fn duration_seconds(&self) -> i64 {
        let first_ts = self.events.front().map(|e| e.exch_ts);
        let last_ts = self.max_exch_ts;
        match (first_ts, last_ts) {
            (Some(first), Some(last)) if last >= first => (last - first) / NANOSECONDS_PER_SECOND,
            _ => 0,
        }
    }

    fn has_required_coverage(&self) -> bool {
        if self.window_ns <= 0 {
            return !self.events.is_empty();
        }

        let first_ts = self.events.front().map(|e| e.exch_ts);
        let last_ts = self.max_exch_ts;
        // Use 95% threshold to avoid off-by-one issues from integer division
        // (e.g., 1799.9s truncated to 1799 failing a strict >= 1800 check)
        let threshold = self.window_ns.saturating_mul(95) / 100;
        matches!(
            (first_ts, last_ts),
            (Some(first), Some(last)) if last.saturating_sub(first) >= threshold
        )
    }

    fn into_vec(mut self) -> Vec<Event> {
        let mut events: Vec<Event> = self.events.drain(..).collect();

        // Input is expected to be chronological. If we detect out-of-order data,
        // restore deterministic ordering and re-apply the time window once.
        if self.out_of_order {
            events.sort_by_key(|e| e.exch_ts);

            if self.window_ns > 0 {
                if let Some(last_ts) = events.last().map(|e| e.exch_ts) {
                    let cutoff = last_ts.saturating_sub(self.window_ns);
                    let keep_from = events
                        .iter()
                        .position(|e| e.exch_ts >= cutoff)
                        .unwrap_or(events.len());
                    if keep_from > 0 {
                        events.drain(0..keep_from);
                    }
                }
            }
        }

        events
    }
}

fn parse_collector_line_into_window(
    line: &str,
    window: &mut EventWindow,
    parse_errors: &mut usize,
) {
    let Some((local_ts_str, json_str)) = line.split_once(' ') else {
        return;
    };

    let local_ts: i64 = match local_ts_str.parse() {
        Ok(ts) => ts,
        Err(_) => return,
    };

    if let Ok(depth) = serde_json::from_str::<DepthUpdateJson>(json_str) {
        let exch_ts = depth.E * 1_000_000; // ms to ns
        let is_snapshot = depth.e == "depthSnapshot";
        let ev_type = if is_snapshot {
            DEPTH_SNAPSHOT_EVENT
        } else {
            DEPTH_EVENT
        };

        for bid in &depth.b {
            let px: f64 = bid[0].parse().unwrap_or(0.0);
            let qty: f64 = bid[1].parse().unwrap_or(0.0);
            if px > 0.0 {
                window.push(Event {
                    ev: ev_type | BUY_EVENT,
                    exch_ts,
                    local_ts,
                    px,
                    qty,
                    order_id: 0,
                    ival: 0,
                    fval: 0.0,
                });
            }
        }

        for ask in &depth.a {
            let px: f64 = ask[0].parse().unwrap_or(0.0);
            let qty: f64 = ask[1].parse().unwrap_or(0.0);
            if px > 0.0 {
                window.push(Event {
                    ev: ev_type, // No BUY_EVENT flag = sell side
                    exch_ts,
                    local_ts,
                    px,
                    qty,
                    order_id: 0,
                    ival: 0,
                    fval: 0.0,
                });
            }
        }
    } else if serde_json::from_str::<TradeJson>(json_str).is_ok() {
        // Skip trade events for warmup - we only need depth for OBI
    } else {
        *parse_errors += 1;
    }
}

fn parse_jsonl_bytes_into_window(
    bytes: &[u8],
    range_start: u64,
    window: &mut EventWindow,
    line_count: &mut usize,
    parse_errors: &mut usize,
) {
    let mut slice = bytes;

    // If this is not the beginning of file, the first bytes may be a partial line.
    if range_start > 0 {
        if let Some(pos) = slice.iter().position(|&b| b == b'\n') {
            slice = &slice[(pos + 1)..];
        } else {
            return;
        }
    }

    // The last line may be partially written; ignore trailing incomplete bytes.
    let Some(last_newline_pos) = slice.iter().rposition(|&b| b == b'\n') else {
        return;
    };
    slice = &slice[..last_newline_pos];

    for raw_line in slice.split(|&b| b == b'\n') {
        if raw_line.is_empty() {
            continue;
        }

        *line_count += 1;
        if *line_count % 10000 == 0 {
            info!("Processing line {}, events so far: {}", *line_count, window.len());
        }

        let line = String::from_utf8_lossy(raw_line);
        parse_collector_line_into_window(&line, window, parse_errors);
    }
}

/// Load events from npz file, filtering to only include recent data
fn load_warmup_events_npz(
    path: &Path,
    min_duration_seconds: i64,
) -> Result<Vec<Event>, String> {
    info!("Loading warmup data from npz: {:?}", path);
    warn!(
        "NPZ warmup loads the full file into memory before filtering; prefer jsonl/gz in production"
    );

    let data = read_npz_file::<Event>(path.to_str().unwrap(), "data")
        .map_err(|e| format!("Failed to read npz file: {}", e))?;

    if data.len() == 0 {
        return Err("Empty npz file".to_string());
    }

    // Get the time range
    let first_ts = data[0].exch_ts;
    let last_ts = data[data.len() - 1].exch_ts;
    let total_duration = (last_ts - first_ts) / 1_000_000_000;

    info!(
        "Loaded {} events, duration: {} seconds",
        data.len(),
        total_duration
    );

    // Only keep the last `min_duration_seconds` of data for warmup
    let warmup_start_ts = last_ts - (min_duration_seconds * 1_000_000_000);

    let events: Vec<Event> = (0..data.len())
        .filter(|&i| data[i].exch_ts >= warmup_start_ts)
        .map(|i| data[i].clone())
        .collect();

    info!(
        "Filtered to {} events for warmup ({} seconds of data)",
        events.len(),
        min_duration_seconds
    );

    Ok(events)
}

/// Load events from IPC collector .gz file (JSON format)
fn load_warmup_events_gz(
    path: &Path,
    min_duration_seconds: i64,
) -> Result<Vec<Event>, String> {
    info!("Loading warmup data from gz: {:?}", path);

    let file = std::fs::File::open(path).map_err(|e| format!("Failed to open gz file: {}", e))?;
    let decoder = GzDecoder::new(file);
    let reader = BufReader::new(decoder);

    let mut window = EventWindow::new(min_duration_seconds);
    let mut line_count = 0;
    let mut parse_errors = 0;

    for line_result in reader.lines() {
        let line = match line_result {
            Ok(l) => l,
            Err(e) => {
                // Handle truncated/corrupted gz files gracefully
                warn!("Error reading line {} (may be truncated gz file): {}", line_count + 1, e);
                break; // Stop reading on error - file may be incomplete
            }
        };
        line_count += 1;

        // Log progress every 10000 lines
        if line_count % 10000 == 0 {
            info!(
                "Processing line {}, retained events so far: {}",
                line_count,
                window.len()
            );
        }
        parse_collector_line_into_window(&line, &mut window, &mut parse_errors);
    }

    if window.is_empty() {
        return Err(format!(
            "No events parsed from gz file (lines: {}, errors: {})",
            line_count, parse_errors
        ));
    }

    let events = window.into_vec();
    let total_duration = get_data_duration(&events);

    info!(
        "Loaded {} retained events from {} lines, duration: {} seconds",
        events.len(),
        line_count,
        total_duration
    );

    info!(
        "Windowed to {} events for warmup ({} seconds of data)",
        events.len(),
        min_duration_seconds
    );

    Ok(events)
}

/// Load events from IPC collector .jsonl file (uncompressed JSON format)
/// This function can read files that are currently being written to.
fn load_warmup_events_jsonl(
    path: &Path,
    min_duration_seconds: i64,
) -> Result<Vec<Event>, String> {
    info!("Loading warmup data from jsonl: {:?}", path);

    let mut file =
        std::fs::File::open(path).map_err(|e| format!("Failed to open jsonl file: {}", e))?;
    let end_len = file
        .metadata()
        .map_err(|e| format!("Failed to stat jsonl file: {}", e))?
        .len();

    if end_len == 0 {
        return Err("Empty jsonl file".to_string());
    }

    let mut tail_bytes = JSONL_INITIAL_TAIL_BYTES.min(end_len);
    let mut pass = 0usize;

    loop {
        pass += 1;
        let range_start = end_len.saturating_sub(tail_bytes);
        let bytes_to_read = end_len.saturating_sub(range_start);

        if bytes_to_read > usize::MAX as u64 {
            return Err(format!(
                "jsonl range too large to map into memory: {} bytes",
                bytes_to_read
            ));
        }

        info!(
            "JSONL tail pass {}: scanning bytes [{}..{}] ({} bytes)",
            pass, range_start, end_len, bytes_to_read
        );

        file.seek(SeekFrom::Start(range_start))
            .map_err(|e| format!("Failed to seek jsonl file: {}", e))?;

        let mut buf = vec![0u8; bytes_to_read as usize];
        if let Err(e) = file.read_exact(&mut buf) {
            return Err(format!(
                "Failed to read jsonl range [{}..{}]: {}",
                range_start, end_len, e
            ));
        }

        let mut window = EventWindow::new(min_duration_seconds);
        let mut line_count = 0usize;
        let mut parse_errors = 0usize;

        parse_jsonl_bytes_into_window(
            &buf,
            range_start,
            &mut window,
            &mut line_count,
            &mut parse_errors,
        );

        if window.is_empty() {
            if range_start == 0 {
                return Err(format!(
                    "No events parsed from jsonl file (lines: {}, errors: {})",
                    line_count, parse_errors
                ));
            }

            let next_tail = (tail_bytes.saturating_mul(2)).min(end_len);
            if next_tail == tail_bytes {
                return Err(format!(
                    "No events parsed from jsonl file (lines: {}, errors: {})",
                    line_count, parse_errors
                ));
            }

            info!(
                "JSONL tail pass {} found no complete events; expanding scan {} -> {} bytes",
                pass, tail_bytes, next_tail
            );
            tail_bytes = next_tail;
            continue;
        }

        let duration_seconds = window.duration_seconds();
        info!(
            "JSONL tail pass {} parsed {} lines, retained {} events, duration: {} seconds, errors: {}",
            pass,
            line_count,
            window.len(),
            duration_seconds,
            parse_errors
        );

        if window.has_required_coverage() || range_start == 0 {
            let events = window.into_vec();
            let total_duration = get_data_duration(&events);

            info!(
                "Loaded {} retained events from {} lines, duration: {} seconds (tail passes: {})",
                events.len(),
                line_count,
                total_duration,
                pass
            );
            info!(
                "Windowed to {} events for warmup ({} seconds of data)",
                events.len(),
                min_duration_seconds
            );

            return Ok(events);
        }

        let next_tail = (tail_bytes.saturating_mul(2)).min(end_len);
        if next_tail == tail_bytes {
            let events = window.into_vec();
            let total_duration = get_data_duration(&events);

            info!(
                "Loaded {} retained events from {} lines, duration: {} seconds (tail passes: {})",
                events.len(),
                line_count,
                total_duration,
                pass
            );
            info!(
                "Windowed to {} events for warmup ({} seconds of data)",
                events.len(),
                min_duration_seconds
            );

            return Ok(events);
        }

        info!(
            "JSONL tail pass {} coverage too short ({}s < {}s); expanding scan {} -> {} bytes",
            pass, duration_seconds, min_duration_seconds, tail_bytes, next_tail
        );
        tail_bytes = next_tail;
    }
}

/// Load warmup events from npz, gz, or jsonl file
fn load_warmup_events(path: &Path, min_duration_seconds: i64) -> Result<Vec<Event>, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    match ext {
        "npz" => load_warmup_events_npz(path, min_duration_seconds),
        "gz" => load_warmup_events_gz(path, min_duration_seconds),
        "jsonl" => load_warmup_events_jsonl(path, min_duration_seconds),
        _ => Err(format!("Unsupported file format: {}", ext)),
    }
}

impl ObiMmStrategy {
    /// Attempt to warm up factors from historical data.
    ///
    /// This method loads historical data from data files and replays it
    /// to initialize the factor engines before live trading starts.
    ///
    /// Supports:
    /// - `.jsonl` files: Current IPC collector format (can read while being written)
    /// - `.gz` files: Compressed IPC collector JSON format
    /// - `.npz` files: Converted numpy format
    ///
    /// # Arguments
    ///
    /// * `config` - Warmup configuration
    ///
    /// # Returns
    ///
    /// `WarmupResult` indicating success or failure reason.
    pub fn warmup_from_npz(&mut self, config: &WarmupConfig) -> WarmupResult {
        // Find the most recent data file (npz or gz)
        let data_path = match find_recent_data_file(config) {
            Some(path) => path,
            None => {
                warn!("No data files found for symbol {}", config.symbol);
                return WarmupResult::NoDataFound;
            }
        };

        info!("Found warmup file: {:?}", data_path);

        // Load events
        let events = match load_warmup_events(&data_path, config.min_duration_seconds) {
            Ok(events) => events,
            Err(e) => {
                warn!("Failed to load warmup events: {}", e);
                return WarmupResult::Error(e);
            }
        };

        if events.is_empty() {
            return WarmupResult::NoDataFound;
        }

        // Check data freshness
        let data_age = match check_data_freshness(&events, config.max_age_seconds) {
            Ok(age) => age,
            Err(age) => {
                warn!(
                    "Warmup data is too old: {} seconds (max: {})",
                    age, config.max_age_seconds
                );
                return WarmupResult::DataTooOld { age_seconds: age };
            }
        };

        info!(
            "Data freshness check passed. Age: {} seconds",
            data_age
        );

        // Create a temporary depth for warmup
        let tick_size = self.tick_size;
        let lot_size = 0.001; // Default lot size
        let mut depth = HashMapMarketDepth::new(tick_size, lot_size);

        // Replay events to warm up factors
        let events_processed = self.replay_events_for_warmup(&events, &mut depth);

        info!(
            "Warmup complete. Processed {} events. is_ready={}",
            events_processed,
            self.is_ready()
        );

        WarmupResult::Success {
            events_processed,
            data_age_seconds: data_age,
        }
    }

    /// Replay events to warm up factors without generating trading signals.
    fn replay_events_for_warmup<MD>(&mut self, events: &[Event], depth: &mut MD) -> usize
    where
        MD: MarketDepth + L2MarketDepth + ApplySnapshot,
    {
        let mut events_processed = 0;
        let mut last_mid_price = 0.0;
        let mut last_update_period: i64 = -1;
        let mut last_change_period: i64 = -1;

        for event in events {
            // Apply depth updates
            let ev_type = event.ev & 0xFF;
            let is_buy = (event.ev & BUY_EVENT) != 0;

            if ev_type == DEPTH_EVENT || ev_type == DEPTH_SNAPSHOT_EVENT {
                if is_buy {
                    depth.update_bid_depth(event.px, event.qty, event.exch_ts);
                } else {
                    depth.update_ask_depth(event.px, event.qty, event.exch_ts);
                }
            }

            // Get current market state
            let best_bid = depth.best_bid();
            let best_ask = depth.best_ask();

            if best_bid <= 0.0 || best_ask <= 0.0 {
                continue;
            }

            let mid_price = (best_bid + best_ask) / 2.0;
            let best_bid_tick = depth.best_bid_tick();
            let best_ask_tick = depth.best_ask_tick();

            // Check update interval
            let current_period = event.exch_ts / self.params.update_interval_ns;
            if current_period <= last_update_period {
                continue;
            }
            last_update_period = current_period;

            // Update volatility at appropriate intervals
            let change_period = current_period / 6;
            if change_period > last_change_period {
                if last_mid_price > 0.0 {
                    self.minute_change = mid_price / last_mid_price - 1.0;
                }

                let volatility = self.volatility_factor.update(event.exch_ts, mid_price);
                let vol = if volatility == 0.0 {
                    self.stop_loss.volatility_mean()
                } else {
                    volatility
                };
                self.stop_loss.check(vol, self.minute_change);

                last_mid_price = mid_price;
                last_change_period = change_period;
            }

            // Update OBI factor
            self.obi_factor.update_with_accessor(
                best_bid_tick,
                best_ask_tick,
                mid_price,
                |tick| depth.bid_qty_at_tick(tick),
                |tick| depth.ask_qty_at_tick(tick),
            );

            events_processed += 1;
        }

        events_processed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_warmup_config() {
        let config = WarmupConfig::new("/data", "btcusdt")
            .with_max_age(3600)
            .with_min_duration(1800);

        assert_eq!(config.max_age_seconds, 3600);
        assert_eq!(config.min_duration_seconds, 1800);
        assert_eq!(config.symbol, "btcusdt");
    }
}
