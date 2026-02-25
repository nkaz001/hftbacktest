use std::{
    fmt::Debug,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

use hftbacktest::types::{LiveEvent, Order};
use tokio::sync::mpsc::UnboundedSender;

/// Type alias for boxed futures used in async trait methods
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A message will be received by the publisher thread and then published to the bots.
pub enum PublishEvent {
    BatchStart(u64),
    BatchEnd(u64),
    LiveEvent(LiveEvent),
    RegisterInstrument {
        id: u64,
        symbol: String,
        tick_size: f64,
        lot_size: f64,
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
    /// Registers an instrument to be traded through this connector.
    fn register(&mut self, symbol: String);

    /// Returns an [`OrderManager`].
    fn order_manager(&self) -> Arc<Mutex<dyn GetOrders + Send + 'static>>;

    /// Runs the connector, establishing the connection and preparing to exchange information such
    /// as data feed and orders. This method should not block, and any response should be returned
    /// through the channel using [`PublishEvent`]. The returned error should not be related to the
    /// exchange; instead, it should indicate a connector internal error.
    fn run(&mut self, tx: UnboundedSender<PublishEvent>);

    /// Submits a new order. This method should not block, and the response should be returned
    /// through the channel using [`PublishEvent`]. The returned error should not be related to the
    /// exchange; instead, it should indicate a connector internal error.
    fn submit(&self, symbol: String, order: Order, tx: UnboundedSender<PublishEvent>);

    /// Cancels an open order. This method should not block, and the response should be returned
    /// through the channel using [`PublishEvent`]. The returned error should not be related to the
    /// exchange; instead, it should indicate a connector internal error.
    fn cancel(&self, symbol: String, order: Order, tx: UnboundedSender<PublishEvent>);

    /// Gracefully shuts down the connector, canceling all open orders for the given symbols.
    /// This is called when the connector is about to exit.
    fn shutdown(&self, symbols: Vec<String>) -> BoxFuture<'_, ()>;

    /// Resolves tick_size and lot_size for a symbol from the exchange.
    /// Returns (tick_size, lot_size) or None if not supported or symbol not found.
    /// Used when the bot sends tick_size=0.0 or lot_size=0.0 (auto-resolve convention).
    fn resolve_instrument_info(&self, _symbol: &str) -> BoxFuture<'_, Option<(f64, f64)>> {
        Box::pin(async { None })
    }

    /// Notifies the connector of the resolved tick_size for a symbol.
    /// Called after auto-resolving instrument info so the ordermanager can use correct
    /// tick_size for price_tick conversions (avoiding division by zero when bot sends tick_size=0).
    fn set_symbol_tick_size(&self, _symbol: &str, _tick_size: f64) {}
}

/// Provides `orders` method to get the current working orders.
pub trait GetOrders {
    fn orders(&self, symbol: Option<String>) -> Vec<Order>;
}
