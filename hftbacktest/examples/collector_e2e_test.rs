//! End-to-end test for collector data pipeline.
//!
//! This example:
//! 1. Collects real-time data from Binance Futures (BTCUSDT)
//! 2. Converts collector output to Tardis CSV format
//! 3. Converts Tardis CSV to HftBacktest Event format
//! 4. Validates the data for correctness
//!
//! Usage:
//!   cargo run --release --example collector_e2e_test -- --duration 60 --output ./data
//!
//! This will collect 60 seconds of data and save to ./data directory.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use clap::Parser;
use flate2::read::GzDecoder;

use hftbacktest::data::{
    CollectorConvertConfig, convert_collector_to_events, convert_to_tardis_csv,
};
use hftbacktest::types::{
    BUY_EVENT, DEPTH_CLEAR_EVENT, DEPTH_EVENT, DEPTH_SNAPSHOT_EVENT, Event, SELL_EVENT, TRADE_EVENT,
};

#[derive(Parser, Debug)]
#[command(name = "collector_e2e_test")]
#[command(about = "End-to-end test for collector data pipeline")]
struct Args {
    /// Duration to collect data in seconds
    #[arg(short, long, default_value = "30")]
    duration: u64,

    /// Output directory for data files
    #[arg(short, long, default_value = "./collector_test_data")]
    output: PathBuf,

    /// Symbol to collect (lowercase)
    #[arg(short, long, default_value = "btcusdt")]
    symbol: String,

    /// Skip data collection (use existing data)
    #[arg(long)]
    skip_collect: bool,
}

/// Validation results
#[derive(Debug, Default)]
struct ValidationResult {
    total_events: usize,
    trade_events: usize,
    depth_events: usize,
    snapshot_events: usize,
    clear_events: usize,
    buy_events: usize,
    sell_events: usize,

    // Timestamp checks
    min_exch_ts: i64,
    max_exch_ts: i64,
    min_local_ts: i64,
    max_local_ts: i64,

    // Latency stats
    min_latency_ns: i64,
    max_latency_ns: i64,
    avg_latency_ns: f64,

    // Price range (for trades)
    min_price: f64,
    max_price: f64,

    // Order violations
    exch_ts_violations: usize,
    local_ts_violations: usize,

    // Errors
    errors: Vec<String>,
}

fn run_collector(output_dir: &Path, symbol: &str, duration: Duration) -> Result<PathBuf, String> {
    println!("Starting collector for {} seconds...", duration.as_secs());

    // Ensure output directory exists
    fs::create_dir_all(output_dir)
        .map_err(|e| format!("Failed to create output directory: {}", e))?;

    // Build collector first
    println!("Building collector...");
    let status = Command::new("cargo")
        .args(["build", "--release", "-p", "collector"])
        .current_dir("/home/b0qi/bookdisco/hftbacktest")
        .status()
        .map_err(|e| format!("Failed to build collector: {}", e))?;

    if !status.success() {
        return Err("Collector build failed".to_string());
    }

    // Run collector
    let collector_path = "/home/b0qi/bookdisco/hftbacktest/target/release/collector";
    let output_path = output_dir.to_string_lossy().to_string();

    println!(
        "Running collector: {} {} binancefuturesum {}",
        collector_path, output_path, symbol
    );

    let mut child: Child = Command::new(collector_path)
        .args([&output_path, "binancefuturesum", symbol])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start collector: {}", e))?;

    let start = Instant::now();

    // Wait for duration
    while start.elapsed() < duration {
        std::thread::sleep(Duration::from_secs(1));
        let elapsed = start.elapsed().as_secs();
        print!("\rCollecting... {}s / {}s", elapsed, duration.as_secs());
        std::io::stdout().flush().ok();
    }
    println!();

    // Stop collector
    println!("Stopping collector...");
    child.kill().ok();
    child.wait().ok();

    // Find the output file
    let today = chrono::Utc::now().format("%Y%m%d").to_string();
    let expected_file = output_dir.join(format!("{}_{}.gz", symbol, today));

    if expected_file.exists() {
        println!("Collector output: {:?}", expected_file);
        Ok(expected_file)
    } else {
        // Try to find any matching file
        let entries = fs::read_dir(output_dir)
            .map_err(|e| format!("Failed to read output directory: {}", e))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "gz") {
                let name = path.file_name().unwrap().to_string_lossy();
                if name.starts_with(symbol) {
                    println!("Collector output: {:?}", path);
                    return Ok(path);
                }
            }
        }

        Err(format!(
            "No collector output file found in {:?}",
            output_dir
        ))
    }
}

fn count_lines_in_gz(path: &Path) -> Result<usize, String> {
    let file = File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
    let reader = BufReader::new(GzDecoder::new(file));
    Ok(reader.lines().count())
}

fn validate_events(events: &[Event]) -> ValidationResult {
    let mut result = ValidationResult::default();

    if events.is_empty() {
        result.errors.push("No events to validate".to_string());
        return result;
    }

    result.total_events = events.len();
    result.min_exch_ts = i64::MAX;
    result.max_exch_ts = i64::MIN;
    result.min_local_ts = i64::MAX;
    result.max_local_ts = i64::MIN;
    result.min_latency_ns = i64::MAX;
    result.max_latency_ns = i64::MIN;
    result.min_price = f64::INFINITY;
    result.max_price = f64::NEG_INFINITY;

    let mut prev_exch_ts = 0i64;
    let mut prev_local_ts = 0i64;
    let mut total_latency: i64 = 0;

    for event in events {
        // Count event types
        if event.ev & TRADE_EVENT != 0 {
            result.trade_events += 1;
            if event.px > 0.0 {
                result.min_price = result.min_price.min(event.px);
                result.max_price = result.max_price.max(event.px);
            }
        }
        if event.ev & DEPTH_EVENT != 0 {
            result.depth_events += 1;
        }
        if event.ev & DEPTH_SNAPSHOT_EVENT != 0 {
            result.snapshot_events += 1;
        }
        if event.ev & DEPTH_CLEAR_EVENT != 0 {
            result.clear_events += 1;
        }
        if event.ev & BUY_EVENT != 0 {
            result.buy_events += 1;
        }
        if event.ev & SELL_EVENT != 0 {
            result.sell_events += 1;
        }

        // Timestamp stats
        result.min_exch_ts = result.min_exch_ts.min(event.exch_ts);
        result.max_exch_ts = result.max_exch_ts.max(event.exch_ts);
        result.min_local_ts = result.min_local_ts.min(event.local_ts);
        result.max_local_ts = result.max_local_ts.max(event.local_ts);

        // Latency
        let latency = event.local_ts - event.exch_ts;
        result.min_latency_ns = result.min_latency_ns.min(latency);
        result.max_latency_ns = result.max_latency_ns.max(latency);
        total_latency += latency;

        // Check ordering
        if event.exch_ts < prev_exch_ts {
            result.exch_ts_violations += 1;
        }
        if event.local_ts < prev_local_ts {
            result.local_ts_violations += 1;
        }

        prev_exch_ts = event.exch_ts;
        prev_local_ts = event.local_ts;
    }

    result.avg_latency_ns = total_latency as f64 / events.len() as f64;

    // Validate results
    if result.trade_events == 0 {
        result.errors.push("No trade events found".to_string());
    }
    if result.depth_events == 0 && result.snapshot_events == 0 {
        result
            .errors
            .push("No depth or snapshot events found".to_string());
    }
    if result.exch_ts_violations > 0 {
        result.errors.push(format!(
            "Exchange timestamp violations: {}",
            result.exch_ts_violations
        ));
    }
    if result.local_ts_violations > 0 {
        result.errors.push(format!(
            "Local timestamp violations: {}",
            result.local_ts_violations
        ));
    }

    result
}

fn validate_orderbook_consistency(events: &[Event]) -> Result<(), String> {
    println!("\nValidating orderbook consistency...");

    let mut bids: BTreeMap<i64, f64> = BTreeMap::new(); // price (as i64 cents) -> qty
    let mut asks: BTreeMap<i64, f64> = BTreeMap::new();

    let price_to_key = |price: f64| -> i64 { (price * 100.0) as i64 };

    let mut depth_event_count = 0;
    let mut consistency_errors = 0;

    for event in events {
        let is_bid = event.ev & BUY_EVENT != 0;
        let is_ask = event.ev & SELL_EVENT != 0;
        let book = if is_bid { &mut bids } else { &mut asks };

        if event.ev & DEPTH_CLEAR_EVENT != 0 {
            book.clear();
        } else if event.ev & (DEPTH_EVENT | DEPTH_SNAPSHOT_EVENT) != 0 {
            let key = price_to_key(event.px);
            if event.qty == 0.0 {
                book.remove(&key);
            } else {
                book.insert(key, event.qty);
            }
            depth_event_count += 1;

            // Check consistency every 1000 events
            if depth_event_count % 1000 == 0 && !bids.is_empty() && !asks.is_empty() {
                let best_bid = *bids.keys().last().unwrap();
                let best_ask = *asks.keys().next().unwrap();

                if best_bid >= best_ask {
                    consistency_errors += 1;
                    if consistency_errors <= 5 {
                        println!(
                            "  Warning: Crossed book at event {}: bid {} >= ask {}",
                            depth_event_count,
                            best_bid as f64 / 100.0,
                            best_ask as f64 / 100.0
                        );
                    }
                }
            }
        }
    }

    println!(
        "  Processed {} depth events, {} consistency errors",
        depth_event_count, consistency_errors
    );

    if consistency_errors > depth_event_count / 100 {
        // More than 1% errors
        Err(format!(
            "Too many orderbook consistency errors: {} / {}",
            consistency_errors, depth_event_count
        ))
    } else {
        Ok(())
    }
}

fn print_validation_result(result: &ValidationResult) {
    println!("\n=== Validation Results ===");
    println!("Total events: {}", result.total_events);
    println!("  Trade events: {}", result.trade_events);
    println!("  Depth events: {}", result.depth_events);
    println!("  Snapshot events: {}", result.snapshot_events);
    println!("  Clear events: {}", result.clear_events);
    println!("  Buy side: {}", result.buy_events);
    println!("  Sell side: {}", result.sell_events);

    println!("\nTimestamp range:");
    let duration_ns = result.max_exch_ts - result.min_exch_ts;
    let duration_sec = duration_ns as f64 / 1_000_000_000.0;
    println!("  Duration: {:.2} seconds", duration_sec);

    println!("\nLatency stats:");
    println!(
        "  Min: {:.3} ms",
        result.min_latency_ns as f64 / 1_000_000.0
    );
    println!(
        "  Max: {:.3} ms",
        result.max_latency_ns as f64 / 1_000_000.0
    );
    println!("  Avg: {:.3} ms", result.avg_latency_ns / 1_000_000.0);

    if result.trade_events > 0 {
        println!("\nPrice range (trades):");
        println!("  Min: {:.2}", result.min_price);
        println!("  Max: {:.2}", result.max_price);
    }

    if !result.errors.is_empty() {
        println!("\nErrors:");
        for err in &result.errors {
            println!("  - {}", err);
        }
    } else {
        println!("\nNo errors found!");
    }
}

fn main() -> Result<(), String> {
    let args = Args::parse();

    println!("=== Collector End-to-End Test ===");
    println!("Symbol: {}", args.symbol);
    println!("Duration: {} seconds", args.duration);
    println!("Output: {:?}", args.output);
    println!();

    // Step 1: Collect data (or use existing)
    let collector_file = if args.skip_collect {
        // Find existing file
        let entries = fs::read_dir(&args.output)
            .map_err(|e| format!("Failed to read output directory: {}", e))?;

        let mut found = None;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "gz") {
                let name = path.file_name().unwrap().to_string_lossy();
                if name.starts_with(&args.symbol) {
                    found = Some(path);
                    break;
                }
            }
        }
        found.ok_or_else(|| format!("No existing data file found in {:?}", args.output))?
    } else {
        run_collector(
            &args.output,
            &args.symbol,
            Duration::from_secs(args.duration),
        )?
    };

    // Count raw lines
    let raw_lines = count_lines_in_gz(&collector_file)?;
    println!("Raw collector output: {} lines", raw_lines);

    // Step 2: Convert to Tardis CSV
    println!("\nConverting to Tardis CSV format...");
    let trades_csv = args.output.join("trades.csv.gz");
    let depth_csv = args.output.join("depth.csv.gz");
    let book_ticker_csv = args.output.join("book_ticker.csv.gz");

    let config = CollectorConvertConfig::default();
    let stats = convert_to_tardis_csv(
        &[&collector_file],
        &trades_csv,
        &depth_csv,
        &book_ticker_csv,
        &config,
    )?;

    println!("Conversion stats:");
    println!("  Trades: {}", stats.trades);
    println!("  Depth updates: {}", stats.depth_updates);
    println!("  Snapshot entries: {}", stats.snapshot_entries);
    println!("  Book tickers: {}", stats.book_ticker_count);

    // Step 3: Convert to Event format
    println!("\nConverting to Event format...");
    let output_npz = args.output.join("events.npz");

    let (events, _) = convert_collector_to_events(
        &[&collector_file],
        Some(&output_npz),
        &CollectorConvertConfig::default(),
    )?;

    println!("Generated {} events", events.len());
    println!("Saved to {:?}", output_npz);

    // Step 4: Validate
    println!("\nValidating events...");
    let validation = validate_events(&events);
    print_validation_result(&validation);

    // Step 5: Validate orderbook consistency
    validate_orderbook_consistency(&events)?;

    println!("\n=== Test Complete ===");

    if validation.errors.is_empty() {
        println!("All validations passed!");
        Ok(())
    } else {
        Err(format!(
            "{} validation errors found",
            validation.errors.len()
        ))
    }
}
