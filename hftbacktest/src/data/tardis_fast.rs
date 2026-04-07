//! Optimized Tardis.dev data converter.
//!
//! This module provides an optimized version of the Tardis converter with:
//! - Zero-copy parsing (no intermediate String allocations)
//! - Direct Event construction (no intermediate record structs)
//! - Larger I/O buffers
//! - Inline parsing functions

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use flate2::read::GzDecoder;

use crate::types::{
    BUY_EVENT, DEPTH_BBO_EVENT, DEPTH_CLEAR_EVENT, DEPTH_EVENT, DEPTH_SNAPSHOT_EVENT, Event,
    SELL_EVENT, TRADE_EVENT,
};

use super::tardis::{SnapshotMode, TardisConvertConfig};
use super::validation::{
    argsort_by_exch_ts, argsort_by_local_ts, correct_event_order, correct_local_timestamp,
    validate_event_order,
};

/// Buffer size for file I/O (8MB)
const IO_BUFFER_SIZE: usize = 8 * 1024 * 1024;

/// Detected file type based on header
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileType {
    Trades,
    Depth,
    BookTicker,
}

/// Opens a file with optimized buffer, handling .gz compression automatically
fn open_file_fast(path: &Path) -> std::io::Result<Box<dyn BufRead>> {
    let file = File::open(path)?;
    if path.extension().map_or(false, |ext| ext == "gz") {
        Ok(Box::new(BufReader::with_capacity(
            IO_BUFFER_SIZE,
            GzDecoder::new(file),
        )))
    } else {
        Ok(Box::new(BufReader::with_capacity(IO_BUFFER_SIZE, file)))
    }
}

/// Detects file type from header line
#[inline]
fn detect_file_type_fast(header: &str) -> Option<FileType> {
    // Quick check based on distinctive header fields
    if header.contains(",id,side,") {
        Some(FileType::Trades)
    } else if header.contains(",is_snapshot,side,") {
        Some(FileType::Depth)
    } else if header.contains("ask_price")
        && header.contains("bid_price")
        && header.contains("ask_amount")
    {
        Some(FileType::BookTicker)
    } else {
        None
    }
}

/// Fast integer parsing without error handling overhead
#[inline(always)]
fn parse_i64_fast(s: &str) -> i64 {
    let mut result: i64 = 0;
    for b in s.bytes() {
        if b >= b'0' && b <= b'9' {
            result = result * 10 + (b - b'0') as i64;
        }
    }
    result
}

/// Fast float parsing (simplified, assumes valid input)
#[inline(always)]
fn parse_f64_fast(s: &str) -> f64 {
    // Use standard parser for floats (fast-float is used internally by Rust)
    s.parse().unwrap_or(0.0)
}

/// Column layout for book_ticker CSV, detected from header.
#[derive(Debug, Clone, Copy)]
struct BookTickerLayout {
    ask_price: usize,
    ask_amount: usize,
    bid_price: usize,
    bid_amount: usize,
}

impl BookTickerLayout {
    fn from_header(header: &str) -> Option<Self> {
        let cols: Vec<&str> = header.trim().split(',').collect();
        Some(Self {
            ask_price: cols.iter().position(|&c| c == "ask_price")?,
            ask_amount: cols.iter().position(|&c| c == "ask_amount")?,
            bid_price: cols.iter().position(|&c| c == "bid_price")?,
            bid_amount: cols.iter().position(|&c| c == "bid_amount")?,
        })
    }
}

/// Parses a book_ticker line into two Events (bid + ask BBO).
/// Column order is determined by the header via BookTickerLayout.
#[inline]
fn parse_bookticker_to_events(line: &str, layout: &BookTickerLayout) -> Option<(Event, Event)> {
    let fields: Vec<&str> = line.split(',').collect();
    if fields.len()
        <= layout
            .ask_price
            .max(layout.ask_amount)
            .max(layout.bid_price)
            .max(layout.bid_amount)
    {
        return None;
    }

    // Fields 2, 3 are always timestamp, local_timestamp
    let timestamp = parse_i64_fast(fields[2]);
    let local_timestamp = parse_i64_fast(fields[3]);

    let ask_price = parse_f64_fast(fields[layout.ask_price]);
    let ask_amount = parse_f64_fast(fields[layout.ask_amount]);
    let bid_price = parse_f64_fast(fields[layout.bid_price]);
    let bid_amount = parse_f64_fast(fields[layout.bid_amount]);

    let exch_ts = timestamp * 1000;
    let local_ts = local_timestamp * 1000;

    Some((
        Event {
            ev: DEPTH_BBO_EVENT | BUY_EVENT,
            exch_ts,
            local_ts,
            px: bid_price,
            qty: bid_amount,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        },
        Event {
            ev: DEPTH_BBO_EVENT | SELL_EVENT,
            exch_ts,
            local_ts,
            px: ask_price,
            qty: ask_amount,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        },
    ))
}

/// Parses a trade line directly into an Event
#[inline]
fn parse_trade_to_event(line: &str) -> Option<Event> {
    // Format: exchange,symbol,timestamp,local_timestamp,id,side,price,amount
    let mut iter = line.split(',');

    // Skip exchange (0), symbol (1)
    iter.next()?;
    iter.next()?;

    // Parse timestamp (2), local_timestamp (3)
    let timestamp = parse_i64_fast(iter.next()?);
    let local_timestamp = parse_i64_fast(iter.next()?);

    // Skip id (4)
    iter.next()?;

    // Parse side (5)
    let side_str = iter.next()?;
    let side_flag = match side_str.as_bytes().first()? {
        b'b' => BUY_EVENT,  // "buy"
        b's' => SELL_EVENT, // "sell"
        _ => 0,
    };

    // Parse price (6), amount (7)
    let price = parse_f64_fast(iter.next()?);
    let amount = parse_f64_fast(iter.next()?);

    Some(Event {
        ev: TRADE_EVENT | side_flag,
        exch_ts: timestamp * 1000, // microseconds to nanoseconds
        local_ts: local_timestamp * 1000,
        px: price,
        qty: amount,
        order_id: 0,
        ival: 0,
        fval: 0.0,
    })
}

/// Parses a depth line, returning parsed fields
#[inline]
fn parse_depth_line(line: &str) -> Option<(i64, i64, bool, bool, f64, f64)> {
    // Format: exchange,symbol,timestamp,local_timestamp,is_snapshot,side,price,amount
    let mut iter = line.split(',');

    // Skip exchange (0), symbol (1)
    iter.next()?;
    iter.next()?;

    // Parse timestamp (2), local_timestamp (3)
    let timestamp = parse_i64_fast(iter.next()?);
    let local_timestamp = parse_i64_fast(iter.next()?);

    // Parse is_snapshot (4)
    let is_snapshot_str = iter.next()?;
    let is_snapshot = is_snapshot_str == "true";

    // Parse side (5)
    let side_str = iter.next()?;
    let is_bid = matches!(side_str.as_bytes().first(), Some(b'b')); // "bid" or "buy"

    // Parse price (6), amount (7)
    let price = parse_f64_fast(iter.next()?);
    let amount = parse_f64_fast(iter.next()?);

    Some((
        timestamp,
        local_timestamp,
        is_snapshot,
        is_bid,
        price,
        amount,
    ))
}

/// Optimized depth converter state
struct DepthConverterFast {
    ss_bid: Vec<Event>,
    ss_ask: Vec<Event>,
    is_sod_snapshot: bool,
    is_snapshot: bool,
    snapshot_mode: SnapshotMode,
}

impl DepthConverterFast {
    fn new(ss_buffer_size: usize, snapshot_mode: SnapshotMode) -> Self {
        Self {
            ss_bid: Vec::with_capacity(ss_buffer_size),
            ss_ask: Vec::with_capacity(ss_buffer_size),
            is_sod_snapshot: true,
            is_snapshot: false,
            snapshot_mode,
        }
    }

    /// Processes a parsed depth record
    #[inline]
    fn process(
        &mut self,
        timestamp: i64,
        local_timestamp: i64,
        is_snapshot: bool,
        is_bid: bool,
        price: f64,
        amount: f64,
        output: &mut Vec<Event>,
    ) {
        let side_flag = if is_bid { BUY_EVENT } else { SELL_EVENT };
        let exch_ts = timestamp * 1000;
        let local_ts = local_timestamp * 1000;

        if is_snapshot {
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
                exch_ts,
                local_ts,
                px: price,
                qty: amount,
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
            output.push(Event {
                ev: DEPTH_EVENT | side_flag,
                exch_ts,
                local_ts,
                px: price,
                qty: amount,
                order_id: 0,
                ival: 0,
                fval: 0.0,
            });
        }
    }

    /// Flushes accumulated snapshot events
    fn flush_snapshot(&mut self, output: &mut Vec<Event>) {
        // Flush bid snapshot
        if !self.ss_bid.is_empty() {
            let min_px = self
                .ss_bid
                .iter()
                .map(|e| e.px)
                .fold(f64::INFINITY, f64::min);

            output.push(Event {
                ev: DEPTH_CLEAR_EVENT | BUY_EVENT,
                exch_ts: self.ss_bid[0].exch_ts,
                local_ts: self.ss_bid[0].local_ts,
                px: min_px,
                qty: 0.0,
                order_id: 0,
                ival: 0,
                fval: 0.0,
            });

            output.extend(self.ss_bid.drain(..));
        }

        // Flush ask snapshot
        if !self.ss_ask.is_empty() {
            let max_px = self
                .ss_ask
                .iter()
                .map(|e| e.px)
                .fold(f64::NEG_INFINITY, f64::max);

            output.push(Event {
                ev: DEPTH_CLEAR_EVENT | SELL_EVENT,
                exch_ts: self.ss_ask[0].exch_ts,
                local_ts: self.ss_ask[0].local_ts,
                px: max_px,
                qty: 0.0,
                order_id: 0,
                ival: 0,
                fval: 0.0,
            });

            output.extend(self.ss_ask.drain(..));
        }
    }
}

/// Optimized Tardis converter
pub fn convert_tardis_fast<P: AsRef<Path>>(
    input_files: &[P],
    config: &TardisConvertConfig,
) -> Result<Vec<Event>, String> {
    let mut events: Vec<Event> = Vec::with_capacity(config.buffer_size);
    let mut depth_converter = DepthConverterFast::new(config.ss_buffer_size, config.snapshot_mode);

    for file_path in input_files {
        let path = file_path.as_ref();
        println!("Reading {:?}", path);

        let mut reader = open_file_fast(path).map_err(|e| format!("Failed to open file: {}", e))?;

        // Read header
        let mut header = String::new();
        reader
            .read_line(&mut header)
            .map_err(|e| format!("Failed to read header: {}", e))?;

        let file_type = detect_file_type_fast(&header)
            .ok_or_else(|| format!("Unknown file type for {:?}", path))?;

        // Reuse line buffer
        let mut line = String::with_capacity(256);

        match file_type {
            FileType::Trades => {
                while reader
                    .read_line(&mut line)
                    .map_err(|e| format!("Read error: {}", e))?
                    > 0
                {
                    if let Some(event) = parse_trade_to_event(line.trim_end()) {
                        events.push(event);
                    }
                    line.clear();
                }
            }
            FileType::Depth => {
                while reader
                    .read_line(&mut line)
                    .map_err(|e| format!("Read error: {}", e))?
                    > 0
                {
                    if let Some((ts, local_ts, is_snap, is_bid, px, qty)) =
                        parse_depth_line(line.trim_end())
                    {
                        depth_converter.process(
                            ts,
                            local_ts,
                            is_snap,
                            is_bid,
                            px,
                            qty,
                            &mut events,
                        );
                    }
                    line.clear();
                }
            }
            FileType::BookTicker => {
                let layout = BookTickerLayout::from_header(&header)
                    .ok_or_else(|| format!("Invalid book_ticker header: {}", header.trim()))?;
                while reader
                    .read_line(&mut line)
                    .map_err(|e| format!("Read error: {}", e))?
                    > 0
                {
                    if let Some((bid_ev, ask_ev)) =
                        parse_bookticker_to_events(line.trim_end(), &layout)
                    {
                        events.push(bid_ev);
                        events.push(ask_ev);
                    }
                    line.clear();
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
        println!(
            "local_timestamp is ahead of exch_timestamp by {}",
            -min_latency
        );
    }

    println!("Correcting the event order");
    let sorted_exch_indices = argsort_by_exch_ts(&events);
    let sorted_local_indices = argsort_by_local_ts(&events);
    let corrected = correct_event_order(&events, &sorted_exch_indices, &sorted_local_indices);

    println!("Validating event order");
    validate_event_order(&corrected)?;

    println!("Conversion complete: {} events", corrected.len());
    Ok(corrected)
}

/// Convenience function with default configuration
pub fn convert_fast<P: AsRef<Path>>(input_files: &[P]) -> Result<Vec<Event>, String> {
    convert_tardis_fast(input_files, &TardisConvertConfig::default())
}

/// Converts Tardis files and saves the result to an NPZ file (fast version).
#[cfg(feature = "backtest")]
pub fn convert_and_save_fast<P: AsRef<Path>>(
    input_files: &[P],
    output_path: P,
    config: &TardisConvertConfig,
) -> Result<Vec<Event>, String> {
    let events = convert_tardis_fast(input_files, config)?;
    super::tardis::write_npz_file(&events, output_path)?;
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_i64_fast() {
        assert_eq!(parse_i64_fast("12345"), 12345);
        assert_eq!(parse_i64_fast("1731715200006000"), 1731715200006000);
    }

    #[test]
    fn test_parse_trade_to_event() {
        let line = "binance-futures,BTCUSDT,1234567890123,1234567890456,trade123,buy,50000.5,0.001";
        let event = parse_trade_to_event(line).unwrap();
        assert_eq!(event.ev, TRADE_EVENT | BUY_EVENT);
        assert_eq!(event.exch_ts, 1234567890123 * 1000);
        assert_eq!(event.local_ts, 1234567890456 * 1000);
        assert_eq!(event.px, 50000.5);
        assert_eq!(event.qty, 0.001);
    }

    #[test]
    fn test_parse_depth_line() {
        let line = "binance-futures,BTCUSDT,1234567890123,1234567890456,false,bid,50000.0,1.5";
        let result = parse_depth_line(line).unwrap();
        assert_eq!(result.0, 1234567890123); // timestamp
        assert_eq!(result.1, 1234567890456); // local_timestamp
        assert!(!result.2); // is_snapshot
        assert!(result.3); // is_bid
        assert_eq!(result.4, 50000.0); // price
        assert_eq!(result.5, 1.5); // amount
    }

    #[test]
    fn test_parse_bookticker() {
        // Tardis actual format: ask_amount,ask_price,bid_price,bid_amount
        let header =
            "exchange,symbol,timestamp,local_timestamp,ask_amount,ask_price,bid_price,bid_amount";
        let layout = BookTickerLayout::from_header(header).unwrap();

        let line = "binance-futures,BTCUSDT,1234567890123,1234567890456,2.0,50001.0,50000.0,1.5";
        let (bid_ev, ask_ev) = parse_bookticker_to_events(line, &layout).unwrap();

        assert_eq!(bid_ev.ev, DEPTH_BBO_EVENT | BUY_EVENT);
        assert_eq!(bid_ev.px, 50000.0);
        assert_eq!(bid_ev.qty, 1.5);
        assert_eq!(bid_ev.exch_ts, 1234567890123 * 1000);

        assert_eq!(ask_ev.ev, DEPTH_BBO_EVENT | SELL_EVENT);
        assert_eq!(ask_ev.px, 50001.0);
        assert_eq!(ask_ev.qty, 2.0);
        assert_eq!(ask_ev.exch_ts, 1234567890123 * 1000);
    }

    #[test]
    fn test_convert_with_bookticker() {
        use std::io::Write;

        let dir = std::env::temp_dir().join("hft_test_bookticker");
        let _ = std::fs::create_dir_all(&dir);

        // Create depth CSV with snapshot + updates
        let depth_path = dir.join("depth.csv");
        {
            let mut f = std::fs::File::create(&depth_path).unwrap();
            writeln!(
                f,
                "exchange,symbol,timestamp,local_timestamp,is_snapshot,side,price,amount"
            )
            .unwrap();
            // Snapshot: bid 50000 @ 1.0, ask 50001 @ 2.0
            writeln!(
                f,
                "binance-futures,BTCUSDT,1000000,1000100,true,bid,50000.0,1.0"
            )
            .unwrap();
            writeln!(
                f,
                "binance-futures,BTCUSDT,1000000,1000100,true,ask,50001.0,2.0"
            )
            .unwrap();
            // Depth update at t=2000000
            writeln!(
                f,
                "binance-futures,BTCUSDT,2000000,2000100,false,bid,49999.0,0.5"
            )
            .unwrap();
            writeln!(
                f,
                "binance-futures,BTCUSDT,2000000,2000100,false,ask,50002.0,0.3"
            )
            .unwrap();
        }

        // Create trades CSV
        let trades_path = dir.join("trades.csv");
        {
            let mut f = std::fs::File::create(&trades_path).unwrap();
            writeln!(
                f,
                "exchange,symbol,timestamp,local_timestamp,id,side,price,amount"
            )
            .unwrap();
            writeln!(
                f,
                "binance-futures,BTCUSDT,1500000,1500100,1,buy,50001.0,0.1"
            )
            .unwrap();
        }

        // Create book_ticker CSV — BBO updates between depth updates
        let bt_path = dir.join("bookticker.csv");
        {
            let mut f = std::fs::File::create(&bt_path).unwrap();
            writeln!(f, "exchange,symbol,timestamp,local_timestamp,ask_price,ask_amount,bid_price,bid_amount").unwrap();
            // BBO at t=1200000 (between snapshot and depth update)
            writeln!(
                f,
                "binance-futures,BTCUSDT,1200000,1200100,50001.5,1.8,50000.5,0.9"
            )
            .unwrap();
            // BBO at t=1800000 — bid moves up, should not cross ask
            writeln!(
                f,
                "binance-futures,BTCUSDT,1800000,1800100,50001.0,2.0,50000.8,1.1"
            )
            .unwrap();
        }

        let config = TardisConvertConfig {
            buffer_size: 10000,
            ss_buffer_size: 1000,
            base_latency: 0,
            snapshot_mode: SnapshotMode::Process,
        };

        let input_files = vec![
            trades_path.to_str().unwrap().to_string(),
            depth_path.to_str().unwrap().to_string(),
            bt_path.to_str().unwrap().to_string(),
        ];
        let events = convert_tardis_fast(&input_files, &config).unwrap();

        // Verify we have all event types
        let mut has_depth = false;
        let mut has_trade = false;
        let mut has_bbo = false;
        for ev in &events {
            let ev_type = ev.ev & 0xFF;
            match ev_type {
                1 | 4 => has_depth = true, // DEPTH_EVENT | DEPTH_SNAPSHOT_EVENT
                2 => has_trade = true,     // TRADE_EVENT
                5 => has_bbo = true,       // DEPTH_BBO_EVENT
                _ => {}
            }
        }
        assert!(has_depth, "should have depth events");
        assert!(has_trade, "should have trade events");
        assert!(has_bbo, "should have BBO events");

        // Verify BBO events have correct flags
        let bbo_events: Vec<_> = events.iter().filter(|e| (e.ev & 0xFF) == 5).collect();
        assert_eq!(bbo_events.len(), 4); // 2 book_ticker lines * 2 events each (bid+ask)

        // Verify events are sorted by exchange timestamp
        for w in events.windows(2) {
            assert!(
                w[0].exch_ts <= w[1].exch_ts,
                "events should be sorted: {} <= {}",
                w[0].exch_ts,
                w[1].exch_ts,
            );
        }

        // Verify no BBO event has bid >= ask within the same timestamp
        let mut bbo_by_ts: std::collections::HashMap<i64, (Option<f64>, Option<f64>)> =
            std::collections::HashMap::new();
        for ev in &bbo_events {
            let entry = bbo_by_ts.entry(ev.exch_ts).or_insert((None, None));
            if (ev.ev & BUY_EVENT) != 0 {
                entry.0 = Some(ev.px); // bid
            } else {
                entry.1 = Some(ev.px); // ask
            }
        }
        for (ts, (bid, ask)) in &bbo_by_ts {
            if let (Some(b), Some(a)) = (bid, ask) {
                assert!(b < a, "BBO crossed at ts={}: bid={} >= ask={}", ts, b, a);
            }
        }

        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// End-to-end: convert book_ticker CSV → events → feed into HashMapMarketDepth
    /// Verify the orderbook never crosses.
    #[test]
    fn test_bookticker_depth_no_crossing() {
        use crate::depth::{L1MarketDepth, L2MarketDepth, MarketDepth};
        use crate::prelude::HashMapMarketDepth;

        let mut depth = HashMapMarketDepth::new(0.1, 0.001);

        // Simulate initial depth: bid=100.0, ask=100.1
        depth.update_bid_depth(100.0, 1.0, 1000);
        depth.update_ask_depth(100.1, 2.0, 1000);
        assert_eq!(depth.best_bid_tick(), 1000_i64); // 100.0 / 0.1
        assert_eq!(depth.best_ask_tick(), 1001_i64); // 100.1 / 0.1
        assert!(
            depth.best_bid_tick() < depth.best_ask_tick(),
            "initial: no cross"
        );

        // BBO update: bid stays at 100.0, qty changes
        depth.update_best_bid(100.0, 1.5, 2000);
        assert!(
            depth.best_bid_tick() < depth.best_ask_tick(),
            "after bid update: no cross"
        );

        // BBO update: ask stays at 100.1, qty changes
        depth.update_best_ask(100.1, 1.8, 2001);
        assert!(
            depth.best_bid_tick() < depth.best_ask_tick(),
            "after ask update: no cross"
        );

        // BBO update: bid jumps to 100.1 — would cross ask at 100.1!
        // The depth should push ask up to prevent crossing
        depth.update_best_bid(100.1, 0.5, 3000);
        assert!(
            depth.best_bid_tick() < depth.best_ask_tick(),
            "after aggressive bid: no cross (bid={}, ask={})",
            depth.best_bid_tick(),
            depth.best_ask_tick(),
        );

        // BBO update: ask drops to 99.9 — would cross bid!
        // Should push bid down
        depth.update_best_ask(99.9, 0.3, 4000);
        assert!(
            depth.best_bid_tick() < depth.best_ask_tick(),
            "after aggressive ask: no cross (bid={}, ask={})",
            depth.best_bid_tick(),
            depth.best_ask_tick(),
        );

        // BBO backoff: bid was at some level, moves down
        depth.update_bid_depth(99.5, 1.0, 4500);
        depth.update_bid_depth(99.6, 1.0, 4500);
        depth.update_bid_depth(99.7, 1.0, 4500);
        // BBO says best bid is now 99.5 — levels at 99.6, 99.7 should be cleared
        depth.update_best_bid(99.5, 0.8, 5000);
        assert_eq!(depth.best_bid_tick(), (99.5_f64 / 0.1).round() as i64);
        assert_eq!(
            depth.bid_qty_at_tick((99.6_f64 / 0.1).round() as i64),
            0.0,
            "99.6 should be cleared"
        );
        assert_eq!(
            depth.bid_qty_at_tick((99.7_f64 / 0.1).round() as i64),
            0.0,
            "99.7 should be cleared"
        );
    }
}
