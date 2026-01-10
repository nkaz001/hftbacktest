//! Order Recording Module for HFT strategies
//!
//! This module provides:
//! - IPC message types for order/fill/position events
//! - ClickHouse writer for persistent storage
//! - Recording service that subscribes to IPC and writes to ClickHouse
//! - Conversion utilities for hftbacktest Order types
//! - Order hook context for strategy integration
//!
//! # Quick Start
//!
//! For strategies that need order recording, use the `hook` module:
//!
//! ```ignore
//! use order_recorder::hook::{create_order_hook, OrderHookContext};
//!
//! // Create order hook context (returns None if recording service unavailable)
//! let hook_ctx = create_order_hook(
//!     "hft_recording",
//!     "my_strategy".to_string(),
//!     "btcusdt".to_string(),
//!     0.1,  // tick_size
//! );
//!
//! // Use with LiveBotBuilder
//! if let Some(ctx) = hook_ctx {
//!     let ctx_clone = ctx.clone();
//!     builder = builder.order_recv_hook(move |prev, new| {
//!         ctx_clone.handle_order(prev, new);
//!         Ok(())
//!     });
//! }
//! ```

pub mod events;
pub mod writer;
pub mod service;
pub mod convert;
pub mod hook;

pub use events::*;
pub use writer::{ClickHouseWriter, WriterConfig, WriterError};
pub use service::{RecordingService, DEFAULT_MAX_PUBLISHERS};
pub use convert::*;
pub use hook::{OrderHookContext, create_order_hook};
