//! Tardis.dev data converter.
//!
//! Converts Tardis.dev CSV data files into the Event format used by HftBacktest.

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use flate2::read::GzDecoder;

use crate::types::{
    Event, BUY_EVENT, SELL_EVENT, DEPTH_EVENT, DEPTH_CLEAR_EVENT, DEPTH_SNAPSHOT_EVENT,
    TRADE_EVENT,
};

#[cfg(feature = "backtest")]
use crate::backtest::data::write_npy;

use super::validation::{
    argsort_by_exch_ts, argsort_by_local_ts, correct_event_order, correct_local_timestamp,
    validate_event_order,
};

/// Snapshot processing mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotMode {
    /// Process all snapshots
    Process,
    /// Ignore Start-of-Day snapshots only
    IgnoreSod,
    /// Ignore all snapshots
    Ignore,
}

/// Configuration for Tardis conversion
#[derive(Debug, Clone)]
pub struct TardisConvertConfig {
    /// Buffer size for events (default: 100_000_000)
    pub buffer_size: usize,
    /// Buffer size for snapshot events (default: 1_000_000)
    pub ss_buffer_size: usize,
    /// Base latency to add (in nanoseconds, default: 0)
    pub base_latency: i64,
    /// Snapshot processing mode (default: Process)
    pub snapshot_mode: SnapshotMode,
}

impl Default for TardisConvertConfig {
    fn default() -> Self {
        Self {
            buffer_size: 100_000_000,
            ss_buffer_size: 1_000_000,
            base_latency: 0,
            snapshot_mode: SnapshotMode::Process,
        }
    }
}

/// Detected file type based on header
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileType {
    Trades,
    Depth,
    BookTicker,
}

/// Trade record from Tardis CSV
#[derive(Debug)]
struct TradeRecord {
    timestamp: i64,       // microseconds
    local_timestamp: i64, // microseconds
    side: String,
    price: f64,
    amount: f64,
}

/// Depth record from Tardis CSV
#[derive(Debug)]
struct DepthRecord {
    timestamp: i64,       // microseconds
    local_timestamp: i64, // microseconds
    is_snapshot: bool,
    side: String,
    price: f64,
    amount: f64,
}

/// Opens a file, handling .gz compression automatically
fn open_file(path: &Path) -> std::io::Result<Box<dyn BufRead>> {
    let file = File::open(path)?;
    if path.extension().map_or(false, |ext| ext == "gz") {
        Ok(Box::new(BufReader::new(GzDecoder::new(file))))
    } else {
        Ok(Box::new(BufReader::new(file)))
    }
}

/// Detects file type from header line
fn detect_file_type(header: &str) -> Option<FileType> {
    let fields: Vec<&str> = header.split(',').collect();

    // trades: exchange,symbol,timestamp,local_timestamp,id,side,price,amount
    if fields.len() >= 8 && fields[4] == "id" && fields[5] == "side" {
        return Some(FileType::Trades);
    }

    // depth: exchange,symbol,timestamp,local_timestamp,is_snapshot,side,price,amount
    if fields.len() >= 8 && fields[4] == "is_snapshot" && fields[5] == "side" {
        return Some(FileType::Depth);
    }

    // book_ticker: exchange,symbol,timestamp,local_timestamp,ask_amount,ask_price,bid_price,bid_amount
    if fields.len() >= 8 && fields[4] == "ask_amount" {
        return Some(FileType::BookTicker);
    }

    None
}

/// Parses a trade record from a CSV line
fn parse_trade_record(line: &str) -> Option<TradeRecord> {
    let fields: Vec<&str> = line.split(',').collect();
    if fields.len() < 8 {
        return None;
    }

    Some(TradeRecord {
        timestamp: fields[2].parse().ok()?,
        local_timestamp: fields[3].parse().ok()?,
        side: fields[5].to_string(),
        price: fields[6].parse().ok()?,
        amount: fields[7].parse().ok()?,
    })
}

/// Parses a depth record from a CSV line
fn parse_depth_record(line: &str) -> Option<DepthRecord> {
    let fields: Vec<&str> = line.split(',').collect();
    if fields.len() < 8 {
        return None;
    }

    Some(DepthRecord {
        timestamp: fields[2].parse().ok()?,
        local_timestamp: fields[3].parse().ok()?,
        is_snapshot: fields[4] == "true",
        side: fields[5].to_string(),
        price: fields[6].parse().ok()?,
        amount: fields[7].parse().ok()?,
    })
}

/// Converts trade record to Event
fn trade_to_event(record: &TradeRecord) -> Event {
    let side_flag = match record.side.as_str() {
        "buy" => BUY_EVENT,
        "sell" => SELL_EVENT,
        _ => 0,
    };

    Event {
        ev: TRADE_EVENT | side_flag,
        exch_ts: record.timestamp * 1000, // microseconds to nanoseconds
        local_ts: record.local_timestamp * 1000,
        px: record.price,
        qty: record.amount,
        order_id: 0,
        ival: 0,
        fval: 0.0,
    }
}

/// Internal state for depth conversion
struct DepthConverter {
    ss_bid: Vec<Event>,
    ss_ask: Vec<Event>,
    is_sod_snapshot: bool,
    is_snapshot: bool,
    snapshot_mode: SnapshotMode,
}

impl DepthConverter {
    fn new(ss_buffer_size: usize, snapshot_mode: SnapshotMode) -> Self {
        Self {
            ss_bid: Vec::with_capacity(ss_buffer_size),
            ss_ask: Vec::with_capacity(ss_buffer_size),
            is_sod_snapshot: true,
            is_snapshot: false,
            snapshot_mode,
        }
    }

    /// Processes a depth record and appends events to output
    fn process(&mut self, record: &DepthRecord, output: &mut Vec<Event>) {
        let side_flag = match record.side.as_str() {
            "bid" | "buy" => BUY_EVENT,
            "ask" | "sell" => SELL_EVENT,
            _ => 0,
        };
        let is_bid = side_flag == BUY_EVENT;

        if record.is_snapshot {
            // Check if we should skip this snapshot
            if self.snapshot_mode == SnapshotMode::Ignore {
                return;
            }
            if self.snapshot_mode == SnapshotMode::IgnoreSod && self.is_sod_snapshot {
                return;
            }

            // Start collecting snapshot
            if !self.is_snapshot {
                self.is_snapshot = true;
                self.ss_bid.clear();
                self.ss_ask.clear();
            }

            let event = Event {
                ev: DEPTH_SNAPSHOT_EVENT | side_flag,
                exch_ts: record.timestamp * 1000,
                local_ts: record.local_timestamp * 1000,
                px: record.price,
                qty: record.amount,
                order_id: 0,
                ival: 0,
                fval: 0.0,
            };

            if is_bid {
                self.ss_bid.push(event);
            } else {
                self.ss_ask.push(event);
            }
        } else {
            // Regular depth update
            self.is_sod_snapshot = false;

            if self.is_snapshot {
                // End of snapshot - flush accumulated snapshot events
                self.is_snapshot = false;
                self.flush_snapshot(output);
            }

            // Add regular depth event
            let event = Event {
                ev: DEPTH_EVENT | side_flag,
                exch_ts: record.timestamp * 1000,
                local_ts: record.local_timestamp * 1000,
                px: record.price,
                qty: record.amount,
                order_id: 0,
                ival: 0,
                fval: 0.0,
            };
            output.push(event);
        }
    }

    /// Flushes accumulated snapshot events with DEPTH_CLEAR_EVENT prefix
    fn flush_snapshot(&mut self, output: &mut Vec<Event>) {
        // Flush bid snapshot
        if !self.ss_bid.is_empty() {
            // Find the lowest bid price for clear range
            let min_px = self.ss_bid.iter().map(|e| e.px).fold(f64::INFINITY, f64::min);

            // Add DEPTH_CLEAR_EVENT for bid side
            let clear_event = Event {
                ev: DEPTH_CLEAR_EVENT | BUY_EVENT,
                exch_ts: self.ss_bid[0].exch_ts,
                local_ts: self.ss_bid[0].local_ts,
                px: min_px,
                qty: 0.0,
                order_id: 0,
                ival: 0,
                fval: 0.0,
            };
            output.push(clear_event);

            // Add all bid snapshot events
            output.extend(self.ss_bid.drain(..));
        }

        // Flush ask snapshot
        if !self.ss_ask.is_empty() {
            // Find the highest ask price for clear range
            let max_px = self.ss_ask.iter().map(|e| e.px).fold(f64::NEG_INFINITY, f64::max);

            // Add DEPTH_CLEAR_EVENT for ask side
            let clear_event = Event {
                ev: DEPTH_CLEAR_EVENT | SELL_EVENT,
                exch_ts: self.ss_ask[0].exch_ts,
                local_ts: self.ss_ask[0].local_ts,
                px: max_px,
                qty: 0.0,
                order_id: 0,
                ival: 0,
                fval: 0.0,
            };
            output.push(clear_event);

            // Add all ask snapshot events
            output.extend(self.ss_ask.drain(..));
        }
    }
}

/// Converts Tardis.dev data files into HftBacktest Event format.
///
/// For Tardis's Binance Futures feed data, they use the 'E' event timestamp,
/// representing the sending time, rather than the 'T' transaction time,
/// indicating when the matching occurs. So the latency is slightly less than
/// it actually is.
///
/// # Arguments
/// * `input_files` - Input filenames for both incremental book and trades files.
///                   Trade files should be input before depth files.
/// * `config` - Conversion configuration
///
/// # Returns
/// Converted data compatible with HftBacktest.
pub fn convert_tardis<P: AsRef<Path>>(
    input_files: &[P],
    config: &TardisConvertConfig,
) -> Result<Vec<Event>, String> {
    let mut events: Vec<Event> = Vec::with_capacity(config.buffer_size);
    let mut depth_converter = DepthConverter::new(config.ss_buffer_size, config.snapshot_mode);

    for file_path in input_files {
        let path = file_path.as_ref();
        println!("Reading {:?}", path);

        let mut reader = open_file(path).map_err(|e| format!("Failed to open file: {}", e))?;

        // Read header
        let mut header = String::new();
        reader.read_line(&mut header).map_err(|e| format!("Failed to read header: {}", e))?;
        let header = header.trim();

        let file_type = detect_file_type(header)
            .ok_or_else(|| format!("Unknown file type for {:?}", path))?;

        // Process lines
        for line_result in reader.lines() {
            let line = line_result.map_err(|e| format!("Failed to read line: {}", e))?;
            if line.is_empty() {
                continue;
            }

            match file_type {
                FileType::Trades => {
                    if let Some(record) = parse_trade_record(&line) {
                        events.push(trade_to_event(&record));
                    }
                }
                FileType::Depth => {
                    if let Some(record) = parse_depth_record(&line) {
                        depth_converter.process(&record, &mut events);
                    }
                }
                FileType::BookTicker => {
                    return Err("Use convert_fuse for book_ticker data".to_string());
                }
            }
        }
    }

    if events.is_empty() {
        return Ok(events);
    }

    println!("Correcting the latency");
    let min_latency = correct_local_timestamp(&mut events, config.base_latency);
    if min_latency < 0 {
        println!("local_timestamp is ahead of exch_timestamp by {}", -min_latency);
    }

    println!("Correcting the event order ({} raw events)", events.len());
    let sorted_exch_indices = argsort_by_exch_ts(&events);
    let sorted_local_indices = argsort_by_local_ts(&events);
    let corrected = correct_event_order(&events, &sorted_exch_indices, &sorted_local_indices);

    // Explicitly free source data to reduce peak memory before validation/write
    drop(events);
    drop(sorted_exch_indices);
    drop(sorted_local_indices);

    println!("Validating event order");
    validate_event_order(&corrected)?;

    println!("Conversion complete: {} events", corrected.len());
    Ok(corrected)
}

/// Convenience function with default configuration
pub fn convert<P: AsRef<Path>>(input_files: &[P]) -> Result<Vec<Event>, String> {
    convert_tardis(input_files, &TardisConvertConfig::default())
}

/// Writes events to an NPZ file (compressed numpy archive).
///
/// The events are stored under the key "data" in the archive,
/// matching the Python convention used by `np.savez_compressed(filename, data=events)`.
#[cfg(feature = "backtest")]
pub fn write_npz_file<P: AsRef<Path>>(events: &[Event], output_path: P) -> Result<(), String> {
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    let path = output_path.as_ref();
    println!("Saving to {:?}", path);

    // Create ZIP archive and write NPY data directly to it (no intermediate buffer)
    let file = File::create(path)
        .map_err(|e| format!("Failed to create output file: {}", e))?;

    let mut zip = ZipWriter::new(file);

    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(6))
        .large_file(true);

    zip.start_file("data.npy", options)
        .map_err(|e| format!("Failed to start ZIP entry: {}", e))?;

    // Write NPY header + data directly to zip writer (avoids ~N*64 bytes buffer)
    write_npy(&mut zip, events)
        .map_err(|e| format!("Failed to write NPY data: {}", e))?;

    zip.finish()
        .map_err(|e| format!("Failed to finish ZIP: {}", e))?;

    println!("Saved {} events to {:?}", events.len(), path);
    Ok(())
}

/// Converts Tardis files and saves the result to an NPZ file.
///
/// This is a convenience function that combines `convert_tardis` and `write_npz_file`.
#[cfg(feature = "backtest")]
pub fn convert_and_save<P: AsRef<Path>>(
    input_files: &[P],
    output_path: P,
    config: &TardisConvertConfig,
) -> Result<Vec<Event>, String> {
    let events = convert_tardis(input_files, config)?;
    write_npz_file(&events, output_path)?;
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_file_type() {
        assert_eq!(
            detect_file_type("exchange,symbol,timestamp,local_timestamp,id,side,price,amount"),
            Some(FileType::Trades)
        );
        assert_eq!(
            detect_file_type("exchange,symbol,timestamp,local_timestamp,is_snapshot,side,price,amount"),
            Some(FileType::Depth)
        );
        assert_eq!(
            detect_file_type("exchange,symbol,timestamp,local_timestamp,ask_amount,ask_price,bid_price,bid_amount"),
            Some(FileType::BookTicker)
        );
        assert_eq!(
            detect_file_type("unknown,header"),
            None
        );
    }

    #[test]
    fn test_parse_trade_record() {
        let line = "binance-futures,BTCUSDT,1234567890123,1234567890456,trade123,buy,50000.5,0.001";
        let record = parse_trade_record(line).unwrap();
        assert_eq!(record.timestamp, 1234567890123);
        assert_eq!(record.local_timestamp, 1234567890456);
        assert_eq!(record.side, "buy");
        assert_eq!(record.price, 50000.5);
        assert_eq!(record.amount, 0.001);
    }

    #[test]
    fn test_parse_depth_record() {
        let line = "binance-futures,BTCUSDT,1234567890123,1234567890456,false,bid,50000.0,1.5";
        let record = parse_depth_record(line).unwrap();
        assert_eq!(record.timestamp, 1234567890123);
        assert_eq!(record.local_timestamp, 1234567890456);
        assert!(!record.is_snapshot);
        assert_eq!(record.side, "bid");
        assert_eq!(record.price, 50000.0);
        assert_eq!(record.amount, 1.5);
    }

    #[test]
    fn test_trade_to_event() {
        let record = TradeRecord {
            timestamp: 1000000, // microseconds
            local_timestamp: 1000100,
            side: "buy".to_string(),
            price: 50000.0,
            amount: 0.1,
        };

        let event = trade_to_event(&record);
        assert_eq!(event.ev, TRADE_EVENT | BUY_EVENT);
        assert_eq!(event.exch_ts, 1000000 * 1000); // nanoseconds
        assert_eq!(event.local_ts, 1000100 * 1000);
        assert_eq!(event.px, 50000.0);
        assert_eq!(event.qty, 0.1);
    }

    #[test]
    fn test_depth_converter_regular_update() {
        let mut converter = DepthConverter::new(1000, SnapshotMode::Process);
        let mut output = Vec::new();

        let record = DepthRecord {
            timestamp: 1000000,
            local_timestamp: 1000100,
            is_snapshot: false,
            side: "bid".to_string(),
            price: 50000.0,
            amount: 1.5,
        };

        converter.process(&record, &mut output);

        assert_eq!(output.len(), 1);
        assert_eq!(output[0].ev, DEPTH_EVENT | BUY_EVENT);
        assert_eq!(output[0].px, 50000.0);
        assert_eq!(output[0].qty, 1.5);
    }

    #[test]
    fn test_depth_converter_snapshot() {
        let mut converter = DepthConverter::new(1000, SnapshotMode::Process);
        let mut output = Vec::new();

        // Add snapshot events
        converter.process(&DepthRecord {
            timestamp: 1000000,
            local_timestamp: 1000100,
            is_snapshot: true,
            side: "bid".to_string(),
            price: 50000.0,
            amount: 1.0,
        }, &mut output);

        converter.process(&DepthRecord {
            timestamp: 1000000,
            local_timestamp: 1000100,
            is_snapshot: true,
            side: "bid".to_string(),
            price: 49999.0,
            amount: 2.0,
        }, &mut output);

        // No output yet (still collecting snapshot)
        assert_eq!(output.len(), 0);

        // End snapshot with regular update
        converter.process(&DepthRecord {
            timestamp: 1000001,
            local_timestamp: 1000101,
            is_snapshot: false,
            side: "bid".to_string(),
            price: 50001.0,
            amount: 0.5,
        }, &mut output);

        // Should have: CLEAR + 2 snapshots + 1 regular = 4 events
        assert_eq!(output.len(), 4);

        // First should be DEPTH_CLEAR_EVENT
        assert_eq!(output[0].ev, DEPTH_CLEAR_EVENT | BUY_EVENT);
        assert_eq!(output[0].px, 49999.0); // min bid price

        // Then snapshot events
        assert_eq!(output[1].ev, DEPTH_SNAPSHOT_EVENT | BUY_EVENT);
        assert_eq!(output[2].ev, DEPTH_SNAPSHOT_EVENT | BUY_EVENT);

        // Finally regular event
        assert_eq!(output[3].ev, DEPTH_EVENT | BUY_EVENT);
    }

    #[test]
    fn test_depth_converter_ignore_snapshot() {
        let mut converter = DepthConverter::new(1000, SnapshotMode::Ignore);
        let mut output = Vec::new();

        // Add snapshot events (should be ignored)
        converter.process(&DepthRecord {
            timestamp: 1000000,
            local_timestamp: 1000100,
            is_snapshot: true,
            side: "bid".to_string(),
            price: 50000.0,
            amount: 1.0,
        }, &mut output);

        // Add regular event
        converter.process(&DepthRecord {
            timestamp: 1000001,
            local_timestamp: 1000101,
            is_snapshot: false,
            side: "bid".to_string(),
            price: 50001.0,
            amount: 0.5,
        }, &mut output);

        // Should only have the regular event
        assert_eq!(output.len(), 1);
        assert_eq!(output[0].ev, DEPTH_EVENT | BUY_EVENT);
    }
}
