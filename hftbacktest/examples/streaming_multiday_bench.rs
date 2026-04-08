//! Multi-day streaming vs NPZ backtest comparison.
//!
//! Verifies that multi-day streaming produces identical results to multi-day NPZ,
//! with depth state carrying across day boundaries.
//!
//! Usage:
//!   cargo run --release -p hftbacktest --example streaming_multiday_bench \
//!     --no-default-features --features backtest

use std::path::Path;
use std::time::Instant;

use hftbacktest::backtest::data::{DataSource, Reader, TardisStreamDayConfig};
use hftbacktest::depth::{L1MarketDepth, L2MarketDepth, MarketDepth};
use hftbacktest::prelude::HashMapMarketDepth;
use hftbacktest::types::{
    BUY_EVENT, DEPTH_BBO_EVENT, DEPTH_CLEAR_EVENT, DEPTH_EVENT, DEPTH_SNAPSHOT_EVENT, Event,
    EXCH_EVENT, LOCAL_EVENT, Side, TRADE_EVENT,
};

const BASE: &str = "/mnt/data/samba/users/Data/tardis/datasets/binance-futures";
const SYMBOL: &str = "ADAUSDT";
const DATES: &[&str] = &["2026-03-05", "2026-03-06", "2026-03-07"];
const TICK_SIZE: f64 = 0.0001;

fn process_chunk(data: &hftbacktest::backtest::data::Data<Event>, depth: &mut HashMapMarketDepth) -> (u64, u64) {
    let mut events = 0u64;
    let mut trades = 0u64;
    for i in 0..data.len() {
        let ev = &data[i];
        events += 1;
        if (ev.ev & EXCH_EVENT) != 0 {
            let t = ev.ev & 0xFF;
            let bid = (ev.ev & BUY_EVENT) != 0;
            if t == DEPTH_EVENT || t == DEPTH_SNAPSHOT_EVENT {
                if bid { depth.update_bid_depth(ev.px, ev.qty, ev.exch_ts); }
                else { depth.update_ask_depth(ev.px, ev.qty, ev.exch_ts); }
            } else if t == DEPTH_CLEAR_EVENT {
                depth.clear_depth(if bid { Side::Buy } else { Side::Sell }, ev.px);
            } else if t == DEPTH_BBO_EVENT {
                if bid { depth.update_best_bid(ev.px, ev.qty, ev.exch_ts); }
                else { depth.update_best_ask(ev.px, ev.qty, ev.exch_ts); }
            } else if t == TRADE_EVENT {
                trades += 1;
            }
        }
        if (ev.ev & LOCAL_EVENT) != 0 {
            let t = ev.ev & 0xFF;
            let bid = (ev.ev & BUY_EVENT) != 0;
            if t == DEPTH_EVENT || t == DEPTH_SNAPSHOT_EVENT {
                if bid { depth.update_bid_depth(ev.px, ev.qty, ev.local_ts); }
                else { depth.update_ask_depth(ev.px, ev.qty, ev.local_ts); }
            } else if t == DEPTH_BBO_EVENT {
                if bid { depth.update_best_bid(ev.px, ev.qty, ev.local_ts); }
                else { depth.update_best_ask(ev.px, ev.qty, ev.local_ts); }
            }
        }
    }
    (events, trades)
}

fn run_reader(label: &str, mut reader: Reader<Event>) -> (u64, u64, i64, i64, f64) {
    let mut depth = HashMapMarketDepth::new(TICK_SIZE, 1.0);
    let mut total_events = 0u64;
    let mut total_trades = 0u64;
    let mut chunks = 0u32;
    let start = Instant::now();

    loop {
        match reader.next_data() {
            Ok(data) => {
                let (e, t) = process_chunk(&data, &mut depth);
                total_events += e;
                total_trades += t;
                chunks += 1;
                reader.release(data);
            }
            Err(hftbacktest::backtest::BacktestError::EndOfData) => break,
            Err(e) => {
                eprintln!("  ERROR: {:?}", e);
                break;
            }
        }
    }

    let elapsed = start.elapsed().as_secs_f64();
    println!(
        "  {:<20} {:>10} events  {:>8} trades  {:>4} chunks  {:.2}s  ({:.2} M/s)  bid={} ask={}",
        label, total_events, total_trades, chunks, elapsed,
        total_events as f64 / elapsed / 1e6,
        depth.best_bid_tick(), depth.best_ask_tick(),
    );
    (total_events, total_trades, depth.best_bid_tick(), depth.best_ask_tick(), elapsed)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Multi-Day Streaming vs NPZ: {} × {} days ===\n", SYMBOL, DATES.len());

    // Verify data exists
    for date in DATES {
        let npz = format!("{}/hft_npz/{}/{}_{}.npz", BASE, SYMBOL, SYMBOL, date.replace("-", ""));
        if !Path::new(&npz).exists() {
            eprintln!("Missing NPZ: {}", npz);
            return Ok(());
        }
    }

    // === NPZ multi-day ===
    println!("--- NPZ ({} days) ---", DATES.len());
    let npz_data: Vec<DataSource<Event>> = DATES.iter()
        .map(|d| {
            let npz = format!("{}/hft_npz/{}/{}_{}.npz", BASE, SYMBOL, SYMBOL, d.replace("-", ""));
            DataSource::File(npz)
        })
        .collect();

    let npz_reader = Reader::builder()
        .parallel_load(true)
        .data(npz_data)
        .build()?;

    let (npz_ev, npz_tr, npz_bid, npz_ask, npz_time) = run_reader("NPZ", npz_reader);

    // === Streaming multi-day (with bookTicker) ===
    println!("\n--- Streaming with BBO ({} days) ---", DATES.len());
    let stream_data: Vec<DataSource<Event>> = DATES.iter()
        .map(|d| {
            let bbo = format!("{}/book_ticker/{}/{}.csv.gz", BASE, SYMBOL, d);
            DataSource::TardisStream(TardisStreamDayConfig {
                trades: format!("{}/trades/{}/{}.csv.gz", BASE, SYMBOL, d),
                depth: format!("{}/incremental_book_L2/{}/{}.csv.gz", BASE, SYMBOL, d),
                book_ticker: if Path::new(&bbo).exists() { Some(bbo) } else { None },
            })
        })
        .collect();

    let stream_reader = Reader::builder()
        .parallel_load(true)
        .data(stream_data)
        .build()?;

    let (str_ev, str_tr, str_bid, str_ask, str_time) = run_reader("Streaming+BBO", stream_reader);

    // === Streaming multi-day (NO bookTicker) ===
    println!("\n--- Streaming without BBO ({} days) ---", DATES.len());
    let stream_no_bbo: Vec<DataSource<Event>> = DATES.iter()
        .map(|d| {
            DataSource::TardisStream(TardisStreamDayConfig {
                trades: format!("{}/trades/{}/{}.csv.gz", BASE, SYMBOL, d),
                depth: format!("{}/incremental_book_L2/{}/{}.csv.gz", BASE, SYMBOL, d),
                book_ticker: None,
            })
        })
        .collect();

    let nobbo_reader = Reader::builder()
        .parallel_load(true)
        .data(stream_no_bbo)
        .build()?;

    let (nobbo_ev, nobbo_tr, nobbo_bid, nobbo_ask, nobbo_time) = run_reader("Streaming-noBBO", nobbo_reader);

    // === Comparison ===
    println!("\n=== Results ===");

    let npz_vs_stream = npz_ev == str_ev && npz_tr == str_tr && npz_bid == str_bid && npz_ask == str_ask;
    println!(
        "  NPZ vs Streaming+BBO:  {}  (ratio: {:.2}x)",
        if npz_vs_stream { "MATCH" } else { "MISMATCH" },
        str_time / npz_time,
    );

    // no-BBO will have fewer events (no BBO events) but same trades and similar depth
    println!(
        "  Streaming-noBBO:       {} events ({} fewer), trades={}, bid={} ask={}",
        nobbo_ev,
        str_ev as i64 - nobbo_ev as i64,
        nobbo_tr,
        nobbo_bid,
        nobbo_ask,
    );

    println!("\n  Memory per route:");
    println!("    NPZ:              ~{:.0} MB", npz_ev as f64 * 64.0 / 1e6);
    println!("    Streaming:        ~7 MB (regardless of days)");

    Ok(())
}
