//! Conversion functions for order recording
//!
//! This module provides functions to convert hftbacktest Order types
//! to order-recorder event types.

use crate::events::{FillEvent, OrderStatus, OrderUpdate, Side};
use hftbacktest::types::{Order, Side as HftSide, Status as HftStatus};

/// Maximum reasonable latency threshold (5 seconds)
/// Values above this indicate the local_timestamp wasn't properly updated
pub const MAX_REASONABLE_LATENCY_NS: i64 = 5_000_000_000;

/// Convert hftbacktest Side to order-recorder Side
pub fn convert_side(side: HftSide) -> Side {
    match side {
        HftSide::Buy => Side::Buy,
        HftSide::Sell => Side::Sell,
        HftSide::None | HftSide::Unsupported => Side::Buy, // Default
    }
}

/// Convert hftbacktest Status to order-recorder OrderStatus
pub fn convert_status(status: HftStatus) -> OrderStatus {
    match status {
        HftStatus::New => OrderStatus::New,
        HftStatus::PartiallyFilled => OrderStatus::PartiallyFilled,
        HftStatus::Filled => OrderStatus::Filled,
        HftStatus::Canceled => OrderStatus::Canceled,
        HftStatus::Rejected => OrderStatus::Rejected,
        HftStatus::Expired => OrderStatus::Expired,
        HftStatus::None | HftStatus::Replaced | HftStatus::Unsupported => OrderStatus::New,
    }
}

/// Calculate latency for first acknowledgment from exchange.
///
/// Returns the latency in nanoseconds if this is the first acknowledgment
/// (prev_order.exch_timestamp == 0 AND new_order.exch_timestamp > 0),
/// otherwise returns None.
///
/// This latency should be cached and reused for all subsequent status changes.
pub fn calculate_first_ack_latency(prev_order: &Order, new_order: &Order) -> Option<i64> {
    // Check if this is first acknowledgment
    if prev_order.exch_timestamp == 0
        && new_order.exch_timestamp > 0
        && new_order.local_timestamp > 0
    {
        let latency = new_order.exch_timestamp - new_order.local_timestamp;
        // Sanity check: latency should be reasonable
        if latency > 0 && latency < MAX_REASONABLE_LATENCY_NS {
            return Some(latency);
        }
    }
    None
}

/// Resolve the best tick_size to use for price conversion.
/// Prefers the order's own tick_size (set from instrument) if non-zero,
/// falls back to the context-provided tick_size.
/// This handles the race condition where the hook context tick_size hasn't been
/// updated yet but the order already carries the correct tick_size from InstrumentInfo.
fn resolve_tick_size(order: &Order, context_tick_size: f64) -> f64 {
    if order.tick_size > 0.0 {
        order.tick_size
    } else {
        context_tick_size
    }
}

/// Create an OrderUpdate from hftbacktest Order
///
/// # Arguments
/// * `prev_order` - The previous order state
/// * `new_order` - The new order state
/// * `strategy_id` - The strategy identifier
/// * `symbol` - The trading symbol
/// * `timestamp` - Current timestamp in nanoseconds
/// * `tick_size` - Tick size for price conversion (fallback if order.tick_size is 0)
/// * `cached_latency` - Cached latency from first acknowledgment (reused for all status changes)
pub fn create_order_update(
    prev_order: &Order,
    new_order: &Order,
    strategy_id: &str,
    symbol: &str,
    timestamp: i64,
    tick_size: f64,
    cached_latency: Option<i64>,
) -> OrderUpdate {
    // Use cached latency if available, otherwise try to calculate for first acknowledgment
    let latency_ns = cached_latency.unwrap_or_else(|| {
        calculate_first_ack_latency(prev_order, new_order).unwrap_or(0)
    });

    let ts = resolve_tick_size(new_order, tick_size);

    OrderUpdate {
        timestamp,
        strategy_id: strategy_id.to_string(),
        order_id: new_order.order_id,
        symbol: symbol.to_string(),
        side: convert_side(new_order.side),
        price: new_order.price_tick as f64 * ts,
        qty: new_order.qty,
        status: convert_status(new_order.status),
        prev_status: convert_status(prev_order.status),
        leaves_qty: new_order.leaves_qty,
        exec_qty: new_order.exec_qty,
        exec_price: new_order.exec_price_tick as f64 * ts,
        exch_time: new_order.exch_timestamp,
        latency_ns,
    }
}

/// Check if a fill occurred between two order states
///
/// Returns the fill quantity if there was a fill, None otherwise
pub fn detect_fill(prev_order: &Order, new_order: &Order) -> Option<f64> {
    let fill_qty = new_order.exec_qty - prev_order.exec_qty;
    if fill_qty > 1e-10 {
        Some(fill_qty)
    } else {
        None
    }
}

/// Create a FillEvent from order state change
///
/// # Arguments
/// * `prev_order` - The previous order state
/// * `new_order` - The new order state
/// * `fill_qty` - The quantity filled
/// * `strategy_id` - The strategy identifier
/// * `symbol` - The trading symbol
/// * `timestamp` - Current timestamp in nanoseconds
/// * `mid_price` - Current mid price for slippage calculation
/// * `fill_id` - Unique fill ID
/// * `tick_size` - Tick size for price conversion
pub fn create_fill_event(
    _prev_order: &Order,
    new_order: &Order,
    fill_qty: f64,
    strategy_id: &str,
    symbol: &str,
    timestamp: i64,
    _mid_price: f64,
    fill_id: u64,
    tick_size: f64,
) -> FillEvent {
    let ts = resolve_tick_size(new_order, tick_size);
    FillEvent {
        timestamp,
        strategy_id: strategy_id.to_string(),
        fill_id,
        order_id: new_order.order_id,
        symbol: symbol.to_string(),
        side: convert_side(new_order.side),
        price: new_order.exec_price_tick as f64 * ts,
        qty: fill_qty,
        fee: 0.0, // Fee is typically not available at this point
        is_maker: new_order.maker,
        exch_time: new_order.exch_timestamp,
        mid_price: new_order.exec_price_tick as f64 * ts, // Use fill price as mid_price
    }
}

/// Helper to get the round-trip latency from an order
///
/// Returns the latency in nanoseconds, or 0 if timestamps are not available
pub fn get_latency_ns(order: &Order) -> i64 {
    if order.exch_timestamp > 0 && order.local_timestamp > 0 {
        order.exch_timestamp - order.local_timestamp
    } else {
        0
    }
}
