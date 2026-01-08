//! Template Strategy for HFT System
//!
//! This is a template strategy that demonstrates the key components needed
//! to integrate a new trading strategy into the HFT management system.
//!
//! ## Usage
//!
//! ```bash
//! # Build
//! cargo build --release -p template_strategy
//!
//! # Run directly
//! ./target/release/template_strategy --config config.toml --symbol btcusdt
//!
//! # Or via supervisor (recommended)
//! # Add to cluster.toml and control via Dashboard/API
//! ```
//!
//! ## Key Components
//!
//! 1. CLI Arguments - Parsed by clap, must include --config and --symbol
//! 2. Configuration - Strategy-specific parameters loaded from TOML
//! 3. LiveBot - Connection to connector via IPC for market data and order execution
//! 4. Strategy Loop - Main trading logic
//! 5. Order Recording (optional) - Record trades to ClickHouse

use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use hftbacktest::{
    live::{LiveBot, LiveBotBuilder},
    prelude::*,
};
use serde::Deserialize;
use tracing::{info, warn, error, debug};

// =============================================================================
// CLI Arguments
// =============================================================================

/// Template Strategy - Copy and modify for your own strategy
#[derive(Parser, Debug)]
#[command(name = "template_strategy")]
#[command(about = "Template strategy for HFT system")]
struct Args {
    /// Path to strategy configuration file (TOML)
    #[arg(long)]
    config: String,

    /// Trading symbol (e.g., btcusdt)
    #[arg(long)]
    symbol: String,

    /// Connector name (must match cluster config)
    #[arg(long, default_value = "bf")]
    connector: String,

    /// Enable verbose logging
    #[arg(long, short)]
    verbose: bool,
}

// =============================================================================
// Configuration
// =============================================================================

/// Strategy configuration loaded from TOML file
#[derive(Debug, Clone, Deserialize)]
struct Config {
    /// Global settings
    global: GlobalConfig,
    /// Strategy parameters
    params: StrategyParams,
    /// Risk management (optional)
    #[serde(default)]
    risk: RiskConfig,
}

#[derive(Debug, Clone, Deserialize)]
struct GlobalConfig {
    /// Order quantity per trade
    order_qty: f64,
    /// Maximum position size (absolute value)
    max_position: f64,
    /// Number of orders per side for grid strategies
    #[serde(default = "default_grid_num")]
    grid_num: usize,
    /// Price tick size
    #[serde(default = "default_tick_size")]
    tick_size: f64,
    /// Lot size (minimum order quantity)
    #[serde(default = "default_lot_size")]
    lot_size: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct StrategyParams {
    /// Half spread from mid price (e.g., 0.0005 = 0.05%)
    #[serde(default = "default_half_spread")]
    half_spread: f64,
    /// Position skew factor for inventory management
    #[serde(default = "default_skew")]
    skew: f64,
    /// Order lifetime in seconds before cancellation
    #[serde(default = "default_order_lifetime")]
    order_lifetime_secs: f64,
    /// Update interval in nanoseconds
    #[serde(default = "default_update_interval")]
    update_interval_ns: i64,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct RiskConfig {
    /// Maximum daily loss before stopping (in quote currency)
    #[serde(default)]
    max_daily_loss: Option<f64>,
    /// Maximum drawdown percentage before stopping
    #[serde(default)]
    max_drawdown_pct: Option<f64>,
    /// Kill switch - stop all trading if true
    #[serde(default)]
    kill_switch: bool,
}

// Default value functions
fn default_grid_num() -> usize { 3 }
fn default_tick_size() -> f64 { 0.1 }
fn default_lot_size() -> f64 { 0.001 }
fn default_half_spread() -> f64 { 0.0005 }
fn default_skew() -> f64 { 0.01 }
fn default_order_lifetime() -> f64 { 30.0 }
fn default_update_interval() -> i64 { 5_000_000_000 } // 5 seconds

// =============================================================================
// Strategy State
// =============================================================================

/// Strategy state tracking
struct StrategyState {
    /// Current position
    position: f64,
    /// Last mid price
    last_mid: f64,
    /// Realized PnL
    realized_pnl: f64,
    /// Number of trades
    trade_count: u64,
    /// Last update timestamp
    last_update_ts: i64,
}

impl Default for StrategyState {
    fn default() -> Self {
        Self {
            position: 0.0,
            last_mid: 0.0,
            realized_pnl: 0.0,
            trade_count: 0,
            last_update_ts: 0,
        }
    }
}

// =============================================================================
// Main Entry Point
// =============================================================================

fn main() -> Result<()> {
    // Parse CLI arguments
    let args = Args::parse();

    // Initialize logging
    init_logging(args.verbose);

    info!("========================================");
    info!(" Template Strategy Starting");
    info!("========================================");
    info!("Config: {}", args.config);
    info!("Symbol: {}", args.symbol);
    info!("Connector: {}", args.connector);

    // Load configuration
    let config = load_config(&args.config)?;
    info!("Configuration loaded successfully");
    debug!("Config: {:?}", config);

    // Check kill switch
    if config.risk.kill_switch {
        warn!("Kill switch is enabled - strategy will not trade");
    }

    // Build and run the live bot
    run_strategy(&args, &config)?;

    info!("Strategy shutdown complete");
    Ok(())
}

// =============================================================================
// Strategy Implementation
// =============================================================================

fn run_strategy(args: &Args, config: &Config) -> Result<()> {
    // Build the LiveBot
    // The bot connects to the connector via IPC (iceoryx2)
    let mut bot: LiveBot<HashMapMarketDepth> = LiveBotBuilder::new()
        .register(&args.connector, &args.symbol)
        .depth(|builder| {
            // Configure market depth
            // Use HashMapMarketDepth for general purpose
            // Use ROIVectorMarketDepth for bounded price range instruments
            builder.with_hash_map_market_depth()
        })
        .build()
        .context("Failed to build LiveBot")?;

    info!("LiveBot connected to connector: {}", args.connector);

    // Initialize strategy state
    let mut state = StrategyState::default();

    // Main strategy loop
    info!("Entering main strategy loop...");

    loop {
        // 1. Process events (market data, order updates)
        // This is non-blocking and returns immediately if no events
        match bot.elapse(Duration::from_millis(100)) {
            Ok(true) => {
                // Events were processed
            }
            Ok(false) => {
                // No events, continue
            }
            Err(e) => {
                error!("Error processing events: {}", e);
                continue;
            }
        }

        // 2. Get current timestamp
        let current_ts = bot.current_timestamp();

        // 3. Check if it's time to update quotes
        if current_ts - state.last_update_ts < config.params.update_interval_ns {
            continue;
        }
        state.last_update_ts = current_ts;

        // 4. Check risk limits
        if config.risk.kill_switch {
            continue;
        }

        // 5. Get market data
        let depth = bot.depth(0);
        let best_bid = depth.best_bid();
        let best_ask = depth.best_ask();

        // Skip if no valid quotes
        if best_bid <= 0.0 || best_ask <= 0.0 {
            debug!("No valid quotes, skipping update");
            continue;
        }

        let mid_price = (best_bid + best_ask) / 2.0;
        state.last_mid = mid_price;

        // 6. Get current position
        state.position = bot.position(0);

        // 7. Calculate quotes (this is where your strategy logic goes)
        let (bid_price, ask_price) = calculate_quotes(
            mid_price,
            state.position,
            config,
        );

        debug!(
            "Mid: {:.2}, Pos: {:.4}, Bid: {:.2}, Ask: {:.2}",
            mid_price, state.position, bid_price, ask_price
        );

        // 8. Cancel stale orders
        cancel_stale_orders(&mut bot, current_ts, config.params.order_lifetime_secs)?;

        // 9. Submit new orders (if within position limits)
        if state.position.abs() < config.global.max_position {
            submit_orders(&mut bot, bid_price, ask_price, config)?;
        } else {
            debug!("Position limit reached: {:.4}", state.position);
        }

        // 10. Log periodic status
        if state.trade_count % 100 == 0 && state.trade_count > 0 {
            info!(
                "Status: Pos={:.4}, PnL={:.2}, Trades={}",
                state.position, state.realized_pnl, state.trade_count
            );
        }
    }
}

/// Calculate bid and ask prices based on strategy logic
fn calculate_quotes(
    mid_price: f64,
    position: f64,
    config: &Config,
) -> (f64, f64) {
    let half_spread = config.params.half_spread;
    let skew = config.params.skew;
    let tick_size = config.global.tick_size;

    // Basic market making with inventory skew
    // Skew quotes away from current position to reduce inventory
    let position_skew = -position * skew;

    let bid_price = mid_price * (1.0 - half_spread + position_skew);
    let ask_price = mid_price * (1.0 + half_spread + position_skew);

    // Round to tick size
    let bid_price = (bid_price / tick_size).floor() * tick_size;
    let ask_price = (ask_price / tick_size).ceil() * tick_size;

    (bid_price, ask_price)
}

/// Cancel orders older than lifetime
fn cancel_stale_orders(
    bot: &mut LiveBot<HashMapMarketDepth>,
    current_ts: i64,
    lifetime_secs: f64,
) -> Result<()> {
    let lifetime_ns = (lifetime_secs * 1_000_000_000.0) as i64;

    // Get orders to cancel
    let orders_to_cancel: Vec<u64> = bot
        .orders(0)
        .iter()
        .filter(|(_, order)| {
            order.status == Status::New
                && current_ts - order.local_timestamp > lifetime_ns
        })
        .map(|(id, _)| *id)
        .collect();

    // Cancel them
    for order_id in orders_to_cancel {
        if let Err(e) = bot.cancel(0, order_id, false) {
            warn!("Failed to cancel order {}: {}", order_id, e);
        }
    }

    Ok(())
}

/// Submit bid and ask orders
fn submit_orders(
    bot: &mut LiveBot<HashMapMarketDepth>,
    bid_price: f64,
    ask_price: f64,
    config: &Config,
) -> Result<()> {
    let qty = config.global.order_qty;
    let time_in_force = TimeInForce::GTX; // Post-only orders

    // Submit bid order
    if let Err(e) = bot.submit_buy_order(
        0,
        0, // Use auto-generated order ID
        bid_price,
        qty,
        time_in_force,
        OrdType::Limit,
        false, // wait
    ) {
        warn!("Failed to submit bid order: {}", e);
    }

    // Submit ask order
    if let Err(e) = bot.submit_sell_order(
        0,
        0, // Use auto-generated order ID
        ask_price,
        qty,
        time_in_force,
        OrdType::Limit,
        false, // wait
    ) {
        warn!("Failed to submit ask order: {}", e);
    }

    Ok(())
}

// =============================================================================
// Utility Functions
// =============================================================================

/// Initialize logging with tracing
fn init_logging(verbose: bool) {
    use tracing_subscriber::{fmt, EnvFilter};

    let filter = if verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info")
    };

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .init();
}

/// Load configuration from TOML file
fn load_config(path: &str) -> Result<Config> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path))?;

    let config: Config = toml::from_str(&content)
        .with_context(|| format!("Failed to parse config file: {}", path))?;

    // Validate configuration
    validate_config(&config)?;

    Ok(config)
}

/// Validate configuration values
fn validate_config(config: &Config) -> Result<()> {
    if config.global.order_qty <= 0.0 {
        anyhow::bail!("order_qty must be positive");
    }
    if config.global.max_position <= 0.0 {
        anyhow::bail!("max_position must be positive");
    }
    if config.params.half_spread <= 0.0 {
        anyhow::bail!("half_spread must be positive");
    }
    Ok(())
}

// =============================================================================
// Optional: Order Recording Integration
// =============================================================================

#[cfg(feature = "recording")]
mod recording {
    use super::*;
    use order_recorder::hook::create_order_hook;

    /// Create a LiveBot with order recording enabled
    pub fn build_bot_with_recording(
        connector: &str,
        symbol: &str,
        tick_size: f64,
    ) -> Result<LiveBot<HashMapMarketDepth>> {
        // Create order recording hook
        let hook_ctx = create_order_hook(
            "hft_recording",  // IPC service name (must match order-recorder config)
            "template_strategy".to_string(),
            symbol.to_string(),
            tick_size,
        )?;

        let ctx = Arc::new(hook_ctx);
        let ctx_clone = ctx.clone();

        // Build bot with recording hook
        let bot = LiveBotBuilder::new()
            .register(connector, symbol)
            .depth(|builder| builder.with_hash_map_market_depth())
            .order_recv_hook(move |prev, new| {
                ctx_clone.handle_order(prev, new);
                Ok(())
            })
            .build()?;

        Ok(bot)
    }
}
