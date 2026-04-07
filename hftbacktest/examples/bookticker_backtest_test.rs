//! End-to-end backtest test with bookTicker BBO data.
//!
//! Verifies:
//! 1. BBO events are processed at 1ms resolution (not batched to 50ms)
//! 2. Orderbook never crosses
//! 3. BBO provides price updates between 50ms depth batches
//!
//! Usage:
//!   cargo run --release -p hftbacktest --example bookticker_backtest_test \
//!     --no-default-features --features backtest

use std::time::Instant;

use hftbacktest::{
    backtest::{
        Backtest, ExchangeKind, L2AssetBuilder,
        assettype::LinearAsset,
        data::DataSource,
        models::{
            CommonFees, ConstantLatency, PowerProbQueueFunc3, ProbQueueModel, TradingValueFeeModel,
        },
    },
    depth::{INVALID_MAX, INVALID_MIN, MarketDepth},
    prelude::{Bot, ElapseResult, HashMapMarketDepth},
};

const BASE_PATH: &str =
    "/mnt/data/samba/users/Data/tardis/datasets/binance-futures/hft_npz/BTCUSDT";

fn main() {
    tracing_subscriber::fmt::init();

    let data_file = format!("{}/BTCUSDT_20260301.npz", BASE_PATH);

    println!("=== bookTicker E2E Backtest Test ===");
    println!("Data: {}", data_file);

    let latency_model = ConstantLatency::new(5_000_000, 5_000_000); // 5ms
    let asset_type = LinearAsset::new(1.0);
    let queue_model = ProbQueueModel::new(PowerProbQueueFunc3::new(3.0));

    let mut hbt = Backtest::builder()
        .add_asset(
            L2AssetBuilder::new()
                .data(vec![DataSource::File(data_file)])
                .latency_model(latency_model)
                .asset_type(asset_type)
                .fee_model(TradingValueFeeModel::new(CommonFees::new(-0.00005, 0.0007)))
                .exchange(ExchangeKind::NoPartialFillExchange)
                .queue_model(queue_model)
                .depth(|| HashMapMarketDepth::new(0.1, 0.001))
                .build()
                .unwrap(),
        )
        .build()
        .unwrap();

    println!("Running 10 minutes of market time with wait_next_feed...\n");
    let start = Instant::now();
    let ten_min_ns: i64 = 600_000_000_000;

    let mut feeds = 0u64;
    let mut cross_count = 0u64;
    let mut last_bid = f64::NAN;
    let mut last_ask = f64::NAN;
    let mut last_bid_qty = f64::NAN;
    let mut last_ask_qty = f64::NAN;

    // Track BBO change intervals (ns between consecutive price changes)
    let mut bid_change_intervals: Vec<i64> = Vec::new();
    let mut ask_change_intervals: Vec<i64> = Vec::new();
    let mut last_bid_change_ts: i64 = 0;
    let mut last_ask_change_ts: i64 = 0;

    let mut bid_px_changes = 0u64;
    let mut ask_px_changes = 0u64;
    let mut bid_qty_changes = 0u64;
    let mut ask_qty_changes = 0u64;

    // Track sub-50ms price changes (proves BBO adds resolution)
    let mut sub_50ms_bid_changes = 0u64;
    let mut sub_50ms_ask_changes = 0u64;
    let mut sub_10ms_bid_changes = 0u64;
    let mut sub_1ms_bid_changes = 0u64;

    loop {
        match hbt.wait_next_feed(true, ten_min_ns) {
            Ok(ElapseResult::MarketFeed) | Ok(ElapseResult::OrderResponse) => {}
            _ => break,
        }
        feeds += 1;

        let depth = hbt.depth(0);
        let bb_tick = depth.best_bid_tick();
        let ba_tick = depth.best_ask_tick();

        if bb_tick == INVALID_MIN || ba_tick == INVALID_MAX {
            continue;
        }

        let bid = depth.best_bid();
        let ask = depth.best_ask();
        let bid_qty = depth.best_bid_qty();
        let ask_qty = depth.best_ask_qty();
        let ts = hbt.current_timestamp();

        // Cross check
        if bb_tick >= ba_tick {
            cross_count += 1;
            if cross_count <= 5 {
                eprintln!(
                    "CROSS #{} at ts={}: bid={:.1} >= ask={:.1}",
                    cross_count, ts, bid, ask
                );
            }
        }

        // Bid price changed
        if bid != last_bid && !last_bid.is_nan() {
            bid_px_changes += 1;
            if last_bid_change_ts > 0 {
                let interval = ts - last_bid_change_ts;
                bid_change_intervals.push(interval);
                if interval < 50_000_000 {
                    sub_50ms_bid_changes += 1;
                }
                if interval < 10_000_000 {
                    sub_10ms_bid_changes += 1;
                }
                if interval < 1_000_000 {
                    sub_1ms_bid_changes += 1;
                }
            }
            last_bid_change_ts = ts;
        } else if bid_qty != last_bid_qty && !last_bid_qty.is_nan() {
            bid_qty_changes += 1;
        }

        // Ask price changed
        if ask != last_ask && !last_ask.is_nan() {
            ask_px_changes += 1;
            if last_ask_change_ts > 0 {
                let interval = ts - last_ask_change_ts;
                ask_change_intervals.push(interval);
                if interval < 50_000_000 {
                    sub_50ms_ask_changes += 1;
                }
            }
            last_ask_change_ts = ts;
        } else if ask_qty != last_ask_qty && !last_ask_qty.is_nan() {
            ask_qty_changes += 1;
        }

        last_bid = bid;
        last_ask = ask;
        last_bid_qty = bid_qty;
        last_ask_qty = ask_qty;
    }

    let elapsed = start.elapsed();

    // Compute percentiles of bid change intervals
    bid_change_intervals.sort();
    let n = bid_change_intervals.len();
    let p50 = if n > 0 {
        bid_change_intervals[n / 2]
    } else {
        0
    };
    let p10 = if n > 0 {
        bid_change_intervals[n / 10]
    } else {
        0
    };
    let p1 = if n > 0 {
        bid_change_intervals[n / 100]
    } else {
        0
    };
    let p_min = bid_change_intervals.first().copied().unwrap_or(0);

    println!("=== Results (10 min, {} feeds) ===", feeds);
    println!();
    println!("Crosses:             {}", cross_count);
    println!();
    println!("Bid price changes:   {}", bid_px_changes);
    println!("Ask price changes:   {}", ask_px_changes);
    println!("Bid qty changes:     {}", bid_qty_changes);
    println!("Ask qty changes:     {}", ask_qty_changes);
    println!();
    println!("--- BBO Resolution Analysis ---");
    println!("Bid price change interval:");
    println!("  min:  {:.3}ms", p_min as f64 / 1e6);
    println!("  P1:   {:.3}ms", p1 as f64 / 1e6);
    println!("  P10:  {:.3}ms", p10 as f64 / 1e6);
    println!("  P50:  {:.3}ms", p50 as f64 / 1e6);
    println!();
    println!(
        "Sub-50ms bid price changes: {} ({:.1}%)",
        sub_50ms_bid_changes,
        sub_50ms_bid_changes as f64 / bid_px_changes.max(1) as f64 * 100.0
    );
    println!(
        "Sub-10ms bid price changes: {} ({:.1}%)",
        sub_10ms_bid_changes,
        sub_10ms_bid_changes as f64 / bid_px_changes.max(1) as f64 * 100.0
    );
    println!(
        "Sub-1ms bid price changes:  {} ({:.1}%)",
        sub_1ms_bid_changes,
        sub_1ms_bid_changes as f64 / bid_px_changes.max(1) as f64 * 100.0
    );
    println!();
    println!("Wall time: {:.2}s", elapsed.as_secs_f64());

    if cross_count > 0 {
        eprintln!("\nFAILED: {} crosses detected", cross_count);
        std::process::exit(1);
    }
    if sub_50ms_bid_changes == 0 && bid_px_changes > 100 {
        eprintln!("\nFAILED: No sub-50ms price changes — BBO not providing 1ms resolution");
        std::process::exit(1);
    }
    println!("\nPASSED: Zero crosses, BBO provides sub-50ms resolution");
}
