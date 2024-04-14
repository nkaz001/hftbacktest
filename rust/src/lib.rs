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

use thiserror::Error;

use crate::{
    backtest::state::StateValues,
    ty::{Event, OrdType, Order, TimeInForce},
};

/// Defines backtesting features.
pub mod backtest;

/// Defines exchange connectors
pub mod connector;

/// Defines a market depth to build the order book from the feed data.
pub mod depth;

/// Defines live bot features.
pub mod live;

/// Defines types.
pub mod ty;

#[derive(Error, Debug)]
pub enum BuildError {
    #[error("`{0}` is required")]
    BuilderIncomplete(&'static str),
    #[error("{0}")]
    InvalidArgument(&'static str),
    #[error("`{0}/{1}` already exists")]
    Duplicate(String, String),
    #[error("`{0}` is not found")]
    ConnectorNotFound(String),
    #[error("{0:?}")]
    Error(#[from] anyhow::Error),
}

/// Provides an interface for a backtester or a bot.
pub trait Interface<Q, MD>
where
    Q: Sized + Clone,
{
    type Error;

    /// In backtesting, this timestamp reflects the time at which the backtesting is conducted
    /// within the provided data. In a live bot, it's literally the current local timestamp.
    fn current_timestamp(&self) -> i64;

    /// Retrieve the position you currently hold.
    ///
    /// * `asset_no` - Asset number from which the position will be retrieved.
    fn position(&self, asset_no: usize) -> f64;

    fn state_values(&self, asset_no: usize) -> StateValues;

    /// Gets the market depth.
    ///
    /// * `asset_no` - Asset number from which the market depth will be retrieved.
    fn depth(&self, asset_no: usize) -> &MD;

    /// Gets the last market trades.
    ///
    /// * `asset_no` - Asset number from which the last market trades will be retrieved.
    fn trade(&self, asset_no: usize) -> &Vec<Event>;

    /// Clears the last market trades from the buffer.
    ///
    /// * `asset_no` - Asset number at which this command will be executed. If `None`, all last
    ///                trades in any assets will be cleared.
    fn clear_last_trades(&mut self, asset_no: Option<usize>);

    /// Gets [`Order`]s.
    ///
    /// * `asset_no` - Asset number from which orders will be retrieved.
    fn orders(&self, asset_no: usize) -> &HashMap<i64, Order<Q>>;

    /// Places a buy order.
    ///
    /// * `asset_no` - Asset number at which this command will be executed.
    /// * `order_id` - The unique order ID; there should not be any existing order with the same ID
    ///                on both local and exchange sides.
    /// * `price` - Order price.
    /// * `qty` - Quantity to buy.
    /// * `time_in_force` - Available [`TimeInForce`] options vary depending on the exchange model.
    ///                     See to the exchange model for details.
    ///
    ///  * `order_type` - Available [`OrdType`] options vary depending on the exchange model. See to
    ///                   the exchange model for details.
    ///
    ///  * `wait` - If true, wait until the order placement response is received.
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

    /// Places a sell order.
    ///
    /// * `asset_no` - Asset number at which this command will be executed.
    /// * `order_id` - The unique order ID; there should not be any existing order with the same ID
    ///                on both local and exchange sides.
    /// * `price` - Order price.
    /// * `qty` - Quantity to buy.
    /// * `time_in_force` - Available [`TimeInForce`] options vary depending on the exchange model.
    ///                     See to the exchange model for details.
    ///
    ///  * `order_type` - Available [`OrdType`] options vary depending on the exchange model. See to
    ///                   the exchange model for details.
    ///
    ///  * `wait` - If true, wait until the order placement response is received.
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

    /// Cancels the specified order.
    ///
    /// * `asset_no` - Asset number at which this command will be executed.
    /// * `order_id` - Order ID to cancel.
    /// * `wait` - If true, wait until the order placement response is received.
    fn cancel(&mut self, asset_no: usize, order_id: i64, wait: bool) -> Result<bool, Self::Error>;

    /// Clears inactive orders from the local [`orders`] whose status is neither [`Status::New`] nor
    /// [`Status::PartiallyFilled`].
    fn clear_inactive_orders(&mut self, asset_no: Option<usize>);

    /// Elapses the specified duration.
    ///
    /// Args:
    /// * `duration` - Duration to elapse. Nanoseconds is the default unit. However, unit should be
    ///                the same as the data's timestamp unit.
    ///
    /// Returns:
    ///   `Ok(true)` if the method reaches the specified timestamp within the data. If the end of
    ///   the data is reached before the specified timestamp, it returns `Ok(false)`.
    fn elapse(&mut self, duration: i64) -> Result<bool, Self::Error>;

    /// Elapses time only in backtesting. In live mode, it is ignored.
    ///
    /// The [`elapse`] method exclusively manages time during backtesting, meaning that factors such
    /// as computing time are not properly accounted for. So, this method can be utilized to
    /// simulate such processing times.
    ///
    /// Args:
    /// * `duration` - Duration to elapse. Nanoseconds is the default unit. However, unit should be
    ///                the same as the data's timestamp unit.
    ///
    /// Returns:
    ///   `Ok(true)` if the method reaches the specified timestamp within the data. If the end of
    ///   the data is reached before the specified timestamp, it returns `Ok(false)`.
    fn elapse_bt(&mut self, duration: i64) -> Result<bool, Self::Error>;

    /// Closes this backtester or bot.
    fn close(&mut self) -> Result<(), Self::Error>;
}

/// Gets price precision.
///
/// * `tick_size` - This should not be a computed value.
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
