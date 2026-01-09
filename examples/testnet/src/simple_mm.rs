//! Simple Market Making Strategy for Testnet
//!
//! This is a basic grid trading strategy suitable for testnet testing.
//! It places bid and ask orders around the mid price with configurable spread.
//!
//! Usage:
//!   cargo run --release --example testnet_simple_mm

use std::collections::HashMap;

use hftbacktest::{
    live::{Instrument, LiveBot, LiveBotBuilder, LoggingRecorder, ipc::iceoryx::IceoryxUnifiedChannel},
    prelude::{Bot, HashMapMarketDepth, MarketDepth, OrdType, Recorder, Side, TimeInForce, INVALID_MAX, INVALID_MIN},
};

/// Strategy configuration
struct Config {
    /// Connector name (must match the running connector)
    connector_name: &'static str,
    /// Trading symbol (lowercase)
    symbol: &'static str,
    /// Tick size (price precision)
    tick_size: f64,
    /// Lot size (quantity precision)
    lot_size: f64,
    /// Half spread as a ratio of mid price (e.g., 0.0005 = 0.05%)
    half_spread: f64,
    /// Grid interval as a ratio of mid price
    grid_interval: f64,
    /// Number of orders on each side
    grid_num: usize,
    /// Order quantity
    order_qty: f64,
    /// Maximum position (absolute value)
    max_position: f64,
}

fn prepare_live(config: &Config) -> LiveBot<IceoryxUnifiedChannel, HashMapMarketDepth> {
    LiveBotBuilder::new()
        .register(Instrument::new(
            config.connector_name,
            config.symbol,
            config.tick_size,
            config.lot_size,
            HashMapMarketDepth::new(config.tick_size, config.lot_size),
            0,
        ))
        .build()
        .unwrap()
}

fn run_strategy<MD, I, R>(
    hbt: &mut I,
    recorder: &mut R,
    config: &Config,
) -> Result<(), i64>
where
    MD: MarketDepth,
    I: Bot<MD>,
    <I as Bot<MD>>::Error: std::fmt::Debug,
    R: Recorder,
    <R as Recorder>::Error: std::fmt::Debug,
{
    let tick_size = hbt.depth(0).tick_size() as f64;
    let mut iteration = 0;

    println!("Strategy started. Waiting for market data...");

    // Main loop: run every 500ms (reduced frequency to avoid rate limits)
    // Binance limit: 300 orders per 10 seconds = 30 orders/second
    // With 6 orders per iteration (3 buy + 3 sell), max 5 iterations/second
    while hbt.elapse(500_000_000).unwrap() == hftbacktest::prelude::ElapseResult::Ok {
        iteration += 1;

        // Record every 5 seconds (iteration interval is 500ms)
        if iteration % 10 == 0 {
            let _ = recorder.record(hbt);
        }

        let depth = hbt.depth(0);
        let position = hbt.position(0);

        // Wait for valid market depth
        if depth.best_bid_tick() == INVALID_MIN || depth.best_ask_tick() == INVALID_MAX {
            if iteration % 50 == 0 {
                println!("Waiting for market depth...");
            }
            continue;
        }

        let best_bid = depth.best_bid() as f64;
        let best_ask = depth.best_ask() as f64;
        let mid_price = (best_bid + best_ask) / 2.0;

        // Log status every 10 seconds (500ms × 20 = 10s)
        if iteration % 20 == 0 {
            println!(
                "[{:>6}] Mid: {:.2} | Bid: {:.2} | Ask: {:.2} | Pos: {:.4}",
                iteration / 10,
                mid_price,
                best_bid,
                best_ask,
                position
            );
        }

        // Calculate order prices with position skew
        let skew = config.half_spread / config.grid_num as f64;
        let normalized_position = position / config.order_qty;

        let bid_depth = config.half_spread + skew * normalized_position;
        let ask_depth = config.half_spread - skew * normalized_position;

        let mut bid_price = (mid_price * (1.0 - bid_depth)).min(best_bid);
        let mut ask_price = (mid_price * (1.0 + ask_depth)).max(best_ask);

        // Align to grid
        let grid_interval = (mid_price * config.grid_interval / tick_size).round() * tick_size;
        let grid_interval = grid_interval.max(tick_size);

        bid_price = (bid_price / grid_interval).floor() * grid_interval;
        ask_price = (ask_price / grid_interval).ceil() * grid_interval;

        // Debug: Log calculated prices
        if iteration <= 5 || iteration % 20 == 0 {
            println!(
                "[DEBUG] iter={} mid={:.2} bid_price={:.2} ask_price={:.2} grid_interval={:.2}",
                iteration, mid_price, bid_price, ask_price, grid_interval
            );
        }

        // Clear inactive orders
        hbt.clear_inactive_orders(Some(0));

        // Update bid orders
        {
            let orders = hbt.orders(0);
            let existing_count = orders.len();
            let mut new_orders = HashMap::new();

            if position < config.max_position && bid_price.is_finite() {
                for i in 0..config.grid_num {
                    let price = bid_price - (i as f64) * grid_interval;
                    let price_tick = (price / tick_size).round() as u64;
                    new_orders.insert(price_tick, price);
                }
            }

            // Cancel orders not in new grid
            let cancel_ids: Vec<u64> = orders
                .values()
                .filter(|o| o.side == Side::Buy && o.cancellable() && !new_orders.contains_key(&o.order_id))
                .map(|o| o.order_id)
                .collect();

            // Submit new orders
            let submit_orders: Vec<(u64, f64)> = new_orders
                .into_iter()
                .filter(|(id, _)| !orders.contains_key(id))
                .collect();

            // Debug: Log order activity
            if iteration <= 5 || !submit_orders.is_empty() || !cancel_ids.is_empty() {
                println!(
                    "[DEBUG] BID: existing_orders={} new_orders_to_submit={} orders_to_cancel={}",
                    existing_count, submit_orders.len(), cancel_ids.len()
                );
                for (id, price) in &submit_orders {
                    println!("[DEBUG]   -> Submit BUY: id={} price={:.2} qty={}", id, price, config.order_qty);
                }
            }

            for id in cancel_ids {
                println!("[DEBUG]   -> Cancel BUY: id={}", id);
                let result = hbt.cancel(0, id, false);
                println!("[DEBUG]      Cancel result: {:?}", result);
            }
            for (id, price) in submit_orders {
                let result = hbt.submit_buy_order(0, id, price, config.order_qty, TimeInForce::GTX, OrdType::Limit, false, false);
                println!("[DEBUG]      Submit BUY result: {:?}", result);
            }
        }

        // Update ask orders
        {
            let orders = hbt.orders(0);
            let existing_count = orders.len();
            let mut new_orders = HashMap::new();

            if position > -config.max_position && ask_price.is_finite() {
                for i in 0..config.grid_num {
                    let price = ask_price + (i as f64) * grid_interval;
                    let price_tick = (price / tick_size).round() as u64;
                    new_orders.insert(price_tick, price);
                }
            }

            // Cancel orders not in new grid
            let cancel_ids: Vec<u64> = orders
                .values()
                .filter(|o| o.side == Side::Sell && o.cancellable() && !new_orders.contains_key(&o.order_id))
                .map(|o| o.order_id)
                .collect();

            // Submit new orders
            let submit_orders: Vec<(u64, f64)> = new_orders
                .into_iter()
                .filter(|(id, _)| !orders.contains_key(id))
                .collect();

            // Debug: Log order activity
            if iteration <= 5 || !submit_orders.is_empty() || !cancel_ids.is_empty() {
                println!(
                    "[DEBUG] ASK: existing_orders={} new_orders_to_submit={} orders_to_cancel={}",
                    existing_count, submit_orders.len(), cancel_ids.len()
                );
                for (id, price) in &submit_orders {
                    println!("[DEBUG]   -> Submit SELL: id={} price={:.2} qty={}", id, price, config.order_qty);
                }
            }

            for id in cancel_ids {
                println!("[DEBUG]   -> Cancel SELL: id={}", id);
                let result = hbt.cancel(0, id, false);
                println!("[DEBUG]      Cancel result: {:?}", result);
            }
            for (id, price) in submit_orders {
                let result = hbt.submit_sell_order(0, id, price, config.order_qty, TimeInForce::GTX, OrdType::Limit, false, false);
                println!("[DEBUG]      Submit SELL result: {:?}", result);
            }
        }
    }

    Ok(())
}

fn main() {
    tracing_subscriber::fmt::init();

    // Configuration for BTCUSDT on Binance Futures Testnet
    // Note: Binance requires minimum notional of $100 per order
    // At BTC ~$90,000, minimum qty ≈ 0.0012 BTC
    let config = Config {
        connector_name: "bf",           // Must match connector startup name
        symbol: "btcusdt",              // Lowercase
        tick_size: 0.1,                 // BTCUSDT tick size
        lot_size: 0.001,                // BTCUSDT lot size
        half_spread: 0.001,             // 0.1% half spread (wider for testnet)
        grid_interval: 0.0005,          // 0.05% grid interval
        grid_num: 3,                    // 3 orders per side (reduce rate limit)
        order_qty: 0.002,               // ~$180 per order (above $100 minimum)
        max_position: 0.01,             // Max position
    };

    println!("======================================");
    println!(" Simple Market Making Strategy");
    println!("======================================");
    println!("Connector: {}", config.connector_name);
    println!("Symbol: {}", config.symbol);
    println!("Half Spread: {:.2}%", config.half_spread * 100.0);
    println!("Grid Num: {}", config.grid_num);
    println!("Order Qty: {}", config.order_qty);
    println!("Max Position: {}", config.max_position);
    println!("======================================");
    println!();

    let mut hbt = prepare_live(&config);
    let mut recorder = LoggingRecorder::new();

    if let Err(e) = run_strategy(&mut hbt, &mut recorder, &config) {
        eprintln!("Strategy error: {}", e);
    }

    println!("Closing bot...");
    hbt.close().unwrap();
    println!("Done.");
}
