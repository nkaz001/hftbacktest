use crate::{
    backtest::{
        BacktestError,
        assettype::AssetType,
        models::{FeeModel, L3QueueModel, LatencyModel},
        order::ExchToLocal,
        proc::Processor,
        state::State,
    },
    depth::L3MarketDepth,
    prelude::OrdType,
    types::{
        BUY_EVENT,
        EXCH_ASK_ADD_ORDER_EVENT,
        EXCH_ASK_DEPTH_CLEAR_EVENT,
        EXCH_BID_ADD_ORDER_EVENT,
        EXCH_BID_DEPTH_CLEAR_EVENT,
        EXCH_CANCEL_ORDER_EVENT,
        EXCH_DEPTH_CLEAR_EVENT,
        EXCH_EVENT,
        EXCH_FILL_EVENT,
        EXCH_MODIFY_ORDER_EVENT,
        Event,
        Order,
        OrderId,
        SELL_EVENT,
        Side,
        Status,
        TimeInForce,
    },
};

/// The exchange model without partial fills.
///
/// Support order types: [OrdType::Limit](crate::types::OrdType::Limit)
/// Support time-in-force: [`TimeInForce::GTC`], [`TimeInForce::GTX`]
///
/// **Conditions for Full Execution**
///
/// Buy order in the order book
///
/// - Your order price >= the best ask price
/// - Your order price > sell trade price
/// - Your order is at the front of the queue and your order price == sell trade price
///
/// Sell order in the order book
///
/// - Your order price <= the best bid price
/// - Your order price < buy trade price
/// - Your order is at the front of the queue && your order price == buy trade price
///
/// **Liquidity-Taking Order**
///
/// Regardless of the quantity at the best, liquidity-taking orders will be fully executed at the
/// best. Be aware that this may cause unrealistic fill simulations if you attempt to execute a
/// large quantity.
///
pub struct L3NoPartialFillExchange<AT, LM, QM, MD, FM>
where
    AT: AssetType,
    LM: LatencyModel,
    QM: L3QueueModel<MD>,
    MD: L3MarketDepth,
    FM: FeeModel,
{
    depth: MD,
    state: State<AT, FM>,
    queue_model: QM,
    order_e2l: ExchToLocal<LM>,
}

impl<AT, LM, QM, MD, FM> L3NoPartialFillExchange<AT, LM, QM, MD, FM>
where
    AT: AssetType,
    LM: LatencyModel,
    QM: L3QueueModel<MD>,
    MD: L3MarketDepth,
    FM: FeeModel,
    BacktestError: From<<MD as L3MarketDepth>::Error>,
{
    /// Constructs an instance of `NoPartialFillExchange`.
    pub fn new(
        depth: MD,
        state: State<AT, FM>,
        queue_model: QM,
        order_e2l: ExchToLocal<LM>,
    ) -> Self {
        Self {
            depth,
            state,
            queue_model,
            order_e2l,
        }
    }

    fn expired(&mut self, mut order: Order, timestamp: i64) -> Result<(), BacktestError> {
        order.exec_qty = 0.0;
        order.leaves_qty = 0.0;
        order.status = Status::Expired;
        order.exch_timestamp = timestamp;

        self.order_e2l.respond(order);
        Ok(())
    }

    fn fill<const MAKE_RESPONSE: bool>(
        &mut self,
        order: &mut Order,
        timestamp: i64,
        maker: bool,
        exec_price_tick: i64,
    ) -> Result<(), BacktestError> {
        if order.status == Status::Expired
            || order.status == Status::Canceled
            || order.status == Status::Filled
        {
            return Err(BacktestError::InvalidOrderStatus);
        }

        order.maker = maker;
        if maker {
            order.exec_price_tick = order.price_tick;
        } else {
            order.exec_price_tick = exec_price_tick;
        }

        order.exec_qty = order.leaves_qty;
        order.leaves_qty = 0.0;
        order.status = Status::Filled;
        order.exch_timestamp = timestamp;

        self.state.apply_fill(order);

        if MAKE_RESPONSE {
            self.order_e2l.respond(order.clone());
        }
        Ok(())
    }

    fn fill_ask_orders_by_crossing(
        &mut self,
        prev_best_tick: i64,
        new_best_tick: i64,
        timestamp: i64,
    ) -> Result<(), BacktestError> {
        let filled = self
            .queue_model
            .on_best_bid_update(prev_best_tick, new_best_tick)?;
        for mut order in filled {
            let price_tick = order.price_tick;
            self.fill::<true>(&mut order, timestamp, true, price_tick)?;
        }
        Ok(())
    }

    fn fill_bid_orders_by_crossing(
        &mut self,
        prev_best_tick: i64,
        new_best_tick: i64,
        timestamp: i64,
    ) -> Result<(), BacktestError> {
        let filled = self
            .queue_model
            .on_best_ask_update(prev_best_tick, new_best_tick)?;
        for mut order in filled {
            let price_tick = order.price_tick;
            self.fill::<true>(&mut order, timestamp, true, price_tick)?;
        }
        Ok(())
    }

    fn ack_new(&mut self, order: &mut Order, timestamp: i64) -> Result<(), BacktestError> {
        if self.queue_model.contains_backtest_order(order.order_id) {
            return Err(BacktestError::OrderIdExist);
        }

        if order.side == Side::Buy {
            match order.order_type {
                OrdType::Limit => {
                    // Checks if the buy order price is greater than or equal to the current best ask.
                    if order.price_tick >= self.depth.best_ask_tick() {
                        match order.time_in_force {
                            TimeInForce::GTX => {
                                order.status = Status::Expired;
                                order.exch_timestamp = timestamp;
                                Ok(())
                            }
                            TimeInForce::GTC | TimeInForce::FOK | TimeInForce::IOC => {
                                // Since this always fills the full quantity, both FOK and IOC
                                // orders are also fully filled at the best price.
                                // Takes the market.
                                self.fill::<false>(
                                    order,
                                    timestamp,
                                    false,
                                    self.depth.best_ask_tick(),
                                )
                            }
                            TimeInForce::Unsupported => Err(BacktestError::InvalidOrderRequest),
                        }
                    } else {
                        match order.time_in_force {
                            TimeInForce::GTC | TimeInForce::GTX => {
                                // Initializes the order's queue position.
                                order.status = Status::New;
                                order.exch_timestamp = timestamp;

                                self.queue_model
                                    .add_backtest_order(order.clone(), &self.depth)?;
                                Ok(())
                            }
                            TimeInForce::FOK | TimeInForce::IOC => {
                                order.status = Status::Expired;
                                order.exch_timestamp = timestamp;
                                Ok(())
                            }
                            TimeInForce::Unsupported => Err(BacktestError::InvalidOrderRequest),
                        }
                    }
                }
                OrdType::Market => {
                    // Takes the market.
                    self.fill::<false>(order, timestamp, false, self.depth.best_ask_tick())
                }
                OrdType::Unsupported => Err(BacktestError::InvalidOrderRequest),
            }
        } else {
            match order.order_type {
                OrdType::Limit => {
                    // Checks if the sell order price is less than or equal to the current best bid.
                    if order.price_tick <= self.depth.best_bid_tick() {
                        match order.time_in_force {
                            TimeInForce::GTX => {
                                order.status = Status::Expired;
                                order.exch_timestamp = timestamp;
                                Ok(())
                            }
                            TimeInForce::GTC | TimeInForce::FOK | TimeInForce::IOC => {
                                // Since this always fills the full quantity, both FOK and IOC
                                // orders are also fully filled at the best price.
                                // Takes the market.
                                self.fill::<false>(
                                    order,
                                    timestamp,
                                    false,
                                    self.depth.best_bid_tick(),
                                )
                            }
                            TimeInForce::Unsupported => Err(BacktestError::InvalidOrderRequest),
                        }
                    } else {
                        match order.time_in_force {
                            TimeInForce::GTC | TimeInForce::GTX => {
                                // Initializes the order's queue position.
                                order.status = Status::New;
                                order.exch_timestamp = timestamp;

                                self.queue_model
                                    .add_backtest_order(order.clone(), &self.depth)?;
                                Ok(())
                            }
                            TimeInForce::FOK | TimeInForce::IOC => {
                                order.status = Status::Expired;
                                order.exch_timestamp = timestamp;
                                Ok(())
                            }
                            TimeInForce::Unsupported => Err(BacktestError::InvalidOrderRequest),
                        }
                    }
                }
                OrdType::Market => {
                    // Takes the market.
                    self.fill::<false>(order, timestamp, false, self.depth.best_bid_tick())
                }
                OrdType::Unsupported => Err(BacktestError::InvalidOrderRequest),
            }
        }
    }

    fn ack_cancel(&mut self, order: &mut Order, timestamp: i64) -> Result<(), BacktestError> {
        match self
            .queue_model
            .cancel_backtest_order(order.order_id, &self.depth)
        {
            Ok(exch_order) => {
                let _ = std::mem::replace(order, exch_order);

                order.status = Status::Canceled;
                order.exch_timestamp = timestamp;
                Ok(())
            }
            Err(BacktestError::OrderNotFound) => {
                order.req = Status::Rejected;
                order.exch_timestamp = timestamp;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    fn ack_modify<const RESET_QUEUE_POS: bool>(
        &mut self,
        order: &mut Order,
        timestamp: i64,
    ) -> Result<(), BacktestError> {
        match self
            .queue_model
            .modify_backtest_order(order.order_id, order, &self.depth)
        {
            Ok(()) => {
                order.leaves_qty = order.qty;
                order.exch_timestamp = timestamp;
                Ok(())
            }
            Err(BacktestError::OrderNotFound) => {
                order.req = Status::Rejected;
                order.exch_timestamp = timestamp;
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}

impl<AT, LM, QM, MD, FM> Processor for L3NoPartialFillExchange<AT, LM, QM, MD, FM>
where
    AT: AssetType,
    LM: LatencyModel,
    QM: L3QueueModel<MD>,
    MD: L3MarketDepth,
    FM: FeeModel,
    BacktestError: From<<MD as L3MarketDepth>::Error>,
{
    fn event_seen_timestamp(&self, event: &Event) -> Option<i64> {
        event.is(EXCH_EVENT).then_some(event.exch_ts)
    }

    fn process(&mut self, event: &Event) -> Result<(), BacktestError> {
        if event.is(EXCH_BID_DEPTH_CLEAR_EVENT) {
            self.depth.clear_orders(Side::Buy);
            let expired = self.queue_model.clear_orders(Side::Buy);
            for order in expired {
                self.expired(order, event.exch_ts)?;
            }
        } else if event.is(EXCH_ASK_DEPTH_CLEAR_EVENT) {
            self.depth.clear_orders(Side::Sell);
            let expired = self.queue_model.clear_orders(Side::Sell);
            for order in expired {
                self.expired(order, event.exch_ts)?;
            }
        } else if event.is(EXCH_DEPTH_CLEAR_EVENT) {
            self.depth.clear_orders(Side::None);
            let expired = self.queue_model.clear_orders(Side::None);
            for order in expired {
                self.expired(order, event.exch_ts)?;
            }
        } else if event.is(EXCH_BID_ADD_ORDER_EVENT) {
            let (prev_best_bid_tick, best_bid_tick) =
                self.depth
                    .add_buy_order(event.order_id, event.px, event.qty, event.exch_ts)?;
            self.queue_model.add_market_feed_order(event, &self.depth)?;
            if best_bid_tick > prev_best_bid_tick {
                self.fill_ask_orders_by_crossing(prev_best_bid_tick, best_bid_tick, event.exch_ts)?;
            }
        } else if event.is(EXCH_ASK_ADD_ORDER_EVENT) {
            let (prev_best_ask_tick, best_ask_tick) =
                self.depth
                    .add_sell_order(event.order_id, event.px, event.qty, event.exch_ts)?;
            self.queue_model.add_market_feed_order(event, &self.depth)?;
            if best_ask_tick < prev_best_ask_tick {
                self.fill_bid_orders_by_crossing(prev_best_ask_tick, best_ask_tick, event.exch_ts)?;
            }
        } else if event.is(EXCH_MODIFY_ORDER_EVENT) {
            let (side, prev_best_tick, best_tick) =
                self.depth
                    .modify_order(event.order_id, event.px, event.qty, event.exch_ts)?;
            self.queue_model
                .modify_market_feed_order(event.order_id, event, &self.depth)?;
            if side == Side::Buy {
                if best_tick > prev_best_tick {
                    self.fill_ask_orders_by_crossing(prev_best_tick, best_tick, event.exch_ts)?;
                }
            } else if best_tick < prev_best_tick {
                self.fill_bid_orders_by_crossing(prev_best_tick, best_tick, event.exch_ts)?;
            }
        } else if event.is(EXCH_CANCEL_ORDER_EVENT) {
            let order_id = event.order_id;
            self.depth.delete_order(order_id, event.exch_ts)?;
            self.queue_model
                .cancel_market_feed_order(event.order_id, &self.depth)?;
        } else if event.is(EXCH_FILL_EVENT) {
            // todo: handle properly if no side is provided.
            if event.is(BUY_EVENT) || event.is(SELL_EVENT) {
                let filled = self.queue_model.fill_market_feed_order::<false>(
                    event.order_id,
                    event,
                    &self.depth,
                )?;
                let timestamp = event.exch_ts;
                for mut order in filled {
                    let price_tick = order.price_tick;
                    self.fill::<true>(&mut order, timestamp, true, price_tick)?;
                }
            }
        }

        Ok(())
    }

    fn process_recv_order(
        &mut self,
        timestamp: i64,
        _wait_resp_order_id: Option<OrderId>,
    ) -> Result<bool, BacktestError> {
        while let Some(mut order) = self.order_e2l.receive(timestamp) {
            // Processes a new order.
            if order.req == Status::New {
                order.req = Status::None;
                self.ack_new(&mut order, timestamp)?;
            }
            // Processes a cancel order.
            else if order.req == Status::Canceled {
                order.req = Status::None;
                self.ack_cancel(&mut order, timestamp)?;
            }
            // Processes a modify order.
            else if order.req == Status::Replaced {
                order.req = Status::None;
                self.ack_modify::<false>(&mut order, timestamp)?;
            } else {
                return Err(BacktestError::InvalidOrderRequest);
            }
            // Makes the response.
            self.order_e2l.respond(order);
        }
        Ok(false)
    }

    fn earliest_recv_order_timestamp(&self) -> i64 {
        self.order_e2l
            .earliest_recv_order_timestamp()
            .unwrap_or(i64::MAX)
    }

    fn earliest_send_order_timestamp(&self) -> i64 {
        self.order_e2l
            .earliest_send_order_timestamp()
            .unwrap_or(i64::MAX)
    }
}
