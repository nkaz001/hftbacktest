use std::{
    cell::RefCell,
    cmp::Ordering,
    collections::{HashMap, HashSet},
    rc::Rc,
};

use crate::{
    backtest::{
        BacktestError,
        assettype::AssetType,
        models::{FeeModel, LatencyModel, QueueModel},
        order::ExchToLocal,
        proc::Processor,
        state::State,
    },
    depth::{INVALID_MAX, INVALID_MIN, L2MarketDepth, MarketDepth},
    prelude::OrdType,
    types::{
        EXCH_ASK_DEPTH_CLEAR_EVENT,
        EXCH_ASK_DEPTH_EVENT,
        EXCH_ASK_DEPTH_SNAPSHOT_EVENT,
        EXCH_BID_DEPTH_CLEAR_EVENT,
        EXCH_BID_DEPTH_EVENT,
        EXCH_BID_DEPTH_SNAPSHOT_EVENT,
        EXCH_BUY_TRADE_EVENT,
        EXCH_DEPTH_CLEAR_EVENT,
        EXCH_EVENT,
        EXCH_SELL_TRADE_EVENT,
        Event,
        Order,
        OrderId,
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
pub struct NoPartialFillExchange<AT, LM, QM, MD, FM>
where
    AT: AssetType,
    LM: LatencyModel,
    QM: QueueModel<MD>,
    MD: MarketDepth,
    FM: FeeModel,
{
    // key: order_id, value: Order<Q>
    orders: Rc<RefCell<HashMap<OrderId, Order>>>,
    // key: order's price tick, value: order_ids
    buy_orders: HashMap<i64, HashSet<OrderId>>,
    sell_orders: HashMap<i64, HashSet<OrderId>>,

    order_e2l: ExchToLocal<LM>,

    depth: MD,
    state: State<AT, FM>,
    queue_model: QM,

    filled_orders: Vec<OrderId>,
}

impl<AT, LM, QM, MD, FM> NoPartialFillExchange<AT, LM, QM, MD, FM>
where
    AT: AssetType,
    LM: LatencyModel,
    QM: QueueModel<MD>,
    MD: MarketDepth,
    FM: FeeModel,
{
    /// Constructs an instance of `NoPartialFillExchange`.
    pub fn new(
        depth: MD,
        state: State<AT, FM>,
        queue_model: QM,
        order_e2l: ExchToLocal<LM>,
    ) -> Self {
        Self {
            orders: Default::default(),
            buy_orders: Default::default(),
            sell_orders: Default::default(),
            order_e2l,
            depth,
            state,
            queue_model,
            filled_orders: Default::default(),
        }
    }

    fn check_if_sell_filled(
        &mut self,
        order: &mut Order,
        price_tick: i64,
        qty: f64,
        timestamp: i64,
    ) -> Result<(), BacktestError> {
        match order.price_tick.cmp(&price_tick) {
            Ordering::Greater => {}
            Ordering::Less => {
                self.filled_orders.push(order.order_id);
                return self.fill::<true>(order, timestamp, true, order.price_tick);
            }
            Ordering::Equal => {
                // Updates the order's queue position.
                self.queue_model.trade(order, qty, &self.depth);
                if self.queue_model.is_filled(order, &self.depth) > 0.0 {
                    self.filled_orders.push(order.order_id);
                    return self.fill::<true>(order, timestamp, true, order.price_tick);
                }
            }
        }
        Ok(())
    }

    fn check_if_buy_filled(
        &mut self,
        order: &mut Order,
        price_tick: i64,
        qty: f64,
        timestamp: i64,
    ) -> Result<(), BacktestError> {
        match order.price_tick.cmp(&price_tick) {
            Ordering::Greater => {
                self.filled_orders.push(order.order_id);
                return self.fill::<true>(order, timestamp, true, order.price_tick);
            }
            Ordering::Less => {}
            Ordering::Equal => {
                // Updates the order's queue position.
                self.queue_model.trade(order, qty, &self.depth);
                if self.queue_model.is_filled(order, &self.depth) > 0.0 {
                    self.filled_orders.push(order.order_id);
                    return self.fill::<true>(order, timestamp, true, order.price_tick);
                }
            }
        }
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

    fn remove_filled_orders(&mut self) {
        if !self.filled_orders.is_empty() {
            let mut orders = self.orders.borrow_mut();
            for order_id in self.filled_orders.drain(..) {
                let order = orders.remove(&order_id).unwrap();
                if order.side == Side::Buy {
                    self.buy_orders
                        .get_mut(&order.price_tick)
                        .unwrap()
                        .remove(&order_id);
                } else {
                    self.sell_orders
                        .get_mut(&order.price_tick)
                        .unwrap()
                        .remove(&order_id);
                }
            }
        }
    }

    fn on_bid_qty_chg(&mut self, price_tick: i64, prev_qty: f64, new_qty: f64) {
        let orders = self.orders.clone();
        if let Some(order_ids) = self.buy_orders.get(&price_tick) {
            for order_id in order_ids.iter() {
                let mut orders_borrowed = orders.borrow_mut();
                let order = orders_borrowed.get_mut(order_id).unwrap();
                self.queue_model
                    .depth(order, prev_qty, new_qty, &self.depth);
            }
        }
    }

    fn on_ask_qty_chg(&mut self, price_tick: i64, prev_qty: f64, new_qty: f64) {
        let orders = self.orders.clone();
        if let Some(order_ids) = self.sell_orders.get(&price_tick) {
            for order_id in order_ids.iter() {
                let mut orders_borrowed = orders.borrow_mut();
                let order = orders_borrowed.get_mut(order_id).unwrap();
                self.queue_model
                    .depth(order, prev_qty, new_qty, &self.depth);
            }
        }
    }

    fn on_best_bid_update(
        &mut self,
        prev_best_tick: i64,
        new_best_tick: i64,
        timestamp: i64,
    ) -> Result<(), BacktestError> {
        // If the best has been significantly updated compared to the previous best, it would be
        // better to iterate orders dict instead of order price ladder.
        {
            let orders = self.orders.clone();
            let mut orders_borrowed = orders.borrow_mut();
            if prev_best_tick == INVALID_MIN
                || (orders_borrowed.len() as i64) < new_best_tick - prev_best_tick
            {
                for (_, order) in orders_borrowed.iter_mut() {
                    if order.side == Side::Sell && order.price_tick <= new_best_tick {
                        self.filled_orders.push(order.order_id);
                        self.fill::<true>(order, timestamp, true, order.price_tick)?;
                    }
                }
            } else {
                for t in (prev_best_tick + 1)..=new_best_tick {
                    if let Some(order_ids) = self.sell_orders.get(&t) {
                        for order_id in order_ids.clone().iter() {
                            self.filled_orders.push(*order_id);
                            let order = orders_borrowed.get_mut(order_id).unwrap();
                            self.fill::<true>(order, timestamp, true, order.price_tick)?;
                        }
                    }
                }
            }
        }
        self.remove_filled_orders();
        Ok(())
    }

    fn on_best_ask_update(
        &mut self,
        prev_best_tick: i64,
        new_best_tick: i64,
        timestamp: i64,
    ) -> Result<(), BacktestError> {
        // If the best has been significantly updated compared to the previous best, it would be
        // better to iterate orders dict instead of order price ladder.
        {
            let orders = self.orders.clone();
            let mut orders_borrowed = orders.borrow_mut();
            if prev_best_tick == INVALID_MAX
                || (orders_borrowed.len() as i64) < prev_best_tick - new_best_tick
            {
                for (_, order) in orders_borrowed.iter_mut() {
                    if order.side == Side::Buy && order.price_tick >= new_best_tick {
                        self.filled_orders.push(order.order_id);
                        self.fill::<true>(order, timestamp, true, order.price_tick)?;
                    }
                }
            } else {
                for t in new_best_tick..prev_best_tick {
                    if let Some(order_ids) = self.buy_orders.get(&t) {
                        for order_id in order_ids.clone().iter() {
                            self.filled_orders.push(*order_id);
                            let order = orders_borrowed.get_mut(order_id).unwrap();
                            self.fill::<true>(order, timestamp, true, order.price_tick)?;
                        }
                    }
                }
            }
        }
        self.remove_filled_orders();
        Ok(())
    }

    fn ack_new(&mut self, order: &mut Order, timestamp: i64) -> Result<(), BacktestError> {
        if self.orders.borrow().contains_key(&order.order_id) {
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
                                self.queue_model.new_order(order, &self.depth);
                                order.status = Status::New;
                                // The exchange accepts this order.
                                self.buy_orders
                                    .entry(order.price_tick)
                                    .or_default()
                                    .insert(order.order_id);

                                order.exch_timestamp = timestamp;
                                self.orders
                                    .borrow_mut()
                                    .insert(order.order_id, order.clone());
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
                                self.queue_model.new_order(order, &self.depth);
                                order.status = Status::New;
                                // The exchange accepts this order.
                                self.sell_orders
                                    .entry(order.price_tick)
                                    .or_default()
                                    .insert(order.order_id);

                                order.exch_timestamp = timestamp;
                                self.orders
                                    .borrow_mut()
                                    .insert(order.order_id, order.clone());
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
        let exch_order = {
            let mut order_borrowed = self.orders.borrow_mut();
            order_borrowed.remove(&order.order_id)
        };

        if exch_order.is_none() {
            order.req = Status::Rejected;
            order.exch_timestamp = timestamp;
            return Ok(());
        }

        let exch_order = exch_order.unwrap();
        let _ = std::mem::replace(order, exch_order);

        // Deletes the order.
        if order.side == Side::Buy {
            self.buy_orders
                .get_mut(&order.price_tick)
                .unwrap()
                .remove(&order.order_id);
        } else {
            self.sell_orders
                .get_mut(&order.price_tick)
                .unwrap()
                .remove(&order.order_id);
        }
        order.status = Status::Canceled;
        order.exch_timestamp = timestamp;
        Ok(())
    }

    fn ack_modify<const RESET_QUEUE_POS: bool>(
        &mut self,
        order: &mut Order,
        timestamp: i64,
    ) -> Result<(), BacktestError> {
        let (prev_order_price_tick, prev_leaves_qty) = {
            let order_borrowed = self.orders.borrow();
            let exch_order = order_borrowed.get(&order.order_id);

            // The order can be already deleted due to fill or expiration.
            if exch_order.is_none() {
                order.req = Status::Rejected;
                order.exch_timestamp = timestamp;
                return Ok(());
            }

            let exch_order = exch_order.unwrap();
            (exch_order.price_tick, exch_order.leaves_qty)
        };

        // The initialization of the order queue position may not occur when the modified quantity
        // is smaller than the previous quantity, depending on the exchanges. It may need to
        // implement exchange-specific specialization.
        if RESET_QUEUE_POS
            || prev_order_price_tick != order.price_tick
            || order.qty > prev_leaves_qty
        {
            self.ack_cancel(order, timestamp)?;
            self.ack_new(order, timestamp)?;
        } else {
            let mut order_borrowed = self.orders.borrow_mut();
            let exch_order = order_borrowed.get_mut(&order.order_id);
            let exch_order = exch_order.unwrap();

            exch_order.qty = order.qty;
            exch_order.leaves_qty = order.qty;
            exch_order.exch_timestamp = timestamp;
            order.leaves_qty = order.qty;
            order.exch_timestamp = timestamp;
        }
        Ok(())
    }
}

impl<AT, LM, QM, MD, FM> Processor for NoPartialFillExchange<AT, LM, QM, MD, FM>
where
    AT: AssetType,
    LM: LatencyModel,
    QM: QueueModel<MD>,
    MD: MarketDepth + L2MarketDepth,
    FM: FeeModel,
{
    fn event_seen_timestamp(&self, event: &Event) -> Option<i64> {
        event.is(EXCH_EVENT).then_some(event.exch_ts)
    }

    fn process(&mut self, event: &Event) -> Result<(), BacktestError> {
        if event.is(EXCH_BID_DEPTH_CLEAR_EVENT) {
            self.depth.clear_depth(Side::Buy, event.px);
        } else if event.is(EXCH_ASK_DEPTH_CLEAR_EVENT) {
            self.depth.clear_depth(Side::Sell, event.px);
        } else if event.is(EXCH_DEPTH_CLEAR_EVENT) {
            self.depth.clear_depth(Side::None, 0.0);
        } else if event.is(EXCH_BID_DEPTH_EVENT) || event.is(EXCH_BID_DEPTH_SNAPSHOT_EVENT) {
            let (price_tick, prev_best_bid_tick, best_bid_tick, prev_qty, new_qty, timestamp) =
                self.depth
                    .update_bid_depth(event.px, event.qty, event.exch_ts);
            self.on_bid_qty_chg(price_tick, prev_qty, new_qty);
            if best_bid_tick > prev_best_bid_tick {
                self.on_best_bid_update(prev_best_bid_tick, best_bid_tick, timestamp)?;
            }
        } else if event.is(EXCH_ASK_DEPTH_EVENT) || event.is(EXCH_ASK_DEPTH_SNAPSHOT_EVENT) {
            let (price_tick, prev_best_ask_tick, best_ask_tick, prev_qty, new_qty, timestamp) =
                self.depth
                    .update_ask_depth(event.px, event.qty, event.exch_ts);
            self.on_ask_qty_chg(price_tick, prev_qty, new_qty);
            if best_ask_tick < prev_best_ask_tick {
                self.on_best_ask_update(prev_best_ask_tick, best_ask_tick, timestamp)?;
            }
        } else if event.is(EXCH_BUY_TRADE_EVENT) {
            let price_tick = (event.px / self.depth.tick_size()).round() as i64;
            let qty = event.qty;
            {
                let orders = self.orders.clone();
                let mut orders_borrowed = orders.borrow_mut();
                if self.depth.best_bid_tick() == INVALID_MIN
                    || (orders_borrowed.len() as i64) < price_tick - self.depth.best_bid_tick()
                {
                    for (_, order) in orders_borrowed.iter_mut() {
                        if order.side == Side::Sell {
                            self.check_if_sell_filled(order, price_tick, qty, event.exch_ts)?;
                        }
                    }
                } else {
                    for t in (self.depth.best_bid_tick() + 1)..=price_tick {
                        if let Some(order_ids) = self.sell_orders.get(&t) {
                            for order_id in order_ids.clone().iter() {
                                let order = orders_borrowed.get_mut(order_id).unwrap();
                                self.check_if_sell_filled(order, price_tick, qty, event.exch_ts)?;
                            }
                        }
                    }
                }
            }
            self.remove_filled_orders();
        } else if event.is(EXCH_SELL_TRADE_EVENT) {
            let price_tick = (event.px / self.depth.tick_size()).round() as i64;
            let qty = event.qty;
            {
                let orders = self.orders.clone();
                let mut orders_borrowed = orders.borrow_mut();
                if self.depth.best_ask_tick() == INVALID_MAX
                    || (orders_borrowed.len() as i64) < self.depth.best_ask_tick() - price_tick
                {
                    for (_, order) in orders_borrowed.iter_mut() {
                        if order.side == Side::Buy {
                            self.check_if_buy_filled(order, price_tick, qty, event.exch_ts)?;
                        }
                    }
                } else {
                    for t in (price_tick..self.depth.best_ask_tick()).rev() {
                        if let Some(order_ids) = self.buy_orders.get(&t) {
                            for order_id in order_ids.clone().iter() {
                                let order = orders_borrowed.get_mut(order_id).unwrap();
                                self.check_if_buy_filled(order, price_tick, qty, event.exch_ts)?;
                            }
                        }
                    }
                }
            }
            self.remove_filled_orders();
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
