//! Convert Tardis data files to NPZ format for HFT backtest
//!
//! Usage:
//!   cargo run --release --example tardis_convert -- --symbol BTCUSDT --date 2024-01-01
//!   cargo run --release --example tardis_convert -- --symbol ETHUSDT --date 2024-01-01 --date 2024-01-02
//!   cargo run --release --example tardis_convert -- --symbol BTCUSDT --start-date 2024-01-01 --end-date 2024-01-31

use std::env;
use std::fs;
use std::path::Path;
use std::time::Instant;

use hftbacktest::data::{SnapshotMode, TardisConvertConfig, convert_and_save_fast};

const BASE_PATH: &str = "/mnt/data/samba/users/Data/tardis/datasets/binance-futures";

fn get_trades_path(symbol: &str, date: &str) -> String {
    format!("{}/trades/{}/{}.csv.gz", BASE_PATH, symbol, date)
}

fn get_depth_path(symbol: &str, date: &str) -> String {
    format!(
        "{}/incremental_book_L2/{}/{}.csv.gz",
        BASE_PATH, symbol, date
    )
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

fn generate_date_range(start_date: &str, end_date: &str) -> Vec<String> {
    use chrono::{Duration, NaiveDate};

    let start = NaiveDate::parse_from_str(start_date, "%Y-%m-%d")
        .expect("Invalid start date format, expected YYYY-MM-DD");
    let end = NaiveDate::parse_from_str(end_date, "%Y-%m-%d")
        .expect("Invalid end date format, expected YYYY-MM-DD");

    let mut dates = Vec::new();
    let mut current = start;
    while current <= end {
        dates.push(current.format("%Y-%m-%d").to_string());
        current = current + Duration::days(1);
    }
    dates
}

fn convert_single_day(
    symbol: &str,
    date: &str,
    config: &TardisConvertConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let trades_path = get_trades_path(symbol, date);
    let depth_path = get_depth_path(symbol, date);
    let output_path = get_output_path(symbol, date);
    let last_output_path = get_last_output_path(symbol, date);

    // Check if input files exist
    if !Path::new(&trades_path).exists() {
        println!("  [SKIP] Trades file not found: {}", trades_path);
        return Ok(());
    }
    if !Path::new(&depth_path).exists() {
        println!("  [SKIP] Depth file not found: {}", depth_path);
        return Ok(());
    }

    // Create output directory if not exists
    let output_dir = get_output_dir(symbol);
    fs::create_dir_all(&output_dir)?;

    println!("  Converting {} {} ...", symbol, date);
    let start = Instant::now();

    let input_files = vec![trades_path, depth_path];

    // Convert and save main npz file
    let events = convert_and_save_fast(&input_files, output_path, config)?;

    let elapsed = start.elapsed();
    println!(
        "    -> {} events in {:.3}s ({:.0} events/sec)",
        events.len(),
        elapsed.as_secs_f64(),
        events.len() as f64 / elapsed.as_secs_f64()
    );

    // Generate _last.npz (snapshot of last orderbook state)
    // This extracts the final state from the events
    generate_last_snapshot(&events, &last_output_path)?;
    println!("    -> Generated {}", last_output_path);

    // Print statistics
    let mut trade_count = 0;
    let mut depth_count = 0;
    let mut snapshot_count = 0;
    let mut clear_count = 0;

    for event in &events {
        let ev_type = event.ev & 0xFF;
        match ev_type {
            2 => trade_count += 1,    // TRADE_EVENT
            1 => depth_count += 1,    // DEPTH_EVENT
            4 => snapshot_count += 1, // DEPTH_SNAPSHOT_EVENT
            3 => clear_count += 1,    // DEPTH_CLEAR_EVENT
            _ => {}
        }
    }

    println!(
        "    Stats: trades={}, depth={}, snapshot={}, clear={}",
        trade_count, depth_count, snapshot_count, clear_count
    );

    Ok(())
}

fn generate_last_snapshot(
    events: &[hftbacktest::types::Event],
    output_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use hftbacktest::data::write_npz_file;
    use hftbacktest::types::{BUY_EVENT, DEPTH_EVENT, DEPTH_SNAPSHOT_EVENT, Event, SELL_EVENT};
    use std::collections::BTreeMap;

    // Build final orderbook state from events
    let mut bids: BTreeMap<i64, f64> = BTreeMap::new(); // price (as i64) -> qty
    let mut asks: BTreeMap<i64, f64> = BTreeMap::new();

    let mut last_timestamp = 0i64;
    let mut last_local_timestamp = 0i64;

    for event in events {
        let ev_type = event.ev & 0xFF;
        if ev_type == DEPTH_EVENT || ev_type == DEPTH_SNAPSHOT_EVENT {
            let price_key = (event.px * 1e8) as i64; // Use fixed point for price key
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

    // Convert to snapshot events
    let mut snapshot_events: Vec<Event> = Vec::new();

    // Add bids (sorted by price descending - highest first)
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

    // Add asks (sorted by price ascending - lowest first)
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

    // Save as NPZ using the same format as main file
    write_npz_file(&snapshot_events, output_path).map_err(|e| {
        Box::new(std::io::Error::new(std::io::ErrorKind::Other, e)) as Box<dyn std::error::Error>
    })
}

fn print_usage(program: &str) {
    println!("Usage:");
    println!("  {} --symbol <SYMBOL> --date <DATE>", program);
    println!(
        "  {} --symbol <SYMBOL> --date <DATE1> --date <DATE2> ...",
        program
    );
    println!(
        "  {} --symbol <SYMBOL> --start-date <START> --end-date <END>",
        program
    );
    println!();
    println!("Examples:");
    println!("  {} --symbol BTCUSDT --date 2024-01-01", program);
    println!(
        "  {} --symbol ETHUSDT --start-date 2024-01-01 --end-date 2024-01-31",
        program
    );
    println!();
    println!("Input:");
    println!("  trades: {}/trades/<SYMBOL>/<DATE>.csv.gz", BASE_PATH);
    println!(
        "  depth:  {}/incremental_book_L2/<SYMBOL>/<DATE>.csv.gz",
        BASE_PATH
    );
    println!();
    println!("Output:");
    println!("  {}/hft_npz/<SYMBOL>/<SYMBOL>_<DATE>.npz", BASE_PATH);
    println!("  {}/hft_npz/<SYMBOL>/<SYMBOL>_<DATE>_last.npz", BASE_PATH);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage(&args[0]);
        return Ok(());
    }

    // Parse arguments
    let mut symbol: Option<String> = None;
    let mut dates: Vec<String> = Vec::new();
    let mut start_date: Option<String> = None;
    let mut end_date: Option<String> = None;

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
                    dates.push(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("Error: --date requires a value");
                    return Ok(());
                }
            }
            "--start-date" => {
                if i + 1 < args.len() {
                    start_date = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("Error: --start-date requires a value");
                    return Ok(());
                }
            }
            "--end-date" => {
                if i + 1 < args.len() {
                    end_date = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("Error: --end-date requires a value");
                    return Ok(());
                }
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

    // Validate arguments
    let symbol = match symbol {
        Some(s) => s,
        None => {
            eprintln!("Error: --symbol is required");
            print_usage(&args[0]);
            return Ok(());
        }
    };

    // Build date list
    if let (Some(start), Some(end)) = (start_date, end_date) {
        dates = generate_date_range(&start, &end);
    }

    if dates.is_empty() {
        eprintln!("Error: At least one --date or --start-date/--end-date is required");
        print_usage(&args[0]);
        return Ok(());
    }

    // Configure conversion
    let config = TardisConvertConfig {
        buffer_size: 100_000_000,
        ss_buffer_size: 1_000_000,
        base_latency: 0,
        snapshot_mode: SnapshotMode::Process, // Keep all snapshots
    };

    println!("========================================");
    println!("Tardis to NPZ Converter");
    println!("Symbol: {}", symbol);
    println!("Dates: {} day(s)", dates.len());
    println!("========================================\n");

    let total_start = Instant::now();
    let mut success_count = 0;
    let mut skip_count = 0;

    for date in &dates {
        match convert_single_day(&symbol, date, &config) {
            Ok(_) => success_count += 1,
            Err(e) => {
                println!("  [ERROR] {} {}: {}", symbol, date, e);
                skip_count += 1;
            }
        }
    }

    let total_elapsed = total_start.elapsed();
    println!("\n========================================");
    println!("Conversion complete!");
    println!("  Success: {}", success_count);
    println!("  Skipped: {}", skip_count);
    println!("  Total time: {:.2}s", total_elapsed.as_secs_f64());
    println!("========================================");

    Ok(())
}
