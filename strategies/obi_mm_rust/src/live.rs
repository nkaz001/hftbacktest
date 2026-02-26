//! OBI MM Strategy for Live Trading with Order Recording
//!
//! This binary runs the OBI MM strategy on exchanges (Binance Futures, PAPI, Bybit, etc.)
//! and records all trading activity to ClickHouse via the order-recorder service.
//!
//! Usage:
//!   # First start the order-recorder service:
//!   cargo run --release -p order-recorder
//!
//!   # Then start the connector:
//!   cargo run --release -p connector -- --name bf --connector binancepapi --config config.toml
//!
//!   # Then run this strategy:
//!   cargo run --release -p obi-mm --bin obi_mm_live --features live -- \
//!     --connector bf --symbol btcusdt --config config.toml

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use hftbacktest::{
    live::{
        ipc::iceoryx::IceoryxUnifiedChannel, Instrument, LiveBot, LiveBotBuilder, LoggingRecorder,
    },
    prelude::{Bot, HashMapMarketDepth, L2MarketDepth, MarketDepth, Recorder, INVALID_MAX, INVALID_MIN},
    types::ElapseResult,
};
use order_recorder::{
    StrategyState,
    hook::{OrderHookContext, create_order_hook},
};
use tracing::{info, warn, Level};
use tracing_subscriber::FmtSubscriber;

use obi_mm::config::{GlobalConfig, StrategyConfig, StrategyParams, StopLossConfig};
use obi_mm::{ObiMmStrategy, WarmupConfig, WarmupResult};

#[derive(Parser, Debug)]
#[command(author, version, about = "OBI MM Strategy for Live Trading with Recording")]
struct Args {
    /// Path to strategy configuration file (optional, uses defaults if not provided)
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Connector name for IPC communication (must match running connector)
    #[arg(short = 'n', long, default_value = "bf")]
    connector: String,

    /// Symbol to trade (lowercase, e.g., btcusdt)
    #[arg(short, long, default_value = "btcusdt")]
    symbol: String,

    /// Tick size (price precision, 0.0 = auto-resolve from exchange)
    #[arg(long, default_value = "0.0")]
    tick_size: f64,

    /// Lot size (quantity precision, 0.0 = auto-resolve from exchange)
    #[arg(long, default_value = "0.0")]
    lot_size: f64,

    /// Dry run mode (log actions but don't submit orders)
    #[arg(long, default_value = "false")]
    dry_run: bool,

    /// Log interval in seconds
    #[arg(long, default_value = "10")]
    log_interval: u64,

    /// Strategy ID for recording
    #[arg(long, default_value = "obi_mm_live")]
    strategy_id: String,

    /// IPC service name for recording events
    #[arg(long, default_value = "hft_recording")]
    recording_service: String,

    /// Directory containing npz files for warmup
    #[arg(long)]
    warmup_data_dir: Option<PathBuf>,

    /// Maximum age of warmup data in seconds (default: 7200 = 2 hours)
    #[arg(long, default_value = "7200")]
    warmup_max_age: i64,

    /// Minimum warmup data duration in seconds (default: 3600 = 1 hour)
    #[arg(long, default_value = "3600")]
    warmup_min_duration: i64,
}

fn prepare_live_with_hook(
    args: &Args,
    hook_ctx: Option<Arc<OrderHookContext>>,
) -> LiveBot<IceoryxUnifiedChannel, HashMapMarketDepth> {
    let mut builder = LiveBotBuilder::new()
        .register(Instrument::new(
            &args.connector,
            &args.symbol,
            args.tick_size,
            args.lot_size,
            HashMapMarketDepth::new(args.tick_size, args.lot_size),
            0,
        ));

    // Add order hook if context is provided
    if let Some(ctx) = hook_ctx {
        builder = builder.order_recv_hook(move |prev_order, new_order| {
            ctx.handle_order(prev_order, new_order);
            Ok(())
        });
    }

    builder.build().unwrap()
}

fn run_strategy<MD, I, R>(
    hbt: &mut I,
    recorder: &mut R,
    strategy: &mut ObiMmStrategy,
    args: &Args,
    hook_ctx: Option<Arc<OrderHookContext>>,
) -> Result<()>
where
    MD: MarketDepth + L2MarketDepth,
    I: Bot<MD>,
    <I as Bot<MD>>::Error: std::fmt::Debug,
    R: Recorder,
    <R as Recorder>::Error: std::fmt::Debug,
{
    let mut tick_size = hbt.depth(0).tick_size();
    let mut iteration = 0u64;
    let log_interval_iters = args.log_interval * 2; // elapse is 500ms
    let position_publish_interval = 60; // Publish position every 30 seconds (60 x 500ms)

    info!("Strategy started. Waiting for market data...");
    info!("Tick size: {} (will auto-update from InstrumentInfo)", tick_size);

    // Main loop: run every 500ms
    while hbt.elapse(500_000_000).unwrap() == ElapseResult::Ok {
        iteration += 1;

        // Record periodically
        if iteration % 20 == 0 {
            let _ = recorder.record(hbt);
        }

        let depth = hbt.depth(0);
        let position = hbt.position(0);

        // Check if tick_size was updated by InstrumentInfo (auto-resolution from exchange)
        let current_tick_size = depth.tick_size();
        if current_tick_size != tick_size && current_tick_size > 0.0 {
            info!("Tick size updated: {} -> {}", tick_size, current_tick_size);
            strategy.set_tick_size(current_tick_size);
            // Also update order hook context so recorded prices are correct
            if let Some(ref ctx) = hook_ctx {
                ctx.set_tick_size(current_tick_size);
            }
            tick_size = current_tick_size;
        }

        // Wait for valid market depth
        if depth.best_bid_tick() == INVALID_MIN || depth.best_ask_tick() == INVALID_MAX {
            if iteration % 20 == 0 {
                info!("Waiting for market depth...");
            }
            continue;
        }

        let best_bid = depth.best_bid();
        let best_ask = depth.best_ask();
        let mid_price = (best_bid + best_ask) / 2.0;

        // Log status periodically
        if iteration % log_interval_iters == 0 {
            let orders = hbt.orders(0);
            // Use enough decimal places to show meaningful price digits
            let prec = if mid_price >= 100.0 { 2 }
                else if mid_price >= 1.0 { 4 }
                else if mid_price >= 0.01 { 6 }
                else { 8 };
            info!(
                "[{:>6}s] Mid: {:.prec$} | Bid: {:.prec$} | Ask: {:.prec$} | Pos: {:.4} | Orders: {}",
                iteration / 2,
                mid_price,
                best_bid,
                best_ask,
                position,
                orders.len(),
                prec = prec,
            );
        }

        // Run strategy update
        if !args.dry_run {
            strategy.update(hbt, 0);
        } else {
            // Dry run - just log what would happen
            if iteration % log_interval_iters == 0 {
                info!("[DRY RUN] Would update orders at mid={:.2}", mid_price);
            }
        }

        // Publish position snapshot periodically
        if iteration % position_publish_interval == 0 {
            if let Some(ref ctx) = hook_ctx {
                // Calculate simple unrealized PnL based on position and mid price
                // For a more accurate PnL, we'd need to track entry price
                let unrealized_pnl = 0.0; // Simplified - proper PnL requires entry price tracking
                let realized_pnl = 0.0;   // Would need to track from fills

                ctx.publish_position(
                    position,
                    mid_price,   // avg_price approximation
                    mid_price,
                    unrealized_pnl,
                    realized_pnl,
                );
            }
        }

        // Publish factor snapshot periodically (every 20 iterations = 10 seconds)
        if iteration % 20 == 0 {
            if let Some(ref ctx) = hook_ctx {
                ctx.publish_factors(vec![
                    ("obi.zscore", strategy.obi_factor.zscore()),
                    ("obi.raw", strategy.obi_factor.raw()),
                    ("obi.mean", strategy.obi_factor.mean()),
                    ("obi.std", strategy.obi_factor.std()),
                    ("obi.sample_count", strategy.obi_factor.sample_count() as f64),
                    ("vol.value", strategy.volatility_factor.volatility()),
                    ("vol.mean_return", strategy.volatility_factor.mean_return()),
                    ("vol.sample_count", strategy.volatility_factor.sample_count() as f64),
                    ("sl.status", strategy.stop_loss.status() as u8 as f64),
                    ("price.mid", mid_price),
                    ("price.bid", best_bid),
                    ("price.ask", best_ask),
                ]);
            }
        }
    }

    Ok(())
}

pub fn main() -> Result<()> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let args = Args::parse();

    // Load or create configuration
    let config = if let Some(config_path) = &args.config {
        info!("Loading config from {:?}", config_path);
        StrategyConfig::from_file(config_path.to_str().unwrap())?
    } else {
        info!("Using default testnet configuration");
        // Create testnet-optimized defaults
        StrategyConfig {
            global: GlobalConfig {
                order_qty: 0.002,      // ~$180 per order (above $100 minimum)
                max_position: 0.02,    // Max position
                grid_num: 3,           // 3 orders per side
                grid_interval: 1.0,    // $1 grid interval
                roi_lb: 0.0,
                roi_ub: 200000.0,
            },
            params: StrategyParams {
                looking_depth: 0.001,                      // 0.1% depth
                window: 360,                               // 360 samples (shorter for testnet)
                volatility_window: 60,                     // 60 samples for volatility
                half_spread: 0.0005,                       // 0.05% half spread
                skew: 0.01,                                // Position skew
                c1: 0.0001,                                // Alpha coefficient
                power: 1.0,                                // Volatility power
                live_seconds: 30.0,                        // Order lifetime
                update_interval_ns: 10_000_000_000,        // 10 seconds
                volatility_interval_ns: 60_000_000_000,    // 60 seconds
            },
            stop_loss: StopLossConfig {
                volatility_mean: 0.005,
                volatility_std: 0.002,
                change_mean: 0.001,
                change_std: 0.003,
                volatility_threshold: 3.0,
                change_threshold: 3.0,
            },
        }
    };

    println!("======================================");
    println!(" OBI MM Strategy (Live + Recording)");
    println!("======================================");
    println!("Connector: {}", args.connector);
    println!("Symbol: {}", args.symbol);
    println!("Strategy ID: {}", args.strategy_id);
    println!("Recording Service: {}", args.recording_service);
    println!("Tick Size: {}", args.tick_size);
    println!("Lot Size: {}", args.lot_size);
    println!("Dry Run: {}", args.dry_run);
    println!("--------------------------------------");
    println!("Order Qty: {}", config.global.order_qty);
    println!("Max Position: {}", config.global.max_position);
    println!("Grid Num: {}", config.global.grid_num);
    println!("Half Spread: {:.4}%", config.params.half_spread * 100.0);
    println!("Skew: {}", config.params.skew);
    println!("C1 (Alpha): {}", config.params.c1);
    println!("======================================");
    println!();

    // Initialize order hook context using the public module
    let hook_ctx = create_order_hook(
        &args.recording_service,
        args.connector.clone(),
        args.strategy_id.clone(),
        args.symbol.clone(),
        args.tick_size,
    );

    if hook_ctx.is_some() {
        info!("Order recording enabled via order_recv_hook");
    }

    // Publish strategy starting status
    if let Some(ref ctx) = hook_ctx {
        ctx.publish_status(StrategyState::Starting, format!("Starting OBI MM on {}", args.symbol));
    }

    // Create live bot with order hook
    info!("Connecting to connector '{}'...", args.connector);
    let mut hbt = prepare_live_with_hook(&args, hook_ctx.clone());

    // Wait for connection and get tick size
    let tick_size = hbt.depth(0).tick_size();
    info!("Connected! Using tick size: {}", tick_size);

    // Create strategy
    let mut strategy = ObiMmStrategy::new(
        config.global.clone(),
        config.params.clone(),
        config.stop_loss.clone(),
        tick_size,
    );

    // Attempt warmup from npz data if configured
    if let Some(ref warmup_dir) = args.warmup_data_dir {
        info!("Attempting warmup from {:?}", warmup_dir);

        let warmup_config = WarmupConfig::new(warmup_dir, &args.symbol)
            .with_max_age(args.warmup_max_age)
            .with_min_duration(args.warmup_min_duration);

        match strategy.warmup_from_npz(&warmup_config) {
            WarmupResult::Success { events_processed, data_age_seconds } => {
                info!(
                    "Warmup successful: {} events processed, data age: {} seconds",
                    events_processed, data_age_seconds
                );
                let (obi, obi_req, vol, vol_req) = strategy.warmup_progress();
                info!(
                    "Factor status: OBI {}/{}, Vol {}/{}",
                    obi, obi_req, vol, vol_req
                );
            }
            WarmupResult::NoDataFound => {
                warn!("No warmup data found, will warm up from live data");
            }
            WarmupResult::DataTooOld { age_seconds } => {
                warn!(
                    "Warmup data too old ({} seconds), will warm up from live data",
                    age_seconds
                );
            }
            WarmupResult::Error(e) => {
                warn!("Warmup error: {}, will warm up from live data", e);
            }
        }
    } else {
        info!("No warmup data directory specified, will warm up from live data");
    }

    // Create recorder
    let mut recorder = LoggingRecorder::new();

    // Publish running status
    let warmup_status = if strategy.is_ready() {
        "Strategy running (warmup complete)"
    } else {
        "Strategy running (warming up from live data)"
    };
    if let Some(ref ctx) = hook_ctx {
        ctx.publish_status(StrategyState::Running, warmup_status.to_string());
    }

    // Run strategy
    info!("Starting OBI MM strategy... (is_ready={})", strategy.is_ready());
    let result = run_strategy(&mut hbt, &mut recorder, &mut strategy, &args, hook_ctx.clone());

    // Publish stopped status
    if let Some(ref ctx) = hook_ctx {
        let message = match &result {
            Ok(_) => "Strategy stopped normally".to_string(),
            Err(e) => format!("Strategy stopped with error: {:?}", e),
        };
        ctx.publish_status(StrategyState::Stopped, message);
    }

    if let Err(e) = result {
        warn!("Strategy error: {:?}", e);
    }

    // Cleanup
    info!("Closing bot...");
    hbt.close().unwrap();
    info!("Done.");

    Ok(())
}
