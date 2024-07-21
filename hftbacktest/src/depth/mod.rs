use std::collections::HashMap;

pub use btreemarketdepth::BTreeMarketDepth;
pub use hashmapmarketdepth::HashMapMarketDepth;
pub use roivectormarketdepth::ROIVectorMarketDepth;

use crate::{
    backtest::reader::{Data, POD},
    prelude::Side,
};

mod btreemarketdepth;
mod hashmapmarketdepth;
mod roivectormarketdepth;

#[cfg(any(feature = "unstable_fuse", doc))]
mod fuse;

#[cfg(any(feature = "unstable_fuse", doc))]
pub use fuse::FusedHashMapMarketDepth;

use crate::types::OrderId;

/// Represents no best bid.
pub const INVALID_MIN: i64 = i64::MIN;

/// Represents no best ask.
pub const INVALID_MAX: i64 = i64::MAX;

/// Provides MarketDepth interface.
pub trait MarketDepth {
    /// Returns the best bid price.
    fn best_bid(&self) -> f64;

    /// Returns the best ask price.
    fn best_ask(&self) -> f64;

    /// Returns the best bid price in ticks.
    fn best_bid_tick(&self) -> i64;

    /// Returns the best ask price in ticks.
    fn best_ask_tick(&self) -> i64;

    /// Returns the tick size.
    fn tick_size(&self) -> f64;

    /// Returns the lot size.
    fn lot_size(&self) -> f64;

    /// Returns the quantity at the bid market depth for a given price in ticks.
    fn bid_qty_at_tick(&self, price_tick: i64) -> f64;

    /// Returns the quantity at the ask market depth for a given price in ticks.
    fn ask_qty_at_tick(&self, price_tick: i64) -> f64;
}

/// Provides Level2-specific market depth functions.
pub trait L2MarketDepth {
    /// Updates the bid-side market depth and returns a tuple containing (the price in ticks,
    /// the previous best bid price in ticks, the current best bid price in ticks, the previous
    /// quantity at the price, the current quantity at the price, and the timestamp).
    ///
    /// If there is no market depth and thus no best bid, [`INVALID_MIN`] is assigned to the price
    /// in ticks of the tuple returned.
    fn update_bid_depth(
        &mut self,
        price: f64,
        qty: f64,
        timestamp: i64,
    ) -> (i64, i64, i64, f64, f64, i64);

    /// Updates the ask-side market depth and returns a tuple containing (the price in ticks,
    /// the previous best bid price in ticks, the current best bid price in ticks, the previous
    /// quantity at the price, the current quantity at the price, and the timestamp).
    ///
    /// If there is no market depth and thus no best ask, [`INVALID_MAX`] is assigned to the price
    /// in ticks of the tuple returned.
    fn update_ask_depth(
        &mut self,
        price: f64,
        qty: f64,
        timestamp: i64,
    ) -> (i64, i64, i64, f64, f64, i64);

    /// Clears the market depth. If the `side` is neither [crate::types::BUY] nor [crate::types::SELL],
    /// both sides are cleared. In this case, `clear_upto_price` is ignored.
    fn clear_depth(&mut self, side: i64, clear_upto_price: f64);
}

/// Provides a method to initialize the `MarketDepth` from the given snapshot data, such as
/// Start-Of-Day snapshot or End-Of-Day snapshot, for backtesting purpose.
pub trait ApplySnapshot<EventT>
where
    EventT: POD + Clone,
{
    /// Applies the snapshot from the given data to this market depth.
    fn apply_snapshot(&mut self, data: &Data<EventT>);

    /// Returns the current market depth as the depth snapshot events.
    fn snapshot(&self) -> Vec<EventT>;
}

/// Level3 order from the market feed.
#[derive(Debug)]
pub struct L3Order {
    pub order_id: OrderId,
    pub side: Side,
    pub price_tick: i64,
    pub qty: f64,
    pub timestamp: i64,
}

/// Provides Level3-specific market depth functions.
pub trait L3MarketDepth: MarketDepth {
    type Error;

    /// Adds a buy order to the order book and returns a tuple containing (the previous best bid
    /// in ticks, the current best bid in ticks).
    fn add_buy_order(
        &mut self,
        order_id: OrderId,
        px: f64,
        qty: f64,
        timestamp: i64,
    ) -> Result<(i64, i64), Self::Error>;

    /// Adds a sell order to the order book and returns a tuple containing (the previous best ask
    ///  in ticks, the current best ask in ticks).
    fn add_sell_order(
        &mut self,
        order_id: OrderId,
        px: f64,
        qty: f64,
        timestamp: i64,
    ) -> Result<(i64, i64), Self::Error>;

    /// Deletes the order in the order book.
    fn delete_order(
        &mut self,
        order_id: OrderId,
        timestamp: i64,
    ) -> Result<(i64, i64, i64), Self::Error>;

    /// Modifies the order in the order book and returns a tuple containing (side, the previous best
    /// in ticks, the current best in ticks).
    fn modify_order(
        &mut self,
        order_id: OrderId,
        px: f64,
        qty: f64,
        timestamp: i64,
    ) -> Result<(i64, i64, i64), Self::Error>;

    /// Clears the market depth. If the `side` is neither [crate::types::BUY] nor
    /// [crate::types::SELL], both sides are cleared.
    fn clear_depth(&mut self, side: i64);

    /// Returns the orders held in the order book.
    fn orders(&self) -> &HashMap<OrderId, L3Order>;
}

pub trait L1MarketDepth {
    fn update_best_bid(
        &mut self,
        px: f64,
        qty: f64,
        timestamp: i64,
    ) -> (i64, i64, i64, f64, f64, i64);

    fn update_best_ask(
        &mut self,
        px: f64,
        qty: f64,
        timestamp: i64,
    ) -> (i64, i64, i64, f64, f64, i64);
}
