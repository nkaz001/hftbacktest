use std::collections::{HashMap, hash_map::Entry};

use tracing::info;

use crate::{depth::MarketDepth, prelude::Bot, types::Recorder};

/// Provides logging of the live strategy's state values.
#[derive(Default)]
pub struct LoggingRecorder {
    position: HashMap<usize, f64>,
    symbol: String,
    asset_no: usize,
}

impl Recorder for LoggingRecorder {
    type Error = ();

    fn record<MD, I>(&mut self, hbt: &I) -> Result<(), Self::Error>
    where
        MD: MarketDepth,
        I: Bot<MD>,
    {
        let position = hbt.position(self.asset_no);

        let updated = match self.position.entry(self.asset_no) {
            Entry::Occupied(mut entry) => {
                let prev_position = entry.get();
                if *prev_position != position {
                    *entry.get_mut() = position;
                    true
                } else {
                    false
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(position);
                true
            }
        };

        if updated {
            info!(
                asset_no = %self.asset_no,
                symbol = %self.symbol,
                %position,
                "Position updated"
            );
        }

        Ok(())
    }
}

impl LoggingRecorder {
    /// Constructs an instance of `LoggingRecorder` for a single symbol.
    pub fn new(symbol: String, asset_no: usize) -> Self {
        Self {
            position: HashMap::new(),
            symbol,
            asset_no,
        }
    }
}
