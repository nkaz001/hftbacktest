mod local;
mod nopartialfillexchange;
mod partialfillexchange;

use std::collections::HashMap;

pub use local::Local;
pub use nopartialfillexchange::NoPartialFillExchange;
pub use partialfillexchange::PartialFillExchange;

mod l3_local;

mod l3_nopartialfillexchange;

pub use l3_local::L3Local;
pub use l3_nopartialfillexchange::L3NoPartialFillExchange;

use crate::{
    backtest::BacktestError,
    depth::MarketDepth,
    prelude::{Event, OrdType, Order, OrderId, Side, StateValues, TimeInForce},
};

/// Provides local-specific interaction.
pub trait LocalProcessor<MD>: Processor
where
    MD: MarketDepth,
{
    /// Submits a new order.
    ///
    /// * `order_id` - The unique order ID; there should not be any existing order with the same ID
    ///   on both local and exchange sides.
    /// * `price` - Order price.
    /// * `qty` - Quantity to buy.
    /// * `order_type` - Available [`OrdType`] options vary depending on the exchange model. See to
    ///   the exchange model for details.
    /// * `time_in_force` - Available [`TimeInForce`] options vary depending on the exchange model.
    ///   See to the exchange model for details.
    /// * `current_timestamp` - The current backtesting timestamp.
    #[allow(clippy::too_many_arguments)]
    fn submit_order(
        &mut self,
        order_id: OrderId,
        side: Side,
        price: f64,
        qty: f64,
        order_type: OrdType,
        time_in_force: TimeInForce,
        current_timestamp: i64,
    ) -> Result<(), BacktestError>;

    /// Modifies an open order.
    ///
    /// * `order_id` - Order ID to modify.
    /// * `price` - Order price.
    /// * `qty` - Quantity to buy.
    /// * `current_timestamp` - The current backtesting timestamp.
    fn modify(
        &mut self,
        order_id: OrderId,
        price: f64,
        qty: f64,
        current_timestamp: i64,
    ) -> Result<(), BacktestError>;

    /// Cancels an open order.
    ///
    /// * `order_id` - Order ID to cancel.
    /// * `current_timestamp` - The current backtesting timestamp.
    fn cancel(&mut self, order_id: OrderId, current_timestamp: i64) -> Result<(), BacktestError>;

    /// Clears inactive orders from the local orders whose status is neither
    /// [`Status::New`](crate::types::Status::New) nor
    /// [`Status::PartiallyFilled`](crate::types::Status::PartiallyFilled).
    fn clear_inactive_orders(&mut self);

    /// Returns the position you currently hold.
    fn position(&self) -> f64;

    /// Returns the state's values such as balance, fee, and so on.
    fn state_values(&self) -> &StateValues;

    /// Returns the [`MarketDepth`].
    fn depth(&self) -> &MD;

    /// Returns a hash map of order IDs and their corresponding [`Order`]s.
    fn orders(&self) -> &HashMap<OrderId, Order>;

    /// Returns the last market trades.
    fn last_trades(&self) -> &[Event];

    /// Clears the last market trades from the buffer.
    fn clear_last_trades(&mut self);

    /// Returns the last feed's exchange timestamp and local receipt timestamp.
    fn feed_latency(&self) -> Option<(i64, i64)>;

    /// Returns the last order's request timestamp, exchange timestamp, and response receipt
    /// timestamp.
    fn order_latency(&self) -> Option<(i64, i64, i64)>;
}

impl<P: Processor + ?Sized> Processor for Box<P> {
    fn event_seen_timestamp(&self, event: &Event) -> Option<i64> {
        P::event_seen_timestamp(self, event)
    }

    fn process(&mut self, event: &Event) -> Result<(), BacktestError> {
        P::process(self, event)
    }

    fn process_recv_order(
        &mut self,
        timestamp: i64,
        wait_resp_order_id: Option<OrderId>,
    ) -> Result<bool, BacktestError> {
        P::process_recv_order(self, timestamp, wait_resp_order_id)
    }

    fn earliest_recv_order_timestamp(&self) -> i64 {
        P::earliest_recv_order_timestamp(self)
    }

    fn earliest_send_order_timestamp(&self) -> i64 {
        P::earliest_send_order_timestamp(self)
    }
}
/// Processes the historical feed data and the order interaction.
pub trait Processor {
    /// The time of an event as seen by this [Processor]. For a local event processor this will
    /// be the timestamp an event was seen at locally, and for an exchange processor this will
    /// be the timestamp an event was generated at on the exchange.
    ///
    /// `None` should be returned if this processor wouldn't have seen this event (i.e. it only
    /// occurred remotely).
    fn event_seen_timestamp(&self, event: &Event) -> Option<i64>;

    /// Process an event and advance the state of this processor.
    fn process(&mut self, event: &Event) -> Result<(), BacktestError>;

    /// Processes an order upon receipt. This is invoked when the backtesting time reaches the order
    /// receipt timestamp.
    /// Returns Ok(true) if the order with `wait_resp_order_id` is received and processed.
    fn process_recv_order(
        &mut self,
        timestamp: i64,
        wait_resp_order_id: Option<OrderId>,
    ) -> Result<bool, BacktestError>;

    /// Returns the foremost timestamp at which an order is to be received by this processor.
    fn earliest_recv_order_timestamp(&self) -> i64;

    /// Returns the foremost timestamp at which an order sent by this processor is to be received by
    /// the corresponding processor.
    fn earliest_send_order_timestamp(&self) -> i64;
}
