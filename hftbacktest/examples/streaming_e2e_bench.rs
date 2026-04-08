//! End-to-end benchmark: NPZ backtest vs Streaming CSV backtest
//!
//! Compares total wall time including event processing through MarketDepth,
//! which is the real workload in a backtest.
//!
//! Usage:
//!   cargo run --release -p hftbacktest --example streaming_e2e_bench \
//!     --no-default-features --features backtest
//!   cargo run --release -p hftbacktest --example streaming_e2e_bench \
//!     --no-default-features --features backtest -- --symbol BTCUSDT --date 2026-03-05

use std::env;
use std::hint::black_box;
use std::path::Path;
use std::time::Instant;

use hftbacktest::backtest::data::{Data, DataSource, Reader, TardisStreamDayConfig, read_npz_file};
use hftbacktest::depth::{L1MarketDepth, L2MarketDepth, MarketDepth};
use hftbacktest::prelude::HashMapMarketDepth;
use hftbacktest::types::{
    BUY_EVENT, DEPTH_BBO_EVENT, DEPTH_CLEAR_EVENT, DEPTH_EVENT, DEPTH_SNAPSHOT_EVENT, Event,
    EXCH_EVENT, LOCAL_EVENT, Side, TRADE_EVENT,
};

const BASE_PATH: &str = "/mnt/data/samba/users/Data/tardis/datasets/binance-futures";

fn get_paths(symbol: &str, date: &str) -> (String, String, String, String) {
    let trades = format!("{}/trades/{}/{}.csv.gz", BASE_PATH, symbol, date);
    let depth = format!(
        "{}/incremental_book_L2/{}/{}.csv.gz",
        BASE_PATH, symbol, date
    );
    let bbo = format!("{}/book_ticker/{}/{}.csv.gz", BASE_PATH, symbol, date);
    let npz = format!(
        "{}/hft_npz/{}/{}_{}.npz",
        BASE_PATH,
        symbol,
        symbol,
        date.replace("-", "")
    );
    (trades, depth, bbo, npz)
}

/// Simulate real backtest: depth updates + strategy overhead.
/// `extra_work` controls how much fake strategy computation to add per event,
/// to bring processing rate down to realistic levels (~3.5-4 M ev/s).
fn process_events(
    data: &Data<Event>,
    depth: &mut HashMapMarketDepth,
    extra_work: u32,
) -> (u64, u64) {
    let mut trade_count = 0u64;
    let mut event_count = 0u64;
    let mut _accum = 0u64; // strategy simulation accumulator
    for i in 0..data.len() {
        let ev = &data[i];
        event_count += 1;

        // Process exchange-side events (same as partialfillexchange processor)
        if (ev.ev & EXCH_EVENT) != 0 {
            let ev_type = ev.ev & 0xFF;
            let is_bid = (ev.ev & BUY_EVENT) != 0;
            match ev_type {
                t if t == DEPTH_EVENT || t == DEPTH_SNAPSHOT_EVENT => {
                    if is_bid {
                        depth.update_bid_depth(ev.px, ev.qty, ev.exch_ts);
                    } else {
                        depth.update_ask_depth(ev.px, ev.qty, ev.exch_ts);
                    }
                }
                t if t == DEPTH_CLEAR_EVENT => {
                    let side = if is_bid { Side::Buy } else { Side::Sell };
                    depth.clear_depth(side, ev.px);
                }
                t if t == DEPTH_BBO_EVENT => {
                    if is_bid {
                        depth.update_best_bid(ev.px, ev.qty, ev.exch_ts);
                    } else {
                        depth.update_best_ask(ev.px, ev.qty, ev.exch_ts);
                    }
                }
                t if t == TRADE_EVENT => {
                    trade_count += 1;
                }
                _ => {}
            }
        }

        // Process local-side events (same as local processor)
        if (ev.ev & LOCAL_EVENT) != 0 {
            let ev_type = ev.ev & 0xFF;
            let is_bid = (ev.ev & BUY_EVENT) != 0;
            match ev_type {
                t if t == DEPTH_EVENT || t == DEPTH_SNAPSHOT_EVENT => {
                    if is_bid {
                        depth.update_bid_depth(ev.px, ev.qty, ev.local_ts);
                    } else {
                        depth.update_ask_depth(ev.px, ev.qty, ev.local_ts);
                    }
                }
                t if t == DEPTH_BBO_EVENT => {
                    if is_bid {
                        depth.update_best_bid(ev.px, ev.qty, ev.local_ts);
                    } else {
                        depth.update_best_ask(ev.px, ev.qty, ev.local_ts);
                    }
                }
                _ => {}
            }
        }

        // Simulate strategy computation overhead (order management, factor calc, etc.)
        for j in 0..extra_work {
            _accum = black_box(_accum.wrapping_add(black_box(ev.exch_ts as u64).wrapping_mul(j as u64 + 1)));
        }
    }
    black_box(_accum);
    (event_count, trade_count)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    let mut symbol = "ADAUSDT".to_string();
    let mut date = "2026-03-05".to_string();
    let mut tick_size = 0.0;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--symbol" | "-s" => {
                symbol = args[i + 1].clone();
                i += 2;
            }
            "--date" | "-d" => {
                date = args[i + 1].clone();
                i += 2;
            }
            "--tick-size" => {
                tick_size = args[i + 1].parse()?;
                i += 2;
            }
            "--help" | "-h" => {
                println!("Usage: streaming_e2e_bench [--symbol SYM] [--date YYYY-MM-DD]");
                return Ok(());
            }
            _ => {
                i += 1;
            }
        }
    }

    if tick_size == 0.0 {
        tick_size = match symbol.as_str() {
            "BTCUSDT" => 0.1,
            "ETHUSDT" => 0.01,
            "ADAUSDT" => 0.0001,
            "SOLUSDT" => 0.01,
            _ => 0.0001,
        };
    }

    let (trades_path, depth_path, bbo_path, npz_path) = get_paths(&symbol, &date);
    let has_bbo = Path::new(&bbo_path).exists();
    let has_npz = Path::new(&npz_path).exists();

    println!("=== E2E Benchmark: {} {} (tick={}) ===\n", symbol, date, tick_size);

    if !has_npz {
        eprintln!("NPZ not found: {}", npz_path);
        return Ok(());
    }

    // Step 0: Calibrate extra_work to achieve ~3.5 M ev/s processing rate
    // (realistic strategy backtest speed)
    println!("--- Calibrating strategy overhead ---");
    let target_rate = 3.5e6; // 3.5 M ev/s
    let cal_data: Data<Event> = read_npz_file(&npz_path, "data")?;
    let cal_n = cal_data.len().min(500_000); // use 500K events for calibration

    // Measure baseline (extra_work=0)
    let mut cal_depth = HashMapMarketDepth::new(tick_size, 1.0);
    let cal_slice = {
        // Create a small Data for calibration
        let mut events = Vec::with_capacity(cal_n);
        for i in 0..cal_n {
            events.push(cal_data[i].clone());
        }
        Data::from_data(&events)
    };
    let start = Instant::now();
    process_events(&cal_slice, &mut cal_depth, 0);
    let baseline_rate = cal_n as f64 / start.elapsed().as_secs_f64();
    println!("  Baseline (no extra work): {:.2} M ev/s", baseline_rate / 1e6);

    // Binary search for extra_work that achieves target rate
    let mut lo = 0u32;
    let mut hi = 5000u32;
    while lo < hi {
        let mid = (lo + hi) / 2;
        let mut d = HashMapMarketDepth::new(tick_size, 1.0);
        let start = Instant::now();
        process_events(&cal_slice, &mut d, mid);
        let rate = cal_n as f64 / start.elapsed().as_secs_f64();
        if rate > target_rate {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    let extra_work = lo;

    let mut cal_depth2 = HashMapMarketDepth::new(tick_size, 1.0);
    let start = Instant::now();
    process_events(&cal_slice, &mut cal_depth2, extra_work);
    let actual_rate = cal_n as f64 / start.elapsed().as_secs_f64();
    println!(
        "  Calibrated: extra_work={}, rate={:.2} M ev/s (target: {:.1} M)\n",
        extra_work,
        actual_rate / 1e6,
        target_rate / 1e6,
    );
    drop(cal_data);
    drop(cal_slice);

    // === Run with different workloads ===
    for (label, work) in [
        ("LIGHT (depth only)", 0),
        ("REALISTIC (strategy ~3.5M ev/s)", extra_work),
    ] {
        println!("========== {} ==========\n", label);

        // NPZ backtest
        let npz_start = Instant::now();
        let mut npz_reader = Reader::builder()
            .parallel_load(true)
            .data(vec![DataSource::File(npz_path.clone())])
            .build()?;
        let mut npz_depth = HashMapMarketDepth::new(tick_size, 1.0);
        let mut npz_events = 0u64;
        let mut npz_trades = 0u64;
        loop {
            match npz_reader.next_data() {
                Ok(data) => {
                    let (e, t) = process_events(&data, &mut npz_depth, work);
                    npz_events += e;
                    npz_trades += t;
                    npz_reader.release(data);
                }
                Err(hftbacktest::backtest::BacktestError::EndOfData) => break,
                Err(e) => return Err(e.into()),
            }
        }
        let npz_elapsed = npz_start.elapsed();

        // Streaming CSV backtest
        let stream_start = Instant::now();
        let stream_day = TardisStreamDayConfig {
            trades: trades_path.clone(),
            depth: depth_path.clone(),
            book_ticker: has_bbo.then_some(bbo_path.clone()),
        };
        let mut stream_reader = Reader::builder()
            .parallel_load(true)
            .data(vec![DataSource::TardisStream(stream_day)])
            .build()?;
        let mut stream_depth = HashMapMarketDepth::new(tick_size, 1.0);
        let mut stream_events = 0u64;
        let mut stream_trades = 0u64;
        loop {
            match stream_reader.next_data() {
                Ok(data) => {
                    let (e, t) = process_events(&data, &mut stream_depth, work);
                    stream_events += e;
                    stream_trades += t;
                    stream_reader.release(data);
                }
                Err(hftbacktest::backtest::BacktestError::EndOfData) => break,
                Err(e) => return Err(e.into()),
            }
        }
        let stream_elapsed = stream_start.elapsed();

        let ratio = stream_elapsed.as_secs_f64() / npz_elapsed.as_secs_f64();
        let consistent = npz_events == stream_events
            && npz_trades == stream_trades
            && npz_depth.best_bid_tick() == stream_depth.best_bid_tick();

        println!(
            "  {:<25} {:>8.3}s  {:>6.2} M ev/s",
            "NPZ",
            npz_elapsed.as_secs_f64(),
            npz_events as f64 / npz_elapsed.as_secs_f64() / 1e6,
        );
        println!(
            "  {:<25} {:>8.3}s  {:>6.2} M ev/s",
            "Streaming CSV",
            stream_elapsed.as_secs_f64(),
            stream_events as f64 / stream_elapsed.as_secs_f64() / 1e6,
        );
        println!(
            "  Ratio: {:.2}x  |  Overhead: {:.1}%  |  Consistent: {}",
            ratio,
            (ratio - 1.0) * 100.0,
            if consistent { "YES" } else { "NO" },
        );
        println!(
            "  Memory: NPZ ~{:.0} MB  |  Streaming ~7 MB\n",
            npz_events as f64 * 64.0 / 1024.0 / 1024.0,
        );
    }

    Ok(())
}
