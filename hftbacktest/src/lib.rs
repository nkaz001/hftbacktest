#![cfg_attr(docsrs, feature(doc_auto_cfg))]

//! # HftBacktest
//!
//! This Rust framework is designed for developing and running high-frequency trading and
//! market-making strategies. It focuses on accounting for both feed and order latencies, as well as
//! the order queue position for order fill simulation. The framework aims to provide more accurate
//! market replay-based backtesting, based on full order book and trade tick feed data. You can also
//! run the live bot using the same algo code.
//!
//! ## Key Features
//! - Complete tick-by-tick simulation with a variable time interval.
//! - Full order book reconstruction based on Level-2 feeds(Market-By-Price) and Level-3 feeds(Market-By-Order).
//! - Backtest accounting for both feed and order latency, using provided models or your own custom model.
//! - Order fill simulation that takes into account the order queue position, using provided models or your own custom model.
//! - Backtesting of multi-asset and multi-exchange models
//! - Deployment of a live trading bot using the same algo code
//!
//!
//! ## Feature flags
//!
//! Currently, `default` enables `backtest`, `live` features.
//!
//! - `backtest`: Enables backtesting features.
//! - `live`: Enables a live trading bot.
//! - `s3`: Enables accessing data file from S3.
//!

/// Provides backtesting features.
#[cfg(any(feature = "backtest", doc))]
pub mod backtest;

/// Provides market depth implementations.
pub mod depth;

/// Provides live trading bot features.
#[cfg(feature = "live")]
pub mod live;

/// Defines HftBacktest types.
pub mod types;

/// Provides common types.
pub mod prelude;

/// Provides utilities.
mod utils;
