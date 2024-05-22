use std::collections::HashMap;

use crate::{
    backtest::BacktestError,
    depth::MarketDepth,
    types::{Event, OrdType, Order, Side, StateValues, TimeInForce},
};

/// Provides local-specific interaction.
pub trait LocalProcessor<Q, MD>: Processor
where
    Q: Clone,
    MD: MarketDepth,
{
    /// Submits a new order.
    ///
    /// * `order_id` - The unique order ID; there should not be any existing order with the same ID
    ///                on both local and exchange sides.
    /// * `price` - Order price.
    /// * `qty` - Quantity to buy.
    /// * `order_type` - Available [`OrdType`] options vary depending on the exchange model. See to
    ///                   the exchange model for details.
    /// * `time_in_force` - Available [`TimeInForce`] options vary depending on the exchange model.
    ///                     See to the exchange model for details.
    /// * `current_timestamp` - The current backtesting timestamp.
    fn submit_order(
        &mut self,
        order_id: i64,
        side: Side,
        price: f32,
        qty: f32,
        order_type: OrdType,
        time_in_force: TimeInForce,
        current_timestamp: i64,
    ) -> Result<(), BacktestError>;

    /// Cancels the specified order.
    ///
    /// * `order_id` - Order ID to cancel.
    /// * `current_timestamp` - The current backtesting timestamp.
    fn cancel(&mut self, order_id: i64, current_timestamp: i64) -> Result<(), BacktestError>;

    /// Clears inactive orders from the local orders whose status is neither
    /// [`Status::New`] nor [`Status::PartiallyFilled`].
    fn clear_inactive_orders(&mut self);

    /// Returns the position you currently hold.
    fn position(&self) -> f64;

    /// Returns the state's values such as balance, fee, and so on.
    fn state_values(&self) -> StateValues;

    /// Returns the [`MarketDepth`].
    fn depth(&self) -> &MD;

    /// Returns a hash map of order IDs and their corresponding [`Order`]s.
    fn orders(&self) -> &HashMap<i64, Order<Q>>;

    /// Returns the last market trades.
    fn trade(&self) -> &Vec<Event>;

    /// Clears the last market trades from the buffer.
    fn clear_last_trades(&mut self);
}

/// Processes the historical feed data and the order interaction.
pub trait Processor {
    /// Prepares to process the data. This is invoked when the backtesting is initiated.
    /// If successful, returns the timestamp of the first event.
    fn initialize_data(&mut self) -> Result<i64, BacktestError>;

    /// Processes the data. This is invoked when the backtesting time reaches the timestamp of the
    /// event to be processed in the data.
    /// If successful, returns the timestamp of the next event.
    fn process_data(&mut self) -> Result<(i64, i64), BacktestError>;

    /// Processes an order upon receipt. This is invoked when the backtesting time reaches the order
    /// receipt timestamp.
    /// Returns Ok(true) if the order with `wait_resp_order_id` is received and processed.
    fn process_recv_order(
        &mut self,
        timestamp: i64,
        wait_resp_order_id: i64
    ) -> Result<bool, BacktestError>;

    /// Returns the foremost timestamp at which an order is to be received by this processor.
    fn frontmost_recv_order_timestamp(&self) -> i64;

    /// Returns the foremost timestamp at which an order sent by this processor is to be received by
    /// the corresponding processor.
    fn frontmost_send_order_timestamp(&self) -> i64;
}
