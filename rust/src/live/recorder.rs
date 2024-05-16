use std::{
    fs::File,
    io::{Error, Write},
    path::Path,
};

use tracing::info;

use crate::{
    depth::MarketDepth,
    prelude::Interface,
    types::{Recorder, StateValues},
};

/// Provides logging of the live strategy's state values.
pub struct LiveRecorder;

impl Recorder for LiveRecorder {
    type Error = Error;

    fn record<Q, MD, I>(&mut self, hbt: &mut I) -> Result<(), Self::Error>
    where
        Q: Sized + Clone,
        I: Interface<Q, MD>,
        MD: MarketDepth,
    {
        for asset_no in 0..hbt.num_assets() {
            let depth = hbt.depth(asset_no);
            let mid = (depth.best_bid() + depth.best_ask()) / 2.0;
            let state_values = hbt.state_values(asset_no);
            info!(%mid, ?state_values, "State");
        }
        Ok(())
    }
}

impl LiveRecorder {
    /// Constructs an instance of `LiveRecorder`.
    pub fn new() -> Self {
        Self {}
    }
}
