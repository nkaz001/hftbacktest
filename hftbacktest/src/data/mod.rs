//! Data conversion utilities for HftBacktest.
//!
//! This module provides functionality to convert market data from various sources
//! (like Tardis.dev or collector output) into the Event format used by HftBacktest.

mod collector;
#[cfg(feature = "backtest")]
mod streaming;
mod tardis;
mod tardis_fast;
mod validation;

#[cfg(feature = "backtest")]
pub use collector::convert_collector_to_events;
pub use collector::{CollectorConvertConfig, ConversionStats, convert_to_tardis_csv};
#[cfg(feature = "backtest")]
pub use streaming::{
    StreamLocalTimestampPolicy, StreamingTardisReader, StreamingTardisReaderConfig,
    TardisStreamDayConfig, collect_tardis_stream,
};
#[cfg(feature = "backtest")]
pub use tardis::write_npz_file;
pub use tardis::{SnapshotMode, TardisConvertConfig};

// Primary conversion API (fast implementation with bookTicker support)
#[cfg(feature = "backtest")]
pub use tardis_fast::convert_and_save_fast;
pub use tardis_fast::convert_tardis_fast;
pub use tardis_fast::parse_tardis_files;

// Convenience aliases
#[cfg(feature = "backtest")]
pub use tardis_fast::convert_and_save_fast as convert_and_save;
pub use tardis_fast::convert_fast as convert;
pub use tardis_fast::convert_tardis_fast as convert_tardis;

pub use validation::{
    argsort_by_exch_ts, argsort_by_local_ts, correct_event_order, correct_local_timestamp,
    validate_event_order,
};
