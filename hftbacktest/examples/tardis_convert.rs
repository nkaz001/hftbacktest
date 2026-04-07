//! Example: Convert Tardis data files to NPZ format
//!
//! Usage:
//!   cargo run --release --example tardis_convert -- trades.csv.gz depth.csv.gz -o output.npz
//!
//! Or for testing with generated sample data:
//!   cargo run --release --example tardis_convert -- --generate-sample

use std::env;
use std::fs::File;
use std::io::Write;
use std::time::Instant;

use hftbacktest::data::{SnapshotMode, TardisConvertConfig, convert_and_save_fast};

fn generate_sample_data() -> std::io::Result<(String, String)> {
    let trades_path = "/tmp/sample_trades.csv";
    let depth_path = "/tmp/sample_depth.csv";

    // Generate sample trades
    let mut trades_file = File::create(trades_path)?;
    writeln!(
        trades_file,
        "exchange,symbol,timestamp,local_timestamp,id,side,price,amount"
    )?;

    let base_ts: i64 = 1700000000000000; // microseconds
    for i in 0..10000 {
        let ts = base_ts + i * 100;
        let local_ts = ts + 50 + (i % 100);
        let side = if i % 2 == 0 { "buy" } else { "sell" };
        let price = 50000.0 + (i as f64 % 100.0) * 0.5;
        let amount = 0.001 + (i as f64 % 10.0) * 0.001;
        writeln!(
            trades_file,
            "binance-futures,BTCUSDT,{},{},trade{},{},{:.2},{:.6}",
            ts, local_ts, i, side, price, amount
        )?;
    }

    // Generate sample depth updates
    let mut depth_file = File::create(depth_path)?;
    writeln!(
        depth_file,
        "exchange,symbol,timestamp,local_timestamp,is_snapshot,side,price,amount"
    )?;

    // First, add a snapshot
    let snapshot_ts = base_ts;
    let snapshot_local_ts = snapshot_ts + 50;
    for i in 0..100 {
        let bid_price = 50000.0 - (i as f64) * 0.5;
        let ask_price = 50000.5 + (i as f64) * 0.5;
        let amount = 1.0 + (i as f64) * 0.1;

        writeln!(
            depth_file,
            "binance-futures,BTCUSDT,{},{},true,bid,{:.2},{:.3}",
            snapshot_ts, snapshot_local_ts, bid_price, amount
        )?;
        writeln!(
            depth_file,
            "binance-futures,BTCUSDT,{},{},true,ask,{:.2},{:.3}",
            snapshot_ts, snapshot_local_ts, ask_price, amount
        )?;
    }

    // Then add incremental updates
    for i in 0..50000 {
        let ts = base_ts + 1000 + i * 50;
        let local_ts = ts + 30 + (i % 50);
        let side = if i % 2 == 0 { "bid" } else { "ask" };
        let price = if side == "bid" {
            50000.0 - (i as f64 % 50.0) * 0.5
        } else {
            50000.5 + (i as f64 % 50.0) * 0.5
        };
        let amount = (i as f64 % 10.0) * 0.5;

        writeln!(
            depth_file,
            "binance-futures,BTCUSDT,{},{},false,{},{:.2},{:.3}",
            ts, local_ts, side, price, amount
        )?;
    }

    Ok((trades_path.to_string(), depth_path.to_string()))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!(
            "Usage: {} <trades.csv.gz> <depth.csv.gz> [-o output.npz]",
            args[0]
        );
        println!("   or: {} --generate-sample", args[0]);
        return Ok(());
    }

    let (input_files, output_path) = if args[1] == "--generate-sample" {
        println!("Generating sample data...");
        let (trades, depth) = generate_sample_data()?;
        (vec![trades, depth], "/tmp/sample_output.npz".to_string())
    } else {
        let mut input_files = Vec::new();
        let mut output_path = "output.npz".to_string();
        let mut i = 1;
        while i < args.len() {
            if args[i] == "-o" && i + 1 < args.len() {
                output_path = args[i + 1].clone();
                i += 2;
            } else {
                input_files.push(args[i].clone());
                i += 1;
            }
        }
        (input_files, output_path)
    };

    let config = TardisConvertConfig {
        buffer_size: 100_000_000,
        ss_buffer_size: 1_000_000,
        base_latency: 0,
        snapshot_mode: SnapshotMode::IgnoreSod, // Match Python default behavior
    };

    println!("Converting {} files...", input_files.len());
    let start = Instant::now();

    let events = convert_and_save_fast(&input_files, output_path, &config)?;

    let elapsed = start.elapsed();
    println!(
        "Conversion complete: {} events in {:.3}s ({:.0} events/sec)",
        events.len(),
        elapsed.as_secs_f64(),
        events.len() as f64 / elapsed.as_secs_f64()
    );

    // Print some statistics
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

    println!("\nEvent breakdown:");
    println!("  Trades: {}", trade_count);
    println!("  Depth updates: {}", depth_count);
    println!("  Snapshot entries: {}", snapshot_count);
    println!("  Clear events: {}", clear_count);

    Ok(())
}
