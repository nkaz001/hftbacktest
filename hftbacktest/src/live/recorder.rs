use std::collections::{HashMap, hash_map::Entry};

use tracing::info;

use crate::{
    depth::MarketDepth,
    prelude::{Bot, get_precision},
    types::{Recorder, StateValues},
};

/// Provides logging of the live strategy's state values.
#[derive(Default)]
pub struct LoggingRecorder {
    state: HashMap<usize, (f64, StateValues)>,
}

impl Recorder for LoggingRecorder {
    type Error = ();

    fn record<MD, I>(&mut self, hbt: &I) -> Result<(), Self::Error>
    where
        MD: MarketDepth,
        I: Bot<MD>,
    {
        for asset_no in 0..hbt.num_assets() {
            let depth = hbt.depth(asset_no);
            let price_prec = get_precision(depth.tick_size());
            let mid = (depth.best_bid() + depth.best_ask()) / 2.0;
            let state_values = hbt.state_values(asset_no);
            let updated = match self.state.entry(asset_no) {
                Entry::Occupied(mut entry) => {
                    let (prev_mid, prev_state_values) = entry.get();
                    if (*prev_mid != mid) || (prev_state_values != state_values) {
                        *entry.get_mut() = (mid, state_values.clone());
                        true
                    } else {
                        false
                    }
                }
                Entry::Vacant(entry) => {
                    entry.insert((mid, state_values.clone()));
                    true
                }
            };
            if updated {
                info!(
                    %asset_no,
                    %mid,
                    bid = format!("{:.prec$}", depth.best_bid(), prec = price_prec),
                    ask = format!("{:.prec$}", depth.best_ask(), prec = price_prec),
                    ?state_values,
                    "The state of asset number {asset_no} has been updated."
                );
            }
        }
        Ok(())
    }
}

impl LoggingRecorder {
    /// Constructs an instance of `LoggingRecorder`.
    pub fn new() -> Self {
        Default::default()
    }
}
