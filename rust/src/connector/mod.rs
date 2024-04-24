use std::sync::mpsc::Sender;

use crate::types::{LiveEvent, Order};

pub mod binancefutures;

/// Provides an interface for connecting with an exchange or broker for a live bot.
pub trait Connector {
    /// Adds an asset to be traded through this connector.
    fn add(
        &mut self,
        asset_no: usize,
        symbol: String,
        tick_size: f32,
        lot_size: f32,
    ) -> Result<(), anyhow::Error>;

    /// Runs the connector, establishing the connection and preparing to exchange information such
    /// as data feed and orders. This method should not block, and any response should be returned
    /// through the channel using [`LiveEvent`]. The returned error should not be related to the
    /// exchange; instead, it should indicate a connector internal error.
    fn run(&mut self, tx: Sender<LiveEvent>) -> Result<(), anyhow::Error>;

    /// Submits a new order. This method should not block, and the response should be returned
    /// through the channel using [`LiveEvent`]. The returned error should not be related to the
    /// exchange; instead, it should indicate a connector internal error.
    fn submit(
        &self,
        asset_no: usize,
        order: Order<()>,
        ev_tx: Sender<LiveEvent>,
    ) -> Result<(), anyhow::Error>;

    /// Cancels an open order. This method should not block, and the response should be returned
    /// through the channel using [`LiveEvent`]. The returned error should not be related to the
    /// exchange; instead, it should indicate a connector internal error.
    fn cancel(
        &self,
        asset_no: usize,
        order: Order<()>,
        ev_tx: Sender<LiveEvent>,
    ) -> Result<(), anyhow::Error>;
}
