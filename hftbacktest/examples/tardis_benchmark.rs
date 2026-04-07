//! Benchmark: Tardis converter performance
//!
//! Usage:
//!   cargo run --release --example tardis_benchmark -- trades.csv.gz depth.csv.gz [book_ticker.csv.gz]

use std::env;
use std::time::Instant;

use hftbacktest::data::{SnapshotMode, TardisConvertConfig, convert_tardis};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        println!(
            "Usage: {} <trades.csv.gz> <depth.csv.gz> [book_ticker.csv.gz]",
            args[0]
        );
        return Ok(());
    }

    let input_files: Vec<&str> = args[1..].iter().map(|s| s.as_str()).collect();

    let config = TardisConvertConfig {
        buffer_size: 100_000_000,
        ss_buffer_size: 1_000_000,
        base_latency: 0,
        snapshot_mode: SnapshotMode::IgnoreSod,
    };

    println!("=== Tardis Converter Benchmark ===\n");
    println!("Input files: {:?}\n", input_files);

    let start = Instant::now();
    let events = convert_tardis(&input_files, &config)?;
    let elapsed = start.elapsed();
    let rate = events.len() as f64 / elapsed.as_secs_f64();
    println!(
        "  Events: {}\n  Time: {:.3}s\n  Rate: {:.0} events/sec\n",
        events.len(),
        elapsed.as_secs_f64(),
        rate
    );

    Ok(())
}
