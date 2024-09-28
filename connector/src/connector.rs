use std::fmt::Debug;

use hftbacktest::types::{LiveEvent, Order};
use tokio::sync::mpsc::UnboundedSender;

/// Provides the instrument data
#[derive(Clone)]
pub struct Instrument {
    pub symbol: String,
    pub tick_size: f64,
}

/// A message will be received by the publisher thread and then published to the bots.
pub enum PublishMessage {
    LiveEvent(LiveEvent),
    LiveEventsWithId {
        id: u64,
        events: Vec<LiveEvent>,
    },
    AddInstrument {
        id: u64,
        symbol: String,
        tick_size: f64,
    },
}

/// Provides a build function for the Connector.
pub trait ConnectorBuilder {
    type Error: Debug;

    fn build_from(config: &str) -> Result<Self, Self::Error>
    where
        Self: Sized;
}

/// Provides an interface for connecting with an exchange or broker for a live bot.
pub trait Connector {
    /// Adds an asset to be traded through this connector.
    fn add(&mut self, symbol: String, tick_size: f64, id: u64, tx: UnboundedSender<PublishMessage>);

    /// Runs the connector, establishing the connection and preparing to exchange information such
    /// as data feed and orders. This method should not block, and any response should be returned
    /// through the channel using [`PublishMessage`]. The returned error should not be related to the
    /// exchange; instead, it should indicate a connector internal error.
    fn run(&mut self, tx: UnboundedSender<PublishMessage>);

    /// Submits a new order. This method should not block, and the response should be returned
    /// through the channel using [`PublishMessage`]. The returned error should not be related to the
    /// exchange; instead, it should indicate a connector internal error.
    fn submit(&self, symbol: String, order: Order, tx: UnboundedSender<PublishMessage>);

    /// Cancels an open order. This method should not block, and the response should be returned
    /// through the channel using [`PublishMessage`]. The returned error should not be related to the
    /// exchange; instead, it should indicate a connector internal error.
    fn cancel(&self, symbol: String, order: Order, tx: UnboundedSender<PublishMessage>);
}
