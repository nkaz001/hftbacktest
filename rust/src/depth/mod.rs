pub use btreemarketdepth::BTreeMarketDepth;
pub use hashmapmarketdepth::HashMapMarketDepth;

use crate::{backtest::reader::Data, types::Event};

mod btreemarketdepth;
mod hashmapmarketdepth;

#[cfg(feature = "unstable_l3")]
mod l3mbomarketdepth;

/// Represents no best bid.
pub const INVALID_MIN: i32 = i32::MIN;

/// Represents no best ask.
pub const INVALID_MAX: i32 = i32::MAX;

/// Provides MarketDepth interface.
pub trait MarketDepth {
    /// Updates the bid-side market depth and returns a tuple containing (the price in ticks,
    /// the previous best bid price in ticks, the current best bid price in ticks, the previous
    /// quantity at the price, the current quantity at the price, and the timestamp).
    ///
    /// If there is no market depth and thus no best bid, [`INVALID_MIN`] is assigned to the price
    /// in ticks of the tuple returned.
    fn update_bid_depth(
        &mut self,
        price: f32,
        qty: f32,
        timestamp: i64,
    ) -> (i32, i32, i32, f32, f32, i64);

    /// Updates the ask-side market depth and returns a tuple containing (the price in ticks,
    /// the previous best bid price in ticks, the current best bid price in ticks, the previous
    /// quantity at the price, the current quantity at the price, and the timestamp).
    ///
    /// If there is no market depth and thus no best ask, [`INVALID_MAX`] is assigned to the price
    /// in ticks of the tuple returned.
    fn update_ask_depth(
        &mut self,
        price: f32,
        qty: f32,
        timestamp: i64,
    ) -> (i32, i32, i32, f32, f32, i64);

    /// Clears the market depth. If the `side` is neither [crate::types::BUY] nor [crate::types::SELL],
    /// both sides are cleared. In this case, `clear_upto_price` is ignored.
    fn clear_depth(&mut self, side: i64, clear_upto_price: f32);

    /// Returns the best bid price.
    fn best_bid(&self) -> f32;

    /// Returns the best ask price.
    fn best_ask(&self) -> f32;

    /// Returns the best bid price in ticks.
    fn best_bid_tick(&self) -> i32;

    /// Returns the best ask price in ticks.
    fn best_ask_tick(&self) -> i32;

    /// Returns the tick size.
    fn tick_size(&self) -> f32;

    /// Returns the lot size.
    fn lot_size(&self) -> f32;

    /// Returns the quantity at the bid market depth for a given price in ticks.
    fn bid_qty_at_tick(&self, price_tick: i32) -> f32;

    /// Returns the quantity at the ask market depth for a given price in ticks.
    fn ask_qty_at_tick(&self, price_tick: i32) -> f32;
}

/// Provides a method to initialize the [`MarketDepth`] from the given snapshot data, such as
/// Start-Of-Day snapshot or End-Of-Day snapshot, for backtesting purpose.
pub trait ApplySnapshot {
    /// Applies the snapshot from the given data to this market depth.
    fn apply_snapshot(&mut self, data: &Data<Event>);
}

#[cfg(feature = "unstable_l3")]
pub trait L3MarketDepth : MarketDepth {
    type Error;

    fn add_buy_order(
        &mut self,
        order_id: i64,
        px: f32,
        qty: f32,
        timestamp: i64,
    ) -> Result<(i32, i32), Self::Error>;

    fn add_sell_order(
        &mut self,
        order_id: i64,
        px: f32,
        qty: f32,
        timestamp: i64,
    ) -> Result<(i32, i32), Self::Error>;

    fn delete_order(&mut self, order_id: i64, timestamp: i64) -> Result<(), Self::Error>;

    fn modify_order(
        &mut self,
        order_id: i64,
        px: f32,
        qty: f32,
        timestamp: i64,
    ) -> Result<(i64, i32, i32), Self::Error>;
}
