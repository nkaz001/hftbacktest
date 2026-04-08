//! Consistency test: compares events from CSV.gz conversion vs NPZ load.
//!
//! Establishes the ground truth baseline that the streaming reader must reproduce.
//! Also runs a quick backtest with both data sources to compare depth state.
//!
//! Usage:
//!   cargo run --release -p hftbacktest --example streaming_equivalence \
//!     --no-default-features --features backtest
//!   cargo run --release -p hftbacktest --example streaming_equivalence \
//!     --no-default-features --features backtest -- --symbol BTCUSDT --date 2026-03-05

use std::env;
use std::path::Path;
use std::time::Instant;

use hftbacktest::backtest::data::{DataSource, Reader, TardisStreamDayConfig, read_npz_file};
use hftbacktest::data::{
    SnapshotMode, StreamingTardisReaderConfig, TardisConvertConfig, collect_tardis_stream,
    convert_tardis_fast,
};
use hftbacktest::depth::{L1MarketDepth, L2MarketDepth, MarketDepth};
use hftbacktest::prelude::HashMapMarketDepth;
use hftbacktest::types::{
    BUY_EVENT, DEPTH_BBO_EVENT, DEPTH_CLEAR_EVENT, DEPTH_EVENT, DEPTH_SNAPSHOT_EVENT, Event,
    EXCH_EVENT, Side, TRADE_EVENT,
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

fn event_type_name(ev: u64) -> &'static str {
    match ev & 0xFF {
        1 => "DEPTH",
        2 => "TRADE",
        3 => "CLEAR",
        4 => "SNAPSHOT",
        5 => "BBO",
        _ => "OTHER",
    }
}

fn compare_events(csv_events: &[Event], npz_events: &[Event]) -> bool {
    println!("\n=== Event-by-Event Comparison ===\n");

    if csv_events.len() != npz_events.len() {
        println!(
            "  LENGTH MISMATCH: csv={} npz={}",
            csv_events.len(),
            npz_events.len()
        );

        // Find where they diverge
        let min_len = csv_events.len().min(npz_events.len());
        let mut first_diff = min_len;
        for i in 0..min_len {
            if !events_equal(&csv_events[i], &npz_events[i]) {
                first_diff = i;
                break;
            }
        }
        println!("  First divergence at index: {}", first_diff);
        if first_diff < min_len {
            print_event_diff(first_diff, &csv_events[first_diff], &npz_events[first_diff]);
        }
        return false;
    }

    let n = csv_events.len();
    let mut mismatches = 0;
    let mut first_mismatch = None;

    for i in 0..n {
        if !events_equal(&csv_events[i], &npz_events[i]) {
            mismatches += 1;
            if first_mismatch.is_none() {
                first_mismatch = Some(i);
            }
            if mismatches <= 5 {
                print_event_diff(i, &csv_events[i], &npz_events[i]);
            }
        }
    }

    if mismatches == 0 {
        println!("  MATCH: all {} events are identical", n);
        true
    } else {
        println!(
            "\n  MISMATCH: {}/{} events differ (first at index {})",
            mismatches,
            n,
            first_mismatch.unwrap()
        );
        false
    }
}

fn events_equal(a: &Event, b: &Event) -> bool {
    a.ev == b.ev
        && a.exch_ts == b.exch_ts
        && a.local_ts == b.local_ts
        && (a.px == b.px || (a.px.is_nan() && b.px.is_nan()))
        && a.qty == b.qty
        && a.order_id == b.order_id
}

fn print_event_diff(idx: usize, csv: &Event, npz: &Event) {
    println!("  [{}]:", idx);
    if csv.ev != npz.ev {
        println!(
            "    ev:       csv=0x{:016x} ({})  npz=0x{:016x} ({})",
            csv.ev,
            event_type_name(csv.ev),
            npz.ev,
            event_type_name(npz.ev)
        );
    }
    if csv.exch_ts != npz.exch_ts {
        println!(
            "    exch_ts:  csv={}  npz={}  (diff={}ns)",
            csv.exch_ts,
            npz.exch_ts,
            csv.exch_ts - npz.exch_ts
        );
    }
    if csv.local_ts != npz.local_ts {
        println!(
            "    local_ts: csv={}  npz={}  (diff={}ns)",
            csv.local_ts,
            npz.local_ts,
            csv.local_ts - npz.local_ts
        );
    }
    if csv.px != npz.px {
        println!("    px:       csv={}  npz={}", csv.px, npz.px);
    }
    if csv.qty != npz.qty {
        println!("    qty:      csv={}  npz={}", csv.qty, npz.qty);
    }
}

/// Replay exchange-side events through HashMapMarketDepth and return final state.
fn replay_depth(events: &[Event], tick_size: f64, lot_size: f64) -> DepthSnapshot {
    let mut depth = HashMapMarketDepth::new(tick_size, lot_size);
    let mut trade_count = 0u64;
    let mut trade_volume = 0.0f64;

    for ev in events {
        // Only process exchange-side events
        if (ev.ev & EXCH_EVENT) == 0 {
            continue;
        }
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
                trade_volume += ev.px * ev.qty;
            }
            _ => {}
        }
    }

    let best_bid = depth.best_bid_tick();
    let best_ask = depth.best_ask_tick();
    let mid = (best_bid as f64 + best_ask as f64) / 2.0 * tick_size;
    DepthSnapshot {
        best_bid_tick: best_bid,
        best_ask_tick: best_ask,
        mid_price: mid,
        trade_count,
        trade_volume,
    }
}

struct DepthSnapshot {
    best_bid_tick: i64,
    best_ask_tick: i64,
    mid_price: f64,
    trade_count: u64,
    trade_volume: f64,
}

fn print_event_stats(label: &str, events: &[Event]) {
    let mut counts = [0usize; 6];
    let mut exch_count = 0usize;
    let mut local_count = 0usize;
    let mut both_count = 0usize;
    let mut min_exch_ts = i64::MAX;
    let mut max_exch_ts = i64::MIN;

    for ev in events {
        let t = (ev.ev & 0xFF) as usize;
        if t < 6 {
            counts[t] += 1;
        }
        let has_exch = (ev.ev & EXCH_EVENT) != 0;
        let has_local = (ev.ev & (1 << 30)) != 0; // LOCAL_EVENT
        match (has_exch, has_local) {
            (true, true) => both_count += 1,
            (true, false) => exch_count += 1,
            (false, true) => local_count += 1,
            (false, false) => {}
        }
        if ev.exch_ts < min_exch_ts {
            min_exch_ts = ev.exch_ts;
        }
        if ev.exch_ts > max_exch_ts {
            max_exch_ts = ev.exch_ts;
        }
    }

    println!("  {} ({} events):", label, events.len());
    for t in 1..=5 {
        if counts[t] > 0 {
            println!("    {:<12} {}", event_type_name(t as u64), counts[t]);
        }
    }
    println!(
        "    flags: EXCH_ONLY={} LOCAL_ONLY={} BOTH={}",
        exch_count, local_count, both_count
    );
    if min_exch_ts < i64::MAX {
        let span_sec = (max_exch_ts - min_exch_ts) as f64 / 1_000_000_000.0;
        println!("    time span: {:.1}s ({:.1}h)", span_sec, span_sec / 3600.0);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    let mut symbol = "ADAUSDT".to_string();
    let mut date = "2026-03-05".to_string();
    let mut tick_size = 0.0001; // ADAUSDT default

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
                println!(
                    "Usage: streaming_equivalence [--symbol SYM] [--date YYYY-MM-DD] [--tick-size F]"
                );
                return Ok(());
            }
            _ => {
                i += 1;
            }
        }
    }

    // Auto-detect tick_size for common symbols
    if tick_size == 0.0001 {
        tick_size = match symbol.as_str() {
            "BTCUSDT" => 0.1,
            "ETHUSDT" => 0.01,
            "ADAUSDT" => 0.0001,
            "SOLUSDT" => 0.01,
            _ => 0.0001,
        };
    }

    let (trades_path, depth_path, bbo_path, npz_path) = get_paths(&symbol, &date);

    println!("=== Streaming Equivalence Test: {} {} ===\n", symbol, date);

    // Check all files exist
    if !Path::new(&trades_path).exists() || !Path::new(&depth_path).exists() {
        eprintln!("ERROR: trades or depth file missing");
        return Ok(());
    }
    if !Path::new(&npz_path).exists() {
        eprintln!("ERROR: NPZ file missing at {}", npz_path);
        eprintln!("Run tardis_batch_convert first to generate it.");
        return Ok(());
    }

    let has_bbo = Path::new(&bbo_path).exists();

    // === Step 1: Convert from CSV.gz ===
    println!("Step 1: Converting from CSV.gz...");
    let config = TardisConvertConfig {
        buffer_size: 100_000_000,
        ss_buffer_size: 1_000_000,
        base_latency: 0,
        snapshot_mode: SnapshotMode::Process,
    };

    let mut input_files = vec![trades_path.clone(), depth_path.clone()];
    if has_bbo {
        input_files.push(bbo_path.clone());
    }

    let start = Instant::now();
    let csv_events = convert_tardis_fast(&input_files, &config)?;
    let csv_time = start.elapsed();
    println!(
        "  Done: {} events in {:.2}s\n",
        csv_events.len(),
        csv_time.as_secs_f64()
    );

    // === Step 2: Stream from CSV.gz ===
    println!("Step 2: Streaming from CSV.gz...");
    let start = Instant::now();
    let streaming_events = collect_tardis_stream(
        TardisStreamDayConfig {
            trades: trades_path.clone(),
            depth: depth_path.clone(),
            book_ticker: has_bbo.then_some(bbo_path.clone()),
        },
        StreamingTardisReaderConfig::default(),
    )?;
    let streaming_time = start.elapsed();
    println!(
        "  Done: {} events in {:.2}s\n",
        streaming_events.len(),
        streaming_time.as_secs_f64()
    );

    // === Step 3: Load from NPZ ===
    println!("Step 3: Loading from NPZ...");
    let start = Instant::now();
    let npz_data: hftbacktest::backtest::data::Data<Event> = read_npz_file(&npz_path, "data")?;
    let npz_time = start.elapsed();

    // Copy NPZ data into a Vec for comparison
    let npz_events: Vec<Event> = (0..npz_data.len()).map(|i| npz_data[i].clone()).collect();
    println!(
        "  Done: {} events in {:.2}s\n",
        npz_events.len(),
        npz_time.as_secs_f64()
    );

    // === Step 4: Print stats ===
    println!("=== Event Statistics ===");
    print_event_stats("CSV.gz conversion", &csv_events);
    print_event_stats("Streaming reader", &streaming_events);
    print_event_stats("NPZ load", &npz_events);

    // === Step 5: Event-by-event comparison ===
    println!("\n=== Offline CSV.gz vs Streaming ===");
    let stream_matches_offline = compare_events(&streaming_events, &csv_events);
    println!("\n=== Offline CSV.gz vs NPZ ===");
    let offline_matches_npz = compare_events(&csv_events, &npz_events);
    println!("\n=== Streaming vs NPZ ===");
    let streaming_matches_npz = compare_events(&streaming_events, &npz_events);

    // === Step 6: Depth replay comparison ===
    println!("\n=== Depth Replay Comparison ===\n");
    let csv_snap = replay_depth(&csv_events, tick_size, 1.0);
    let streaming_snap = replay_depth(&streaming_events, tick_size, 1.0);
    let npz_snap = replay_depth(&npz_events, tick_size, 1.0);

    println!("  CSV.gz depth: bid_tick={} ask_tick={} mid={:.6} trades={} vol={:.2}",
        csv_snap.best_bid_tick, csv_snap.best_ask_tick, csv_snap.mid_price,
        csv_snap.trade_count, csv_snap.trade_volume);
    println!("  Stream  depth: bid_tick={} ask_tick={} mid={:.6} trades={} vol={:.2}",
        streaming_snap.best_bid_tick, streaming_snap.best_ask_tick, streaming_snap.mid_price,
        streaming_snap.trade_count, streaming_snap.trade_volume);
    println!("  NPZ    depth: bid_tick={} ask_tick={} mid={:.6} trades={} vol={:.2}",
        npz_snap.best_bid_tick, npz_snap.best_ask_tick, npz_snap.mid_price,
        npz_snap.trade_count, npz_snap.trade_volume);

    let depth_match = csv_snap.best_bid_tick == streaming_snap.best_bid_tick
        && csv_snap.best_ask_tick == streaming_snap.best_ask_tick
        && csv_snap.trade_count == streaming_snap.trade_count
        && streaming_snap.best_bid_tick == npz_snap.best_bid_tick
        && streaming_snap.best_ask_tick == npz_snap.best_ask_tick
        && streaming_snap.trade_count == npz_snap.trade_count;

    if depth_match {
        println!("  DEPTH MATCH: final state identical");
    } else {
        println!("  DEPTH MISMATCH");
    }

    // === Result ===
    println!("\n{}", "=".repeat(50));
    if stream_matches_offline && offline_matches_npz && streaming_matches_npz && depth_match {
        println!("RESULT: PASSED — streaming, offline CSV.gz, and NPZ are identical");
    } else {
        println!("RESULT: FAILED — divergence detected");
        println!("  At least one path diverged from the offline CSV.gz baseline.");
    }
    println!("{}", "=".repeat(50));

    Ok(())
}

#[allow(dead_code)]
fn multi_day_streaming_reader_example() -> Result<Reader<Event>, Box<dyn std::error::Error>> {
    Ok(Reader::builder()
        .parallel_load(true)
        .data(vec![
            DataSource::TardisStream(TardisStreamDayConfig {
                trades: "/data/trades/ADAUSDT/2026-03-05.csv.gz".into(),
                depth: "/data/depth/ADAUSDT/2026-03-05.csv.gz".into(),
                book_ticker: Some("/data/book_ticker/ADAUSDT/2026-03-05.csv.gz".into()),
            }),
            DataSource::TardisStream(TardisStreamDayConfig {
                trades: "/data/trades/ADAUSDT/2026-03-06.csv.gz".into(),
                depth: "/data/depth/ADAUSDT/2026-03-06.csv.gz".into(),
                book_ticker: None,
            }),
        ])
        .build()?)
}
