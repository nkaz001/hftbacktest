//! Connector library for exchange integrations
//!
//! This module exports the connector implementations for various exchanges.

#[cfg(feature = "binancefutures")]
pub mod binancefutures;
#[cfg(feature = "binancepapi")]
pub mod binancepapi;
#[cfg(feature = "binancespot")]
pub mod binancespot;
#[cfg(feature = "bybit")]
pub mod bybit;
#[cfg(feature = "bybit")]
pub mod bybit_rest;

pub mod connector;
pub mod utils;

#[cfg(feature = "recording")]
pub mod sync;
