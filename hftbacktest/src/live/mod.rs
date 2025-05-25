use std::collections::HashMap;

pub use bot::{BotError, LiveBot, LiveBotBuilder};
pub use recorder::LoggingRecorder;

use crate::{
    prelude::StateValues,
    types::{Event, Order, OrderId},
};

mod bot;
pub mod ipc;
mod recorder;

/// Provides asset information for internal use.
pub struct Instrument<MD> {
    connector_name: String,
    symbol: String,
    tick_size: f64,
    lot_size: f64,
    depth: MD,
    last_trades: Vec<Event>,
    orders: HashMap<OrderId, Order>,
    last_feed_latency: Option<(i64, i64)>,
    last_order_latency: Option<(i64, i64, i64)>,
    state: StateValues,
}

impl<MD> Instrument<MD> {
    /// * `connector_name` - Name of the [`Connector`], which is registered by
    ///   [`register()`](`LiveBotBuilder::register()`), through which this asset will be traded.
    /// * `symbol` - Symbol of the asset. You need to check with the [`Connector`] which symbology
    ///   is used.
    /// * `tick_size` - The minimum price fluctuation.
    /// * `lot_size` -  The minimum trade size.
    /// * `depth` -  The market depth.
    pub fn new(
        connector_name: &str,
        symbol: &str,
        tick_size: f64,
        lot_size: f64,
        depth: MD,
        last_trades_capacity: usize,
    ) -> Self {
        Self {
            connector_name: connector_name.to_string(),
            symbol: symbol.to_string(),
            tick_size,
            lot_size,
            depth,
            last_trades: Vec::with_capacity(last_trades_capacity),
            orders: Default::default(),
            last_feed_latency: None,
            last_order_latency: None,
            state: Default::default(),
        }
    }
}
