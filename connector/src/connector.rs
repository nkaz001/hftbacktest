use hftbacktest::types::{LiveEvent, Order};
use tokio::sync::mpsc::UnboundedSender;

#[derive(Clone)]
pub struct Instrument {
    pub symbol: String,
    pub tick_size: f64,
}

/// Provides an interface for connecting with an exchange or broker for a live bot.
pub trait Connector {
    /// Adds an asset to be traded through this connector.
    fn add(&mut self, symbol: String, tick_size: f64, tx: UnboundedSender<LiveEvent>);

    /// Runs the connector, establishing the connection and preparing to exchange information such
    /// as data feed and orders. This method should not block, and any response should be returned
    /// through the channel using [`LiveEvent`]. The returned error should not be related to the
    /// exchange; instead, it should indicate a connector internal error.
    fn run(&mut self, tx: UnboundedSender<LiveEvent>);

    /// Submits a new order. This method should not block, and the response should be returned
    /// through the channel using [`LiveEvent`]. The returned error should not be related to the
    /// exchange; instead, it should indicate a connector internal error.
    fn submit(&self, asset: String, order: Order, tx: UnboundedSender<LiveEvent>);

    /// Cancels an open order. This method should not block, and the response should be returned
    /// through the channel using [`LiveEvent`]. The returned error should not be related to the
    /// exchange; instead, it should indicate a connector internal error.
    fn cancel(&self, asset: String, order: Order, tx: UnboundedSender<LiveEvent>);
}
