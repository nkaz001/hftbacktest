//! Collector data converter.
//!
//! Converts the JSON data collected by the collector into Tardis-compatible CSV format,
//! then uses the Tardis converter to produce HftBacktest Event format.
//!
//! The collector output format is:
//! ```text
//! {timestamp_nanos} {json_data}
//! {timestamp_nanos} {json_data}
//! ...
//! ```
//!
//! Where json_data can be:
//! - Trade events: {"stream":"btcusdt@trade","data":{...}}
//! - Depth updates: {"stream":"btcusdt@depth@0ms","data":{...}}
//! - Book ticker: {"stream":"btcusdt@bookTicker","data":{...}}
//! - Snapshots: {"lastUpdateId":...,"E":...,"T":...,"bids":[...],"asks":[...]}

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde::Deserialize;
use serde_json::Value;

/// Configuration for collector data conversion
#[derive(Debug, Clone)]
pub struct CollectorConvertConfig {
    /// Whether to include snapshots in the output
    pub include_snapshots: bool,
    /// Exchange name for Tardis CSV (default: "binance-futures")
    pub exchange: String,
}

impl Default for CollectorConvertConfig {
    fn default() -> Self {
        Self {
            include_snapshots: true,
            exchange: "binance-futures".to_string(),
        }
    }
}

/// Statistics from conversion
#[derive(Debug, Default)]
pub struct ConversionStats {
    pub trades: usize,
    pub depth_updates: usize,
    pub snapshot_entries: usize,
    pub book_ticker_count: usize,
    pub errors: usize,
}

/// WebSocket stream wrapper (Binance combined stream format)
#[derive(Deserialize, Debug)]
struct StreamMessage {
    stream: String,
    data: Value,
}

/// Flat IPC message with event type discriminator.
/// Used by the IPC collector which normalizes all exchange data to this format.
#[derive(Deserialize, Debug)]
struct FlatMessage {
    e: String,
    #[serde(flatten)]
    data: Value,
}

/// Trade data from WebSocket (Binance stream format with trade ID)
#[derive(Deserialize, Debug)]
struct TradeData {
    #[serde(rename = "E")]
    event_time: i64, // milliseconds
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "t")]
    id: i64,
    #[serde(rename = "p")]
    price: String,
    #[serde(rename = "q")]
    qty: String,
    #[serde(rename = "m")]
    is_buyer_maker: bool, // true = sell (taker is seller), false = buy (taker is buyer)
}

/// Trade data from IPC collector (no trade ID field)
#[derive(Deserialize, Debug)]
struct IpcTradeData {
    #[serde(rename = "E")]
    event_time: i64, // milliseconds
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "p")]
    price: String,
    #[serde(rename = "q")]
    qty: String,
    #[serde(rename = "m")]
    is_buyer_maker: bool, // true = sell (taker is seller), false = buy (taker is buyer)
}

/// Depth update data from WebSocket
#[derive(Deserialize, Debug)]
struct DepthData {
    #[serde(rename = "E")]
    event_time: i64, // milliseconds
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "b")]
    bids: Vec<(String, String)>, // [price, qty]
    #[serde(rename = "a")]
    asks: Vec<(String, String)>,
}

/// Book ticker data from WebSocket or IPC collector
#[derive(Deserialize, Debug)]
struct BookTickerData {
    #[serde(rename = "E")]
    event_time: i64, // milliseconds
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "b")]
    bid_price: String,
    #[serde(rename = "B")]
    bid_qty: String,
    #[serde(rename = "a")]
    ask_price: String,
    #[serde(rename = "A")]
    ask_qty: String,
}

/// REST depth snapshot
#[derive(Deserialize, Debug)]
struct SnapshotData {
    #[serde(rename = "lastUpdateId")]
    last_update_id: i64,
    #[serde(rename = "E")]
    event_time: i64, // milliseconds
    #[serde(rename = "T")]
    transaction_time: i64, // milliseconds
    bids: Vec<(String, String)>,
    asks: Vec<(String, String)>,
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

/// Extract symbol from stream name (e.g., "btcusdt@trade" -> "BTCUSDT")
fn extract_symbol(stream: &str) -> Option<String> {
    stream.split('@').next().map(|s| s.to_uppercase())
}

/// Parse a collector line into timestamp and JSON
fn parse_line(line: &str) -> Option<(i64, &str)> {
    let parts: Vec<&str> = line.splitn(2, ' ').collect();
    if parts.len() != 2 {
        return None;
    }
    let timestamp_nanos: i64 = parts[0].parse().ok()?;
    Some((timestamp_nanos, parts[1]))
}

/// Convert collector JSON files to Tardis CSV format.
///
/// # Arguments
/// * `input_files` - Input collector .gz files
/// * `trades_output` - Output path for trades CSV
/// * `depth_output` - Output path for depth CSV
/// * `book_ticker_output` - Output path for book ticker CSV
/// * `config` - Conversion configuration
///
/// # Returns
/// Conversion statistics
pub fn convert_to_tardis_csv<P1: AsRef<Path>, P2: AsRef<Path>, P3: AsRef<Path>, P4: AsRef<Path>>(
    input_files: &[P1],
    trades_output: P2,
    depth_output: P3,
    book_ticker_output: P4,
    config: &CollectorConvertConfig,
) -> Result<ConversionStats, String> {
    let mut stats = ConversionStats::default();

    // Create output files
    let trades_file = File::create(trades_output.as_ref())
        .map_err(|e| format!("Failed to create trades output: {}", e))?;
    let depth_file = File::create(depth_output.as_ref())
        .map_err(|e| format!("Failed to create depth output: {}", e))?;
    let book_ticker_file = File::create(book_ticker_output.as_ref())
        .map_err(|e| format!("Failed to create book ticker output: {}", e))?;

    let mut trades_writer = GzEncoder::new(trades_file, Compression::default());
    let mut depth_writer = GzEncoder::new(depth_file, Compression::default());
    let mut bt_writer = GzEncoder::new(book_ticker_file, Compression::default());

    // Write headers
    writeln!(
        trades_writer,
        "exchange,symbol,timestamp,local_timestamp,id,side,price,amount"
    )
    .map_err(|e| format!("Failed to write trades header: {}", e))?;

    writeln!(
        depth_writer,
        "exchange,symbol,timestamp,local_timestamp,is_snapshot,side,price,amount"
    )
    .map_err(|e| format!("Failed to write depth header: {}", e))?;

    writeln!(
        bt_writer,
        "exchange,symbol,timestamp,local_timestamp,ask_amount,ask_price,bid_price,bid_amount"
    )
    .map_err(|e| format!("Failed to write book ticker header: {}", e))?;

    let mut trade_id_counter: i64 = 0;

    for file_path in input_files {
        let path = file_path.as_ref();
        println!("Processing {:?}", path);

        let reader = open_file(path).map_err(|e| format!("Failed to open file: {}", e))?;

        for line_result in reader.lines() {
            let line = line_result.map_err(|e| format!("Failed to read line: {}", e))?;
            if line.is_empty() {
                continue;
            }

            let (local_ts_nanos, json_str) = match parse_line(&line) {
                Some(v) => v,
                None => {
                    stats.errors += 1;
                    continue;
                }
            };

            // Convert nanoseconds to microseconds for Tardis format
            let local_ts_micros = local_ts_nanos / 1000;

            // Try to parse as Binance combined stream message first
            if let Ok(stream_msg) = serde_json::from_str::<StreamMessage>(json_str) {
                let stream_type = stream_msg.stream.split('@').nth(1).unwrap_or("");
                let symbol = extract_symbol(&stream_msg.stream).unwrap_or_default();

                match stream_type {
                    "trade" => {
                        if let Ok(trade) = serde_json::from_value::<TradeData>(stream_msg.data) {
                            let exch_ts_micros = trade.event_time * 1000;
                            let side = if trade.is_buyer_maker { "sell" } else { "buy" };

                            writeln!(
                                trades_writer,
                                "{},{},{},{},{},{},{},{}",
                                config.exchange,
                                symbol,
                                exch_ts_micros,
                                local_ts_micros,
                                trade.id,
                                side,
                                trade.price,
                                trade.qty
                            )
                            .map_err(|e| format!("Failed to write trade: {}", e))?;

                            stats.trades += 1;
                        }
                    }
                    "depth@0ms" | "depth" => {
                        if let Ok(depth) = serde_json::from_value::<DepthData>(stream_msg.data) {
                            let exch_ts_micros = depth.event_time * 1000;

                            for (price, qty) in &depth.bids {
                                writeln!(
                                    depth_writer,
                                    "{},{},{},{},false,bid,{},{}",
                                    config.exchange,
                                    symbol,
                                    exch_ts_micros,
                                    local_ts_micros,
                                    price,
                                    qty
                                )
                                .map_err(|e| format!("Failed to write depth: {}", e))?;
                                stats.depth_updates += 1;
                            }

                            for (price, qty) in &depth.asks {
                                writeln!(
                                    depth_writer,
                                    "{},{},{},{},false,ask,{},{}",
                                    config.exchange,
                                    symbol,
                                    exch_ts_micros,
                                    local_ts_micros,
                                    price,
                                    qty
                                )
                                .map_err(|e| format!("Failed to write depth: {}", e))?;
                                stats.depth_updates += 1;
                            }
                        }
                    }
                    "bookTicker" => {
                        if let Ok(bt) = serde_json::from_value::<BookTickerData>(stream_msg.data) {
                            let exch_ts_micros = bt.event_time * 1000;

                            writeln!(
                                bt_writer,
                                "{},{},{},{},{},{},{},{}",
                                config.exchange,
                                bt.symbol,
                                exch_ts_micros,
                                local_ts_micros,
                                bt.ask_qty,
                                bt.ask_price,
                                bt.bid_price,
                                bt.bid_qty
                            )
                            .map_err(|e| format!("Failed to write book ticker: {}", e))?;

                            stats.book_ticker_count += 1;
                        }
                    }
                    _ => {}
                }
            } else if let Ok(flat_msg) = serde_json::from_str::<FlatMessage>(json_str) {
                // Flat IPC format: {"e":"depthUpdate","E":...,"s":...,...}
                match flat_msg.e.as_str() {
                    "trade" => {
                        if let Ok(trade) = serde_json::from_str::<IpcTradeData>(json_str) {
                            let exch_ts_micros = trade.event_time * 1000;
                            let side = if trade.is_buyer_maker { "sell" } else { "buy" };
                            trade_id_counter += 1;

                            writeln!(
                                trades_writer,
                                "{},{},{},{},{},{},{},{}",
                                config.exchange,
                                trade.symbol,
                                exch_ts_micros,
                                local_ts_micros,
                                trade_id_counter,
                                side,
                                trade.price,
                                trade.qty
                            )
                            .map_err(|e| format!("Failed to write trade: {}", e))?;

                            stats.trades += 1;
                        }
                    }
                    "depthUpdate" => {
                        if let Ok(depth) = serde_json::from_str::<DepthData>(json_str) {
                            let exch_ts_micros = depth.event_time * 1000;

                            for (price, qty) in &depth.bids {
                                writeln!(
                                    depth_writer,
                                    "{},{},{},{},false,bid,{},{}",
                                    config.exchange,
                                    depth.symbol,
                                    exch_ts_micros,
                                    local_ts_micros,
                                    price,
                                    qty
                                )
                                .map_err(|e| format!("Failed to write depth: {}", e))?;
                                stats.depth_updates += 1;
                            }

                            for (price, qty) in &depth.asks {
                                writeln!(
                                    depth_writer,
                                    "{},{},{},{},false,ask,{},{}",
                                    config.exchange,
                                    depth.symbol,
                                    exch_ts_micros,
                                    local_ts_micros,
                                    price,
                                    qty
                                )
                                .map_err(|e| format!("Failed to write depth: {}", e))?;
                                stats.depth_updates += 1;
                            }
                        }
                    }
                    "depthSnapshot" => {
                        if let Ok(depth) = serde_json::from_str::<DepthData>(json_str) {
                            let exch_ts_micros = depth.event_time * 1000;

                            for (price, qty) in &depth.bids {
                                writeln!(
                                    depth_writer,
                                    "{},{},{},{},true,bid,{},{}",
                                    config.exchange,
                                    depth.symbol,
                                    exch_ts_micros,
                                    local_ts_micros,
                                    price,
                                    qty
                                )
                                .map_err(|e| format!("Failed to write depth: {}", e))?;
                                stats.snapshot_entries += 1;
                            }

                            for (price, qty) in &depth.asks {
                                writeln!(
                                    depth_writer,
                                    "{},{},{},{},true,ask,{},{}",
                                    config.exchange,
                                    depth.symbol,
                                    exch_ts_micros,
                                    local_ts_micros,
                                    price,
                                    qty
                                )
                                .map_err(|e| format!("Failed to write depth: {}", e))?;
                                stats.snapshot_entries += 1;
                            }
                        }
                    }
                    "bookTicker" => {
                        if let Ok(bt) = serde_json::from_str::<BookTickerData>(json_str) {
                            let exch_ts_micros = bt.event_time * 1000;

                            writeln!(
                                bt_writer,
                                "{},{},{},{},{},{},{},{}",
                                config.exchange,
                                bt.symbol,
                                exch_ts_micros,
                                local_ts_micros,
                                bt.ask_qty,
                                bt.ask_price,
                                bt.bid_price,
                                bt.bid_qty
                            )
                            .map_err(|e| format!("Failed to write book ticker: {}", e))?;

                            stats.book_ticker_count += 1;
                        }
                    }
                    _ => {}
                }
            } else if config.include_snapshots {
                // Try to parse as REST depth snapshot (Binance format)
                if let Ok(snapshot) = serde_json::from_str::<SnapshotData>(json_str) {
                    let exch_ts_micros = snapshot.event_time * 1000;
                    let symbol = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .and_then(|s| s.split('_').next())
                        .map(|s| s.to_uppercase())
                        .unwrap_or_else(|| "BTCUSDT".to_string());

                    for (price, qty) in &snapshot.bids {
                        writeln!(
                            depth_writer,
                            "{},{},{},{},true,bid,{},{}",
                            config.exchange, symbol, exch_ts_micros, local_ts_micros, price, qty
                        )
                        .map_err(|e| format!("Failed to write snapshot: {}", e))?;
                        stats.snapshot_entries += 1;
                    }

                    for (price, qty) in &snapshot.asks {
                        writeln!(
                            depth_writer,
                            "{},{},{},{},true,ask,{},{}",
                            config.exchange, symbol, exch_ts_micros, local_ts_micros, price, qty
                        )
                        .map_err(|e| format!("Failed to write snapshot: {}", e))?;
                        stats.snapshot_entries += 1;
                    }
                }
            }
        }
    }

    // Finish compression
    trades_writer
        .finish()
        .map_err(|e| format!("Failed to finish trades file: {}", e))?;
    depth_writer
        .finish()
        .map_err(|e| format!("Failed to finish depth file: {}", e))?;
    bt_writer
        .finish()
        .map_err(|e| format!("Failed to finish book ticker file: {}", e))?;

    println!(
        "Conversion complete: {} trades, {} depth updates, {} snapshot entries, {} book tickers",
        stats.trades, stats.depth_updates, stats.snapshot_entries, stats.book_ticker_count
    );

    Ok(stats)
}

/// Convert collector files directly to HftBacktest Event format.
///
/// This is a convenience function that:
/// 1. Converts collector JSON to Tardis CSV format (temp files)
/// 2. Uses the Tardis converter to produce Event format
/// 3. Optionally saves to NPZ file
///
/// # Arguments
/// * `input_files` - Input collector .gz files
/// * `output_path` - Output NPZ file path (optional)
/// * `config` - Conversion configuration
#[cfg(feature = "backtest")]
pub fn convert_collector_to_events<P: AsRef<Path>>(
    input_files: &[P],
    output_path: Option<P>,
    config: &CollectorConvertConfig,
) -> Result<(Vec<crate::types::Event>, ConversionStats), String> {
    use std::fs;
    use tempfile::tempdir;

    use super::{TardisConvertConfig, convert_tardis_fast, write_npz_file};

    // Create temp directory for intermediate files
    let temp_dir = tempdir().map_err(|e| format!("Failed to create temp dir: {}", e))?;

    let trades_path = temp_dir.path().join("trades.csv.gz");
    let depth_path = temp_dir.path().join("depth.csv.gz");
    let bt_path = temp_dir.path().join("book_ticker.csv.gz");

    // Step 1: Convert to Tardis CSV
    let stats = convert_to_tardis_csv(input_files, &trades_path, &depth_path, &bt_path, config)?;

    // Step 2: Convert Tardis CSV to Events
    let tardis_config = TardisConvertConfig::default();
    let mut input_paths: Vec<&Path> = vec![trades_path.as_ref(), depth_path.as_ref()];
    if stats.book_ticker_count > 0 {
        input_paths.push(bt_path.as_ref());
    }
    let events = convert_tardis_fast(&input_paths, &tardis_config)?;

    // Step 3: Optionally save to NPZ
    if let Some(path) = output_path {
        write_npz_file(&events, path)?;
    }

    // Cleanup temp files
    let _ = fs::remove_file(&trades_path);
    let _ = fs::remove_file(&depth_path);
    let _ = fs::remove_file(&bt_path);

    Ok((events, stats))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use tempfile::NamedTempFile;

    fn create_test_collector_file() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();

        // Write test data in collector format
        let lines = vec![
            // Trade event
            r#"1700000000000000000 {"stream":"btcusdt@trade","data":{"E":1700000000000,"s":"BTCUSDT","t":123456,"p":"50000.50","q":"0.001","m":false}}"#,
            // Depth update
            r#"1700000001000000000 {"stream":"btcusdt@depth@0ms","data":{"E":1700000001000,"s":"BTCUSDT","b":[["50000.00","1.5"]],"a":[["50001.00","2.0"]]}}"#,
            // Book ticker
            r#"1700000001500000000 {"stream":"btcusdt@bookTicker","data":{"e":"bookTicker","E":1700000001500,"T":1700000001500,"s":"BTCUSDT","b":"50000.0","B":"1.0","a":"50001.0","A":"2.0"}}"#,
            // Snapshot
            r#"1700000002000000000 {"lastUpdateId":1000,"E":1700000002000,"T":1700000002000,"bids":[["49999.00","10.0"]],"asks":[["50002.00","5.0"]]}"#,
        ];

        for line in lines {
            writeln!(file, "{}", line).unwrap();
        }

        file
    }

    fn read_gzip_to_string(path: &Path) -> String {
        let file = File::open(path).unwrap();
        let mut decoder = GzDecoder::new(file);
        let mut contents = String::new();
        decoder.read_to_string(&mut contents).unwrap();
        contents
    }

    #[test]
    fn test_parse_line() {
        let line = r#"1700000000000000000 {"stream":"btcusdt@trade","data":{}}"#;
        let (ts, json) = parse_line(line).unwrap();
        assert_eq!(ts, 1700000000000000000);
        assert!(json.contains("stream"));
    }

    #[test]
    fn test_extract_symbol() {
        assert_eq!(extract_symbol("btcusdt@trade"), Some("BTCUSDT".to_string()));
        assert_eq!(
            extract_symbol("ethusdt@depth@0ms"),
            Some("ETHUSDT".to_string())
        );
    }

    #[test]
    fn test_convert_to_tardis_csv() {
        let input = create_test_collector_file();
        let trades_output = NamedTempFile::new().unwrap();
        let depth_output = NamedTempFile::new().unwrap();
        let bt_output = NamedTempFile::new().unwrap();

        let config = CollectorConvertConfig::default();
        let stats = convert_to_tardis_csv(
            &[input.path()],
            trades_output.path(),
            depth_output.path(),
            bt_output.path(),
            &config,
        )
        .unwrap();

        assert_eq!(stats.trades, 1);
        assert_eq!(stats.depth_updates, 2); // 1 bid + 1 ask
        assert_eq!(stats.snapshot_entries, 2); // 1 bid + 1 ask
        assert_eq!(stats.book_ticker_count, 1);

        let bt_csv = read_gzip_to_string(bt_output.path());
        let mut lines = bt_csv.lines();
        assert_eq!(
            lines.next(),
            Some(
                "exchange,symbol,timestamp,local_timestamp,ask_amount,ask_price,bid_price,bid_amount"
            )
        );
        assert_eq!(
            lines.next(),
            Some(
                "binance-futures,BTCUSDT,1700000001500000,1700000001500000,2.0,50001.0,50000.0,1.0"
            )
        );
    }

    fn create_test_ipc_collector_file() -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();

        let lines = vec![
            // IPC bookTicker (complete BBO)
            r#"1770281275681188238 {"e":"bookTicker","s":"ADAUSDT","b":"0.2819","B":"168493","a":"0.2820","A":"50000","E":1770281275474}"#,
            // IPC depthUpdate (bid)
            r#"1770281275681192795 {"e":"depthUpdate","E":1770281275474,"s":"ADAUSDT","b":[["0.28","0"]],"a":[]}"#,
            // IPC depthUpdate (ask)
            r#"1770281275681196921 {"e":"depthUpdate","E":1770281275474,"s":"ADAUSDT","b":[],"a":[["0.283","50000"]]}"#,
            // IPC trade
            r#"1770281275991408629 {"e":"trade","E":1770281275940,"s":"ADAUSDT","p":"0.282","q":"11011","m":false}"#,
            // IPC trade (seller)
            r#"1770281280769764789 {"e":"trade","E":1770281280693,"s":"ADAUSDT","p":"0.2821","q":"35","m":true}"#,
            // IPC depthSnapshot
            r#"1770281290000000000 {"e":"depthSnapshot","E":1770281290000,"s":"ADAUSDT","b":[["0.2819","100000"]],"a":[["0.282","200000"]]}"#,
        ];

        for line in lines {
            writeln!(file, "{}", line).unwrap();
        }

        file
    }

    #[test]
    fn test_convert_ipc_format() {
        let input = create_test_ipc_collector_file();
        let trades_output = NamedTempFile::new().unwrap();
        let depth_output = NamedTempFile::new().unwrap();
        let bt_output = NamedTempFile::new().unwrap();

        let config = CollectorConvertConfig::default();
        let stats = convert_to_tardis_csv(
            &[input.path()],
            trades_output.path(),
            depth_output.path(),
            bt_output.path(),
            &config,
        )
        .unwrap();

        assert_eq!(stats.trades, 2);
        assert_eq!(stats.depth_updates, 2); // 1 bid + 1 ask
        assert_eq!(stats.snapshot_entries, 2); // 1 bid + 1 ask from depthSnapshot
        assert_eq!(stats.book_ticker_count, 1);

        let bt_csv = read_gzip_to_string(bt_output.path());
        let mut lines = bt_csv.lines();
        assert_eq!(
            lines.next(),
            Some(
                "exchange,symbol,timestamp,local_timestamp,ask_amount,ask_price,bid_price,bid_amount"
            )
        );
        assert_eq!(
            lines.next(),
            Some(
                "binance-futures,ADAUSDT,1770281275474000,1770281275681188,50000,0.2820,0.2819,168493"
            )
        );
    }
}
