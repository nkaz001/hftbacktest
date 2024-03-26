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
//! - Full order book reconstruction based on L2 feeds(Market-By-Price).
//! - Backtest accounting for both feed and order latency, using provided models or your own custom model.
//! - Order fill simulation that takes into account the order queue position, using provided models or your own custom model.
//! - Backtesting of multi-asset and multi-exchange models
//! - Deployment of a live trading bot using the same algo code
//!
use std::collections::HashMap;

use crate::{
    backtest::state::StateValues,
    ty::{OrdType, Order, Row, TimeInForce},
};

/// Defines backtesting features.
pub mod backtest;

/// Defines exchange connectors
pub mod connector;

/// Defines a market depth to build the order book from the feed data.
pub mod depth;

/// Defines errors.
pub mod error;

/// Defines live bot features.
pub mod live;

/// Defines types.
pub mod ty;

/// Provides an interface for a backtester or a bot.
pub trait Interface<Q, MD>
where
    Q: Sized + Clone,
{
    type Error;

    fn current_timestamp(&self) -> i64;

    fn position(&self, asset_no: usize) -> f64;

    fn state_values(&self, asset_no: usize) -> StateValues;

    fn depth(&self, asset_no: usize) -> &MD;

    fn trade(&self, asset_no: usize) -> &Vec<Row>;

    fn clear_last_trades(&mut self, asset_no: Option<usize>);

    fn orders(&self, asset_no: usize) -> &HashMap<i64, Order<Q>>;

    fn submit_buy_order(
        &mut self,
        asset_no: usize,
        order_id: i64,
        price: f32,
        qty: f32,
        time_in_force: TimeInForce,
        order_type: OrdType,
        wait: bool,
    ) -> Result<bool, Self::Error>;

    fn submit_sell_order(
        &mut self,
        asset_no: usize,
        order_id: i64,
        price: f32,
        qty: f32,
        time_in_force: TimeInForce,
        order_type: OrdType,
        wait: bool,
    ) -> Result<bool, Self::Error>;

    fn cancel(&mut self, asset_no: usize, order_id: i64, wait: bool) -> Result<bool, Self::Error>;

    fn clear_inactive_orders(&mut self, asset_no: Option<usize>);

    fn elapse(&mut self, duration: i64) -> Result<bool, Self::Error>;

    /// Elapses time only in backtesting. In live mode, it is ignored.
    ///
    /// The [`elapse`] method exclusively manages time during backtesting, meaning that factors such
    /// as computing time are not properly accounted for. So, this method can be utilized to
    /// simulate such processing times.
    fn elapse_bt(&mut self, duration: i64) -> Result<bool, Self::Error>;

    fn close(&mut self) -> Result<(), Self::Error>;
}

/// Gets price precision.
///
/// [`tick_size`] should not be a computed value.
pub fn get_precision(tick_size: f32) -> usize {
    let s = tick_size.to_string();
    let mut prec = 0;
    for (i, c) in s.chars().enumerate() {
        if c == '.' {
            prec = s.len() - i - 1;
            break;
        }
    }
    prec
}
