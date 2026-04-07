//! Convert collector data and validate.
//!
//! Usage:
//!   cargo run --release --example convert_collector_data -- <input_file> [output_npz]

use std::collections::BTreeMap;
use std::env;
use std::path::PathBuf;

use hftbacktest::data::{CollectorConvertConfig, convert_collector_to_events};
use hftbacktest::types::{
    BUY_EVENT, DEPTH_CLEAR_EVENT, DEPTH_EVENT, DEPTH_SNAPSHOT_EVENT, SELL_EVENT, TRADE_EVENT,
};

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <input_file.gz> [output.npz]", args[0]);
        return Err("Missing input file".to_string());
    }

    let input_file = PathBuf::from(&args[1]);
    let output_file = if args.len() > 2 {
        Some(PathBuf::from(&args[2]))
    } else {
        None
    };

    println!("Input: {:?}", input_file);
    if let Some(ref out) = output_file {
        println!("Output: {:?}", out);
    }
    println!();

    // Convert
    let config = CollectorConvertConfig::default();
    let (events, stats) =
        convert_collector_to_events(&[&input_file], output_file.as_ref(), &config)?;

    println!("\n=== Conversion Stats ===");
    println!("Trades: {}", stats.trades);
    println!("Depth updates: {}", stats.depth_updates);
    println!("Snapshot entries: {}", stats.snapshot_entries);
    println!("Book ticker (skipped): {}", stats.book_ticker_count);
    println!("Errors: {}", stats.errors);

    println!("\n=== Event Stats ===");
    println!("Total events: {}", events.len());

    // Count event types
    let mut trade_count = 0;
    let mut depth_count = 0;
    let mut snapshot_count = 0;
    let mut clear_count = 0;
    let mut buy_count = 0;
    let mut sell_count = 0;
    let mut min_price = f64::INFINITY;
    let mut max_price = f64::NEG_INFINITY;

    for event in &events {
        if event.ev & TRADE_EVENT != 0 {
            trade_count += 1;
            if event.px > 0.0 {
                min_price = min_price.min(event.px);
                max_price = max_price.max(event.px);
            }
        }
        if event.ev & DEPTH_EVENT != 0 {
            depth_count += 1;
        }
        if event.ev & DEPTH_SNAPSHOT_EVENT != 0 {
            snapshot_count += 1;
        }
        if event.ev & DEPTH_CLEAR_EVENT != 0 {
            clear_count += 1;
        }
        if event.ev & BUY_EVENT != 0 {
            buy_count += 1;
        }
        if event.ev & SELL_EVENT != 0 {
            sell_count += 1;
        }
    }

    println!("Trade events: {}", trade_count);
    println!("Depth events: {}", depth_count);
    println!("Snapshot events: {}", snapshot_count);
    println!("Clear events: {}", clear_count);
    println!("Buy side: {}", buy_count);
    println!("Sell side: {}", sell_count);

    if trade_count > 0 && min_price.is_finite() {
        println!("\n=== Price Range (trades) ===");
        println!("Min: {:.2}", min_price);
        println!("Max: {:.2}", max_price);
    }

    // Time range
    if !events.is_empty() {
        let first = &events[0];
        let last = &events[events.len() - 1];
        let duration_ns = last.exch_ts - first.exch_ts;
        let duration_sec = duration_ns as f64 / 1_000_000_000.0;

        println!("\n=== Time Range ===");
        println!("Duration: {:.2} seconds", duration_sec);

        // Latency stats
        let latencies: Vec<i64> = events.iter().map(|e| e.local_ts - e.exch_ts).collect();

        let min_latency = *latencies.iter().min().unwrap();
        let max_latency = *latencies.iter().max().unwrap();
        let avg_latency: f64 = latencies.iter().sum::<i64>() as f64 / latencies.len() as f64;

        println!("\n=== Latency Stats ===");
        println!("Min: {:.3} ms", min_latency as f64 / 1_000_000.0);
        println!("Max: {:.3} ms", max_latency as f64 / 1_000_000.0);
        println!("Avg: {:.3} ms", avg_latency / 1_000_000.0);
    }

    // Validate orderbook consistency
    println!("\n=== Orderbook Validation ===");
    let mut bids: BTreeMap<i64, f64> = BTreeMap::new();
    let mut asks: BTreeMap<i64, f64> = BTreeMap::new();
    let price_to_key = |price: f64| -> i64 { (price * 100.0) as i64 };

    let mut consistency_errors = 0;
    let mut checks = 0;

    for event in &events {
        let is_bid = event.ev & BUY_EVENT != 0;
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

            checks += 1;
            if checks % 1000 == 0 && !bids.is_empty() && !asks.is_empty() {
                let best_bid = *bids.keys().last().unwrap();
                let best_ask = *asks.keys().next().unwrap();
                if best_bid >= best_ask {
                    consistency_errors += 1;
                }
            }
        }
    }

    println!("Checks performed: {}", checks / 1000);
    println!("Consistency errors: {}", consistency_errors);

    if consistency_errors == 0 {
        println!("\n✓ Data validation passed!");
    } else {
        println!("\n✗ Found {} consistency errors", consistency_errors);
    }

    Ok(())
}
