//! Phased benchmark for the Tardis conversion pipeline.
//!
//! Measures each phase separately to establish baselines for streaming optimization.
//! Uses real data from NFS.
//!
//! Usage:
//!   cargo run --release -p hftbacktest --example streaming_benchmark \
//!     --no-default-features --features backtest
//!   cargo run --release -p hftbacktest --example streaming_benchmark \
//!     --no-default-features --features backtest -- --symbol BTCUSDT --date 2026-03-05

use std::env;
use std::path::Path;
use std::time::Instant;

use hftbacktest::backtest::data::read_npz_file;
use hftbacktest::data::{
    SnapshotMode, TardisConvertConfig, argsort_by_exch_ts, argsort_by_local_ts,
    StreamingTardisReader, StreamingTardisReaderConfig, TardisStreamDayConfig,
    correct_event_order, correct_local_timestamp, parse_tardis_files, validate_event_order,
};
use hftbacktest::types::Event;

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

fn file_size_mb(path: &str) -> f64 {
    std::fs::metadata(path)
        .map(|m| m.len() as f64 / 1024.0 / 1024.0)
        .unwrap_or(0.0)
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

fn streaming_live_mem_mb(config: &StreamingTardisReaderConfig) -> f64 {
    let chunk_bytes = config.chunk_size as f64 * 64.0;
    let window_bytes = config.reorder_window as f64 * 2.0 * 64.0;
    (chunk_bytes + window_bytes) / 1024.0 / 1024.0
}

struct PhaseResult {
    name: &'static str,
    duration_ms: f64,
    events_in: usize,
    events_out: usize,
    mem_mb: f64,
}

impl PhaseResult {
    fn rate(&self) -> f64 {
        if self.duration_ms > 0.0 {
            self.events_in as f64 / (self.duration_ms / 1000.0)
        } else {
            0.0
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    let mut symbol = "ADAUSDT".to_string();
    let mut date = "2026-03-05".to_string();

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
            "--help" | "-h" => {
                println!("Usage: streaming_benchmark [--symbol SYM] [--date YYYY-MM-DD]");
                println!("Default: ADAUSDT 2026-03-05 (medium ~100MB raw)");
                println!("Stress:  BTCUSDT 2026-03-05 (large ~1.3GB raw)");
                return Ok(());
            }
            _ => {
                i += 1;
            }
        }
    }

    let (trades_path, depth_path, bbo_path, npz_path) = get_paths(&symbol, &date);

    // Verify files exist
    let has_trades = Path::new(&trades_path).exists();
    let has_depth = Path::new(&depth_path).exists();
    let has_bbo = Path::new(&bbo_path).exists();
    let has_npz = Path::new(&npz_path).exists();

    println!("=== Streaming Benchmark: {} {} ===\n", symbol, date);
    println!("Input files:");
    println!(
        "  trades:      {} ({:.1} MB)",
        if has_trades { "OK" } else { "MISSING" },
        file_size_mb(&trades_path)
    );
    println!(
        "  depth:       {} ({:.1} MB)",
        if has_depth { "OK" } else { "MISSING" },
        file_size_mb(&depth_path)
    );
    println!(
        "  book_ticker: {} ({:.1} MB)",
        if has_bbo { "OK" } else { "MISSING" },
        file_size_mb(&bbo_path)
    );
    println!(
        "  npz:         {} ({:.1} MB)",
        if has_npz { "OK" } else { "MISSING" },
        file_size_mb(&npz_path)
    );

    if !has_trades || !has_depth {
        eprintln!("\nERROR: trades and depth files are required");
        return Ok(());
    }

    let config = TardisConvertConfig {
        buffer_size: 100_000_000,
        ss_buffer_size: 1_000_000,
        base_latency: 0,
        snapshot_mode: SnapshotMode::Process,
    };

    let mut phases: Vec<PhaseResult> = Vec::new();

    // === Phase 1a: Parse trades only ===
    {
        let start = Instant::now();
        let events = parse_tardis_files(&[&trades_path], &config)?;
        let elapsed = start.elapsed();
        phases.push(PhaseResult {
            name: "parse trades",
            duration_ms: elapsed.as_secs_f64() * 1000.0,
            events_in: events.len(),
            events_out: events.len(),
            mem_mb: events.len() as f64 * 64.0 / 1024.0 / 1024.0,
        });
    }

    // === Phase 1b: Parse depth only ===
    {
        let start = Instant::now();
        let events = parse_tardis_files(&[&depth_path], &config)?;
        let elapsed = start.elapsed();
        phases.push(PhaseResult {
            name: "parse depth",
            duration_ms: elapsed.as_secs_f64() * 1000.0,
            events_in: events.len(),
            events_out: events.len(),
            mem_mb: events.len() as f64 * 64.0 / 1024.0 / 1024.0,
        });
    }

    // === Phase 1c: Parse book_ticker only ===
    if has_bbo {
        let start = Instant::now();
        let events = parse_tardis_files(&[&bbo_path], &config)?;
        let elapsed = start.elapsed();
        phases.push(PhaseResult {
            name: "parse bbo",
            duration_ms: elapsed.as_secs_f64() * 1000.0,
            events_in: events.len(),
            events_out: events.len(),
            mem_mb: events.len() as f64 * 64.0 / 1024.0 / 1024.0,
        });
    }

    // === Phase 2: Parse all files combined ===
    let mut input_files = vec![trades_path.clone(), depth_path.clone()];
    if has_bbo {
        input_files.push(bbo_path.clone());
    }

    let (mut events, parse_all_ms) = {
        let start = Instant::now();
        let events = parse_tardis_files(&input_files, &config)?;
        let elapsed = start.elapsed();
        let ms = elapsed.as_secs_f64() * 1000.0;
        phases.push(PhaseResult {
            name: "parse ALL",
            duration_ms: ms,
            events_in: events.len(),
            events_out: events.len(),
            mem_mb: events.len() as f64 * 64.0 / 1024.0 / 1024.0,
        });
        (events, ms)
    };

    let raw_event_count = events.len();

    // === Phase 3: correct_local_timestamp ===
    let correct_ts_ms = {
        let start = Instant::now();
        let min_latency = correct_local_timestamp(&mut events, config.base_latency);
        let elapsed = start.elapsed();
        let ms = elapsed.as_secs_f64() * 1000.0;
        println!(
            "\n  min_latency = {} ns ({} us)",
            min_latency,
            min_latency / 1000
        );
        phases.push(PhaseResult {
            name: "correct_ts",
            duration_ms: ms,
            events_in: raw_event_count,
            events_out: raw_event_count,
            mem_mb: 0.0,
        });
        ms
    };

    // === Phase 4: argsort ===
    let (sorted_exch, sorted_local, sort_ms) = {
        let start = Instant::now();
        let se = argsort_by_exch_ts(&events);
        let sl = argsort_by_local_ts(&events);
        let elapsed = start.elapsed();
        let ms = elapsed.as_secs_f64() * 1000.0;
        let idx_mem = (se.len() + sl.len()) as f64 * 8.0 / 1024.0 / 1024.0;
        phases.push(PhaseResult {
            name: "argsort x2",
            duration_ms: ms,
            events_in: raw_event_count,
            events_out: raw_event_count,
            mem_mb: idx_mem,
        });
        (se, sl, ms)
    };

    // === Phase 5: correct_event_order (merge) ===
    let (corrected, merge_ms) = {
        let start = Instant::now();
        let result = correct_event_order(&events, &sorted_exch, &sorted_local);
        let elapsed = start.elapsed();
        let ms = elapsed.as_secs_f64() * 1000.0;
        phases.push(PhaseResult {
            name: "merge",
            duration_ms: ms,
            events_in: raw_event_count,
            events_out: result.len(),
            mem_mb: result.len() as f64 * 64.0 / 1024.0 / 1024.0,
        });
        (result, ms)
    };

    // === Phase 6: validate ===
    let validate_ms = {
        let start = Instant::now();
        validate_event_order(&corrected)?;
        let elapsed = start.elapsed();
        let ms = elapsed.as_secs_f64() * 1000.0;
        phases.push(PhaseResult {
            name: "validate",
            duration_ms: ms,
            events_in: corrected.len(),
            events_out: corrected.len(),
            mem_mb: 0.0,
        });
        ms
    };

    // === Phase 7: NPZ load (comparison baseline) ===
    if has_npz {
        let start = Instant::now();
        let npz_data: hftbacktest::backtest::data::Data<Event> =
            read_npz_file(&npz_path, "data")?;
        let elapsed = start.elapsed();
        phases.push(PhaseResult {
            name: "NPZ load",
            duration_ms: elapsed.as_secs_f64() * 1000.0,
            events_in: npz_data.len(),
            events_out: npz_data.len(),
            mem_mb: npz_data.len() as f64 * 64.0 / 1024.0 / 1024.0,
        });
    }

    // === Phase 8: Streaming reader ===
    let streaming_config = StreamingTardisReaderConfig::default();
    let streaming_event_count = {
        let start = Instant::now();
        let mut reader = StreamingTardisReader::new(
            TardisStreamDayConfig {
                trades: trades_path.clone(),
                depth: depth_path.clone(),
                book_ticker: has_bbo.then_some(bbo_path.clone()),
            },
            streaming_config.clone(),
        )?;
        let mut count = 0usize;
        while let Some(chunk) = reader.next_data()? {
            count += chunk.len();
        }
        let elapsed = start.elapsed();
        phases.push(PhaseResult {
            name: "streaming",
            duration_ms: elapsed.as_secs_f64() * 1000.0,
            events_in: count,
            events_out: count,
            mem_mb: streaming_live_mem_mb(&streaming_config),
        });
        count
    };

    // === Summary ===
    let total_pipeline_ms = parse_all_ms + correct_ts_ms + sort_ms + merge_ms + validate_ms;

    println!("\n{:-<82}", "");
    println!(
        "{:<18} {:>10} {:>12} {:>12} {:>10} {:>12}",
        "Phase", "Time (ms)", "Events In", "Events Out", "Mem (MB)", "Rate (M/s)"
    );
    println!("{:-<82}", "");
    for p in &phases {
        println!(
            "{:<18} {:>10.1} {:>12} {:>12} {:>10.1} {:>12.2}",
            p.name,
            p.duration_ms,
            p.events_in,
            p.events_out,
            p.mem_mb,
            p.rate() / 1_000_000.0,
        );
    }
    println!("{:-<82}", "");
    println!(
        "{:<18} {:>10.1} {:>12} {:>12}",
        "TOTAL pipeline", total_pipeline_ms, raw_event_count, corrected.len(),
    );
    println!("{:-<82}", "");

    // Event type breakdown
    let mut counts = [0usize; 6]; // 0=other, 1=depth, 2=trade, 3=clear, 4=snapshot, 5=bbo
    for ev in &corrected {
        let t = (ev.ev & 0xFF) as usize;
        if t < 6 {
            counts[t] += 1;
        } else {
            counts[0] += 1;
        }
    }
    println!("\nEvent breakdown ({} total):", corrected.len());
    for t in 1..=5 {
        if counts[t] > 0 {
            println!(
                "  {:<12} {:>12} ({:.1}%)",
                event_type_name(t as u64),
                counts[t],
                counts[t] as f64 / corrected.len() as f64 * 100.0
            );
        }
    }

    // Memory analysis
    let raw_mem = raw_event_count as f64 * 64.0 / 1024.0 / 1024.0;
    let merged_mem = corrected.len() as f64 * 64.0 / 1024.0 / 1024.0;
    let peak_mem = raw_mem + merged_mem; // both exist during merge phase
    println!("\nMemory analysis:");
    println!("  Raw events:    {:.1} MB ({} events)", raw_mem, raw_event_count);
    println!("  Merged events: {:.1} MB ({} events)", merged_mem, corrected.len());
    println!("  Peak (during merge): {:.1} MB", peak_mem);
    println!(
        "  Expansion ratio: {:.2}x (merge duplicates {:.1}% of events)",
        corrected.len() as f64 / raw_event_count as f64,
        (corrected.len() - raw_event_count) as f64 / raw_event_count as f64 * 100.0
    );

    // Streaming potential
    println!("\nStreaming potential:");
    println!(
        "  NPZ write skipped:  ~{:.0}ms (estimated 65% of pipeline)",
        total_pipeline_ms * 0.65 / 0.35
    );
    println!(
        "  Parse-only rate:    {:.2} M events/s",
        raw_event_count as f64 / (parse_all_ms / 1000.0) / 1_000_000.0
    );
    println!(
        "  Streaming target:   ~70 MB/route (current peak: {:.0} MB)",
        peak_mem
    );
    println!(
        "  Streaming output:   {} events (offline corrected: {})",
        streaming_event_count,
        corrected.len()
    );

    Ok(())
}
