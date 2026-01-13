//! Batch convert Tardis data files to NPZ format
//!
//! Usage:
//!   cargo run --release -p hftbacktest --example tardis_batch_convert --no-default-features --features backtest
//!   cargo run --release -p hftbacktest --example tardis_batch_convert --no-default-features --features backtest -- --symbol BTCUSDT
//!   cargo run --release -p hftbacktest --example tardis_batch_convert --no-default-features --features backtest -- --symbol BTCUSDT --date 2025-06-30
//!   cargo run --release -p hftbacktest --example tardis_batch_convert --no-default-features --features backtest -- --list
//!   cargo run --release -p hftbacktest --example tardis_batch_convert --no-default-features --features backtest -- --dry-run

use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::Path;
use std::time::Instant;

use chrono::{Duration, NaiveDate};
use hftbacktest::data::{convert_and_save_fast, write_npz_file, SnapshotMode, TardisConvertConfig};
use hftbacktest::types::{Event, BUY_EVENT, DEPTH_EVENT, DEPTH_SNAPSHOT_EVENT, SELL_EVENT};
use std::collections::BTreeMap;

const BASE_PATH: &str = "/mnt/data/samba/users/Data/tardis/datasets/binance-futures";

fn get_trades_path(symbol: &str, date: &str) -> String {
    format!("{}/trades/{}/{}.csv.gz", BASE_PATH, symbol, date)
}

fn get_depth_path(symbol: &str, date: &str) -> String {
    format!("{}/incremental_book_L2/{}/{}.csv.gz", BASE_PATH, symbol, date)
}

fn get_output_dir(symbol: &str) -> String {
    format!("{}/hft_npz/{}", BASE_PATH, symbol)
}

fn get_output_path(symbol: &str, date: &str) -> String {
    format!(
        "{}/{}_{}.npz",
        get_output_dir(symbol),
        symbol,
        date.replace("-", "")
    )
}

fn get_last_output_path(symbol: &str, date: &str) -> String {
    format!(
        "{}/{}_{}_last.npz",
        get_output_dir(symbol),
        symbol,
        date.replace("-", "")
    )
}

/// List all available symbols that have both trades and depth data
fn list_symbols() -> Vec<String> {
    let trades_dir = format!("{}/trades", BASE_PATH);
    let depth_dir = format!("{}/incremental_book_L2", BASE_PATH);

    let trades_symbols: HashSet<String> = fs::read_dir(&trades_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let depth_symbols: HashSet<String> = fs::read_dir(&depth_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let mut common: Vec<String> = trades_symbols.intersection(&depth_symbols).cloned().collect();
    common.sort();
    common
}

/// Get dates that have both trades and depth data for a symbol
fn get_valid_dates(symbol: &str) -> Vec<String> {
    let trades_dir = format!("{}/trades/{}", BASE_PATH, symbol);
    let depth_dir = format!("{}/incremental_book_L2/{}", BASE_PATH, symbol);

    let trades_dates: HashSet<String> = fs::read_dir(&trades_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let name = e.file_name().to_str()?.to_string();
                    if name.ends_with(".csv.gz") {
                        Some(name.trim_end_matches(".csv.gz").to_string())
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let depth_dates: HashSet<String> = fs::read_dir(&depth_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| {
                    let name = e.file_name().to_str()?.to_string();
                    if name.ends_with(".csv.gz") {
                        Some(name.trim_end_matches(".csv.gz").to_string())
                    } else {
                        None
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let mut common: Vec<String> = trades_dates.intersection(&depth_dates).cloned().collect();
    common.sort();
    common
}

/// Check if output file already exists
fn output_exists(symbol: &str, date: &str) -> bool {
    Path::new(&get_output_path(symbol, date)).exists()
}

/// Generate last snapshot from events
fn generate_last_snapshot(
    events: &[Event],
    output_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut bids: BTreeMap<i64, f64> = BTreeMap::new();
    let mut asks: BTreeMap<i64, f64> = BTreeMap::new();

    let mut last_timestamp = 0i64;
    let mut last_local_timestamp = 0i64;

    for event in events {
        let ev_type = event.ev & 0xFF;
        if ev_type == DEPTH_EVENT || ev_type == DEPTH_SNAPSHOT_EVENT {
            let price_key = (event.px * 1e8) as i64;
            let is_bid = (event.ev & BUY_EVENT) != 0;

            if is_bid {
                if event.qty == 0.0 {
                    bids.remove(&price_key);
                } else {
                    bids.insert(price_key, event.qty);
                }
            } else {
                if event.qty == 0.0 {
                    asks.remove(&price_key);
                } else {
                    asks.insert(price_key, event.qty);
                }
            }

            last_timestamp = event.exch_ts;
            last_local_timestamp = event.local_ts;
        }
    }

    let mut snapshot_events: Vec<Event> = Vec::new();

    for (&price_key, &qty) in bids.iter().rev() {
        let px = price_key as f64 / 1e8;
        snapshot_events.push(Event {
            ev: DEPTH_SNAPSHOT_EVENT | BUY_EVENT,
            exch_ts: last_timestamp,
            local_ts: last_local_timestamp,
            px,
            qty,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
    }

    for (&price_key, &qty) in asks.iter() {
        let px = price_key as f64 / 1e8;
        snapshot_events.push(Event {
            ev: DEPTH_SNAPSHOT_EVENT | SELL_EVENT,
            exch_ts: last_timestamp,
            local_ts: last_local_timestamp,
            px,
            qty,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
    }

    write_npz_file(&snapshot_events, output_path).map_err(|e| {
        Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error>
    })
}

/// Convert a single day
fn convert_single_day(
    symbol: &str,
    date: &str,
    config: &TardisConvertConfig,
) -> Result<bool, Box<dyn std::error::Error>> {
    let trades_path = get_trades_path(symbol, date);
    let depth_path = get_depth_path(symbol, date);
    let output_path = get_output_path(symbol, date);
    let last_output_path = get_last_output_path(symbol, date);

    // Check if input files exist
    if !Path::new(&trades_path).exists() {
        println!("  [SKIP] Trades file not found: {}", trades_path);
        return Ok(false);
    }
    if !Path::new(&depth_path).exists() {
        println!("  [SKIP] Depth file not found: {}", depth_path);
        return Ok(false);
    }

    // Create output directory
    let output_dir = get_output_dir(symbol);
    fs::create_dir_all(&output_dir)?;

    println!("  [CONVERT] {} {} ...", symbol, date);
    let start = Instant::now();

    let input_files = vec![trades_path, depth_path];
    let events = convert_and_save_fast(&input_files, output_path, config)?;

    let elapsed = start.elapsed();
    println!(
        "    -> {} events in {:.2}s ({:.0} events/sec)",
        events.len(),
        elapsed.as_secs_f64(),
        events.len() as f64 / elapsed.as_secs_f64()
    );

    // Generate last snapshot
    generate_last_snapshot(&events, &last_output_path)?;

    // Print statistics
    let mut trade_count = 0;
    let mut depth_count = 0;
    let mut snapshot_count = 0;
    let mut clear_count = 0;

    for event in &events {
        let ev_type = event.ev & 0xFF;
        match ev_type {
            2 => trade_count += 1,
            1 => depth_count += 1,
            4 => snapshot_count += 1,
            3 => clear_count += 1,
            _ => {}
        }
    }

    println!(
        "    Stats: trades={}, depth={}, snapshot={}, clear={}",
        trade_count, depth_count, snapshot_count, clear_count
    );

    Ok(true)
}

fn print_usage(program: &str) {
    println!("Tardis Batch Converter");
    println!();
    println!("Usage:");
    println!("  {} [OPTIONS]", program);
    println!();
    println!("Options:");
    println!("  --symbol <SYMBOL>    Convert specific symbol only");
    println!("  --date <DATE>        Convert specific date only (requires --symbol)");
    println!("  --list               List available symbols");
    println!("  --dry-run            Show what would be converted without running");
    println!("  --force              Force re-conversion even if output exists");
    println!("  --help               Show this help message");
    println!();
    println!("Examples:");
    println!("  {}                              # Convert all symbols", program);
    println!("  {} --symbol BTCUSDT             # Convert all dates for BTCUSDT", program);
    println!(
        "  {} --symbol BTCUSDT --date 2025-06-30  # Convert specific date",
        program
    );
    println!("  {} --list                       # List available symbols", program);
    println!("  {} --dry-run                    # Preview without converting", program);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    // Parse arguments
    let mut symbol: Option<String> = None;
    let mut date: Option<String> = None;
    let mut list_mode = false;
    let mut dry_run = false;
    let mut force = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--symbol" | "-s" => {
                if i + 1 < args.len() {
                    symbol = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("Error: --symbol requires a value");
                    return Ok(());
                }
            }
            "--date" | "-d" => {
                if i + 1 < args.len() {
                    date = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("Error: --date requires a value");
                    return Ok(());
                }
            }
            "--list" => {
                list_mode = true;
                i += 1;
            }
            "--dry-run" => {
                dry_run = true;
                i += 1;
            }
            "--force" => {
                force = true;
                i += 1;
            }
            "--help" | "-h" => {
                print_usage(&args[0]);
                return Ok(());
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                print_usage(&args[0]);
                return Ok(());
            }
        }
    }

    // List mode
    if list_mode {
        println!("Available symbols with both trades and depth data:");
        let symbols = list_symbols();
        for s in &symbols {
            let dates = get_valid_dates(s);
            println!("  {} ({} dates)", s, dates.len());
        }
        println!("\nTotal: {} symbols", symbols.len());
        return Ok(());
    }

    // Dry run mode
    if dry_run {
        println!("Dry Run - Analyzing available data...\n");
        let symbols = if let Some(ref s) = symbol {
            vec![s.clone()]
        } else {
            list_symbols()
        };

        let mut total_days = 0;
        let mut total_to_convert = 0;

        for s in &symbols {
            let dates = get_valid_dates(s);
            let to_convert: usize = dates.iter().filter(|d| !output_exists(s, d)).count();
            if !dates.is_empty() {
                println!(
                    "  {}: {} dates available, {} to convert",
                    s,
                    dates.len(),
                    to_convert
                );
                total_days += dates.len();
                total_to_convert += to_convert;
            }
        }

        println!("\n========================================");
        println!("Total symbols: {}", symbols.len());
        println!("Total dates available: {}", total_days);
        println!("Total to convert: {}", total_to_convert);
        println!("========================================");
        return Ok(());
    }

    // Configure conversion
    let config = TardisConvertConfig {
        buffer_size: 100_000_000,
        ss_buffer_size: 1_000_000,
        base_latency: 0,
        snapshot_mode: SnapshotMode::Process,
    };

    println!("========================================");
    println!("Tardis Batch Converter");
    println!("Started: {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    println!("========================================\n");

    let total_start = Instant::now();
    let mut success_count = 0;
    let mut skip_count = 0;
    let mut fail_count = 0;

    // Determine symbols to process
    let symbols = if let Some(ref s) = symbol {
        vec![s.clone()]
    } else {
        list_symbols()
    };

    let total_symbols = symbols.len();

    for (idx, s) in symbols.iter().enumerate() {
        // Determine dates to process
        let dates = if let Some(ref d) = date {
            vec![d.clone()]
        } else {
            get_valid_dates(s)
        };

        if dates.is_empty() {
            continue;
        }

        println!(
            "[{}/{}] Processing {} ({} dates)",
            idx + 1,
            total_symbols,
            s,
            dates.len()
        );

        for d in &dates {
            // Skip if already converted (unless force mode)
            if !force && output_exists(s, d) {
                println!("  [SKIP] {} {} - already converted", s, d);
                skip_count += 1;
                continue;
            }

            match convert_single_day(s, d, &config) {
                Ok(true) => {
                    println!("  [OK] {} {}", s, d);
                    success_count += 1;
                }
                Ok(false) => {
                    skip_count += 1;
                }
                Err(e) => {
                    println!("  [FAIL] {} {}: {}", s, d, e);
                    fail_count += 1;
                }
            }
        }
    }

    let total_elapsed = total_start.elapsed();

    println!("\n========================================");
    println!("Batch Conversion Complete!");
    println!("  Processed: {}", success_count);
    println!("  Skipped:   {}", skip_count);
    println!("  Failed:    {}", fail_count);
    println!("  Total time: {:.2}s", total_elapsed.as_secs_f64());
    println!("========================================");

    Ok(())
}
