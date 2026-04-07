//! Skipped in this task on purpose.
//!
//! The original target was to verify queue-model notifications after exchange-side BBO backoff.
//! Current public backtest APIs make depth state observable, but not the internal queue callback
//! sequence needed to prove `on_bid_qty_chg` fired for every cleared level without adding new test
//! hooks or synthetic file-format plumbing. The deterministic BBO coverage for this task is
//! therefore implemented in direct depth tests and in `bbo_snapshot.rs`.
