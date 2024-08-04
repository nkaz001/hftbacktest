use std::{
    cell::RefCell,
    cmp::Ordering,
    collections::{HashMap, HashSet},
    mem,
    rc::Rc,
};

use crate::{
    backtest::{
        assettype::AssetType,
        data::{Data, Reader},
        models::{FeeModel, LatencyModel, QueueModel},
        order::OrderBus,
        proc::Processor,
        state::State,
        BacktestError,
    },
    depth::{L2MarketDepth, MarketDepth, INVALID_MAX, INVALID_MIN},
    types::{
        Event,
        Order,
        OrderId,
        Side,
        Status,
        TimeInForce,
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
    },
};

/// The exchange model with partial fills.
///
/// * Support order types: [OrdType::Limit](crate::types::OrdType::Limit)
/// * Support time-in-force: [`TimeInForce::GTC`], [`TimeInForce::FOK`], [`TimeInForce::IOC`],
///                          [`TimeInForce::GTX`]
///
/// **Conditions for Full Execution**
/// Buy order in the order book
///
/// - Your order price >= the best ask price
/// - Your order price > sell trade price
///
/// Sell order in the order book
///
/// - Your order price <= the best bid price
/// - Your order price < buy trade price
///
/// **Conditions for Partial Execution**
/// Buy order in the order book
///
/// - Filled by (remaining) sell trade quantity: your order is at the front of the queue && your
///   order price == sell trade price
///
/// Sell order in the order book
///
/// - Filled by (remaining) buy trade quantity: your order is at the front of the queue && your
///   order price == buy trade price
///
/// **Liquidity-Taking Order**
/// Liquidity-taking orders will be executed based on the quantity of the order book, even though
/// the best price and quantity do not change due to your execution. Be aware that this may cause
/// unrealistic fill simulations if you attempt to execute a large quantity.
///
/// **General Comment**
/// Simulating partial fills accurately can be challenging, as they may indicate potential market
/// impact. The rule of thumb is to ensure that your backtesting results align with your live
/// results.
/// (more comment will be added...)
///
pub struct PartialFillExchange<AT, LM, QM, MD, FM>
where
    AT: AssetType,
    LM: LatencyModel,
    QM: QueueModel<MD>,
    MD: MarketDepth,
    FM: FeeModel,
{
    reader: Reader<Event>,
    data: Data<Event>,
    row_num: usize,

    // key: order_id, value: Order
    orders: Rc<RefCell<HashMap<OrderId, Order>>>,
    // key: order's price tick, value: order_ids
    buy_orders: HashMap<i64, HashSet<OrderId>>,
    sell_orders: HashMap<i64, HashSet<OrderId>>,

    orders_to: OrderBus,
    orders_from: OrderBus,

    depth: MD,
    state: State<AT, FM>,
    order_latency: LM,
    queue_model: QM,

    filled_orders: Vec<OrderId>,
}

impl<AT, LM, QM, MD, FM> PartialFillExchange<AT, LM, QM, MD, FM>
where
    AT: AssetType,
    LM: LatencyModel,
    QM: QueueModel<MD>,
    MD: MarketDepth,
    FM: FeeModel,
{
    /// Constructs an instance of `PartialFillExchange`.
    pub fn new(
        reader: Reader<Event>,
        depth: MD,
        state: State<AT, FM>,
        order_latency: LM,
        queue_model: QM,
        orders_to: OrderBus,
        orders_from: OrderBus,
    ) -> Self {
        Self {
            reader,
            data: Data::empty(),
            row_num: 0,
            orders: Default::default(),
            buy_orders: Default::default(),
            sell_orders: Default::default(),
            orders_to,
            orders_from,
            depth,
            state,
            order_latency,
            queue_model,
            filled_orders: Default::default(),
        }
    }

    fn process_recv_order_(
        &mut self,
        mut order: Order,
        recv_timestamp: i64,
    ) -> Result<(), BacktestError> {
        // Processes a new order.
        if order.req == Status::New {
            order.req = Status::None;
            self.ack_new(order, recv_timestamp)?;
        }
        // Processes a cancel order.
        else if order.req == Status::Canceled {
            order.req = Status::None;
            self.ack_cancel(order, recv_timestamp)?;
        } else {
            return Err(BacktestError::InvalidOrderRequest);
        }
        Ok(())
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
                return self.fill(order, timestamp, true, order.price_tick, order.leaves_qty);
            }
            Ordering::Equal => {
                // Updates the order's queue position.
                self.queue_model.trade(order, qty, &self.depth);
                let filled_qty = self.queue_model.is_filled(order, &self.depth);
                if filled_qty > 0.0 {
                    // q_ahead is negative since is_filled is true and its value represents the
                    // executable quantity of this order after execution in the queue ahead of this
                    // order.
                    // let q_qty =
                    //     (-order.front_q_qty / self.depth.lot_size()).floor() * self.depth.lot_size();
                    let exec_qty = filled_qty.min(qty).min(order.leaves_qty);
                    self.filled_orders.push(order.order_id);
                    return self.fill(order, timestamp, true, order.price_tick, exec_qty);
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
                return self.fill(order, timestamp, true, order.price_tick, order.leaves_qty);
            }
            Ordering::Less => {}
            Ordering::Equal => {
                // Updates the order's queue position.
                self.queue_model.trade(order, qty, &self.depth);
                let filled_qty = self.queue_model.is_filled(order, &self.depth);
                if filled_qty > 0.0 {
                    // q_ahead is negative since is_filled is true and its value represents the
                    // executable quantity of this order after execution in the queue ahead of this
                    // order.
                    // let q_qty =
                    //     (-order.front_q_qty / self.depth.lot_size()).floor() * self.depth.lot_size();
                    let exec_qty = filled_qty.min(qty).min(order.leaves_qty);
                    self.filled_orders.push(order.order_id);
                    return self.fill(order, timestamp, true, order.price_tick, exec_qty);
                }
            }
        }
        Ok(())
    }

    fn fill(
        &mut self,
        order: &mut Order,
        timestamp: i64,
        maker: bool,
        exec_price_tick: i64,
        exec_qty: f64,
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

        order.exec_qty = exec_qty;
        order.leaves_qty -= exec_qty;
        if (order.leaves_qty / self.depth.lot_size()).round() > 0f64 {
            order.status = Status::PartiallyFilled;
        } else {
            order.status = Status::Filled;
        }
        order.exch_timestamp = timestamp;
        let local_recv_timestamp =
            order.exch_timestamp + self.order_latency.response(timestamp, order);

        self.state.apply_fill(order);
        self.orders_to.append(order.clone(), local_recv_timestamp);
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
                        self.fill(order, timestamp, true, order.price_tick, order.leaves_qty)?;
                    }
                }
            } else {
                for t in (prev_best_tick + 1)..=new_best_tick {
                    if let Some(order_ids) = self.sell_orders.get(&t) {
                        for order_id in order_ids.clone().iter() {
                            self.filled_orders.push(*order_id);
                            let order = orders_borrowed.get_mut(order_id).unwrap();
                            self.fill(order, timestamp, true, order.price_tick, order.leaves_qty)?;
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
                        self.fill(order, timestamp, true, order.price_tick, order.leaves_qty)?;
                    }
                }
            } else {
                for t in new_best_tick..prev_best_tick {
                    if let Some(order_ids) = self.buy_orders.get(&t) {
                        for order_id in order_ids.clone().iter() {
                            self.filled_orders.push(*order_id);
                            let order = orders_borrowed.get_mut(order_id).unwrap();
                            self.fill(order, timestamp, true, order.price_tick, order.leaves_qty)?;
                        }
                    }
                }
            }
        }
        self.remove_filled_orders();
        Ok(())
    }

    fn ack_new(&mut self, mut order: Order, timestamp: i64) -> Result<(), BacktestError> {
        if self.orders.borrow().contains_key(&order.order_id) {
            return Err(BacktestError::OrderIdExist);
        }

        if order.side == Side::Buy {
            // Checks if the buy order price is greater than or equal to the current best ask.
            if order.price_tick >= self.depth.best_ask_tick() {
                match order.time_in_force {
                    TimeInForce::GTX => {
                        order.status = Status::Expired;

                        order.exch_timestamp = timestamp;
                        let local_recv_timestamp =
                            timestamp + self.order_latency.response(timestamp, &order);
                        self.orders_to.append(order.clone(), local_recv_timestamp);
                        Ok(())
                    }
                    TimeInForce::FOK => {
                        // The order must be executed immediately in its entirety; otherwise, the
                        // entire order will be cancelled.
                        let mut execute = false;
                        let mut cum_qty = 0f64;
                        for t in self.depth.best_ask_tick()..=order.price_tick {
                            cum_qty += self.depth.ask_qty_at_tick(t);
                            if (cum_qty / self.depth.lot_size()).round()
                                >= (order.qty / self.depth.lot_size()).round()
                            {
                                execute = true;
                                break;
                            }
                        }
                        if execute {
                            for t in self.depth.best_ask_tick()..=order.price_tick {
                                let qty = self.depth.ask_qty_at_tick(t);
                                if qty > 0.0 {
                                    let exec_qty = qty.min(order.leaves_qty);
                                    self.fill(&mut order, timestamp, false, t, exec_qty)?;
                                    if order.status == Status::Filled {
                                        return Ok(());
                                    }
                                }
                            }
                            unreachable!();
                        } else {
                            order.status = Status::Expired;

                            order.exch_timestamp = timestamp;
                            let local_recv_timestamp =
                                timestamp + self.order_latency.response(timestamp, &order);
                            self.orders_to.append(order.clone(), local_recv_timestamp);
                            Ok(())
                        }
                    }
                    TimeInForce::IOC => {
                        // The order must be executed immediately.
                        for t in self.depth.best_ask_tick()..=order.price_tick {
                            let qty = self.depth.ask_qty_at_tick(t);
                            if qty > 0.0 {
                                let exec_qty = qty.min(order.leaves_qty);
                                self.fill(&mut order, timestamp, false, t, exec_qty)?;
                            }
                            if order.status == Status::Filled {
                                return Ok(());
                            }
                        }
                        order.status = Status::Expired;

                        order.exch_timestamp = timestamp;
                        let local_recv_timestamp =
                            timestamp + self.order_latency.response(timestamp, &order);
                        self.orders_to.append(order.clone(), local_recv_timestamp);
                        Ok(())
                    }
                    TimeInForce::GTC => {
                        // Takes the market.
                        for t in self.depth.best_ask_tick()..order.price_tick {
                            let qty = self.depth.ask_qty_at_tick(t);
                            if qty > 0.0 {
                                let exec_qty = qty.min(order.leaves_qty);
                                self.fill(&mut order, timestamp, false, t, exec_qty)?;
                            }
                            if order.status == Status::Filled {
                                return Ok(());
                            }
                        }

                        // The buy order cannot remain in the ask book, as it cannot affect the
                        // market depth during backtesting based on market-data replay. So, even
                        // though it simulates partial fill, if the order size is not small enough,
                        // it introduces unreality.
                        let (price_tick, leaves_qty) = (order.price_tick, order.leaves_qty);
                        self.fill(&mut order, timestamp, false, price_tick, leaves_qty)
                    }
                    _ => {
                        unreachable!();
                    }
                }
            } else {
                // Initializes the order's queue position.
                self.queue_model.new_order(&mut order, &self.depth);
                order.status = Status::New;
                // The exchange accepts this order.
                self.buy_orders
                    .entry(order.price_tick)
                    .or_default()
                    .insert(order.order_id);

                order.exch_timestamp = timestamp;
                let local_recv_timestamp =
                    timestamp + self.order_latency.response(timestamp, &order);
                self.orders_to.append(order.clone(), local_recv_timestamp);

                self.orders.borrow_mut().insert(order.order_id, order);

                Ok(())
            }
        } else {
            // Checks if the sell order price is less than or equal to the current best bid.
            if order.price_tick <= self.depth.best_bid_tick() {
                match order.time_in_force {
                    TimeInForce::GTX => {
                        order.status = Status::Expired;

                        order.exch_timestamp = timestamp;
                        let local_recv_timestamp =
                            timestamp + self.order_latency.response(timestamp, &order);
                        self.orders_to.append(order.clone(), local_recv_timestamp);
                        Ok(())
                    }
                    TimeInForce::FOK => {
                        // The order must be executed immediately in its entirety; otherwise, the
                        // entire order will be cancelled.
                        let mut execute = false;
                        let mut cum_qty = 0f64;
                        for t in (order.price_tick..=self.depth.best_bid_tick()).rev() {
                            cum_qty += self.depth.bid_qty_at_tick(t);
                            if (cum_qty / self.depth.lot_size()).round()
                                >= (order.qty / self.depth.lot_size()).round()
                            {
                                execute = true;
                                break;
                            }
                        }
                        if execute {
                            for t in (order.price_tick..=self.depth.best_bid_tick()).rev() {
                                let qty = self.depth.bid_qty_at_tick(t);
                                if qty > 0.0 {
                                    let exec_qty = qty.min(order.leaves_qty);
                                    self.fill(&mut order, timestamp, false, t, exec_qty)?;
                                    if order.status == Status::Filled {
                                        return Ok(());
                                    }
                                }
                            }
                            unreachable!();
                        } else {
                            order.status = Status::Expired;

                            order.exch_timestamp = timestamp;
                            let local_recv_timestamp =
                                timestamp + self.order_latency.response(timestamp, &order);
                            self.orders_to.append(order.clone(), local_recv_timestamp);
                            Ok(())
                        }
                    }
                    TimeInForce::IOC => {
                        // The order must be executed immediately.
                        for t in (order.price_tick..=self.depth.best_bid_tick()).rev() {
                            let qty = self.depth.bid_qty_at_tick(t);
                            if qty > 0.0 {
                                let exec_qty = qty.min(order.leaves_qty);
                                self.fill(&mut order, timestamp, false, t, exec_qty)?;
                            }
                            if order.status == Status::Filled {
                                return Ok(());
                            }
                        }
                        order.status = Status::Expired;

                        order.exch_timestamp = timestamp;
                        let local_recv_timestamp =
                            timestamp + self.order_latency.response(timestamp, &order);
                        self.orders_to.append(order.clone(), local_recv_timestamp);
                        Ok(())
                    }
                    TimeInForce::GTC => {
                        // Takes the market.
                        for t in (order.price_tick..=self.depth.best_bid_tick()).rev() {
                            let qty = self.depth.bid_qty_at_tick(t);
                            if qty > 0.0 {
                                let exec_qty = qty.min(order.leaves_qty);
                                self.fill(&mut order, timestamp, false, t, exec_qty)?;
                            }
                            if order.status == Status::Filled {
                                return Ok(());
                            }
                        }

                        // The sell order cannot remain in the bid book, as it cannot affect the
                        // market depth during backtesting based on market-data replay. So, even
                        // though it simulates partial fill, if the order size is not small enough,
                        // it introduces unreality.
                        let (price_tick, leaves_qty) = (order.price_tick, order.leaves_qty);
                        self.fill(&mut order, timestamp, false, price_tick, leaves_qty)
                    }
                    _ => {
                        unreachable!();
                    }
                }
            } else {
                // Initializes the order's queue position.
                self.queue_model.new_order(&mut order, &self.depth);
                order.status = Status::New;
                // The exchange accepts this order.
                self.sell_orders
                    .entry(order.price_tick)
                    .or_default()
                    .insert(order.order_id);

                order.exch_timestamp = timestamp;
                let local_recv_timestamp =
                    timestamp + self.order_latency.response(timestamp, &order);
                self.orders_to.append(order.clone(), local_recv_timestamp);

                self.orders.borrow_mut().insert(order.order_id, order);

                Ok(())
            }
        }
    }

    fn ack_cancel(&mut self, mut order: Order, timestamp: i64) -> Result<(), BacktestError> {
        let exch_order = {
            let mut order_borrowed = self.orders.borrow_mut();
            order_borrowed.remove(&order.order_id)
        };

        if exch_order.is_none() {
            order.req = Status::Rejected;
            order.exch_timestamp = timestamp;
            let local_recv_timestamp = timestamp + self.order_latency.response(timestamp, &order);
            self.orders_to.append(order, local_recv_timestamp);
            return Ok(());
        }

        // Deletes the order.
        let mut exch_order = exch_order.unwrap();
        if exch_order.side == Side::Buy {
            self.buy_orders
                .get_mut(&exch_order.price_tick)
                .unwrap()
                .remove(&exch_order.order_id);
        } else {
            self.sell_orders
                .get_mut(&exch_order.price_tick)
                .unwrap()
                .remove(&exch_order.order_id);
        }

        // Makes the response.
        exch_order.status = Status::Canceled;
        exch_order.exch_timestamp = timestamp;
        let local_recv_timestamp = timestamp + self.order_latency.response(timestamp, &exch_order);
        self.orders_to
            .append(exch_order.clone(), local_recv_timestamp);
        Ok(())
    }

    fn ack_modify(&mut self, mut order: Order, timestamp: i64) -> Result<(), BacktestError> {
        todo!();
        // let mut exch_order = {
        //     let mut order_borrowed = self.orders.borrow_mut();
        //     let exch_order = order_borrowed.remove(&order.order_id);
        //
        //     // The order can be already deleted due to fill or expiration.
        //     if exch_order.is_none() {
        //         order.req = Status::Rejected;
        //         order.exch_timestamp = timestamp;
        //         let local_recv_timestamp =
        //             timestamp + self.order_latency.response(timestamp, &order);
        //         self.orders_to.append(order, local_recv_timestamp);
        //         return Ok(local_recv_timestamp);
        //     }
        //
        //     exch_order.unwrap()
        // };
        //
        // let prev_price_tick = exch_order.price_tick;
        // exch_order.price_tick = order.price_tick;
        // // No partial fill occurs.
        // exch_order.qty = order.qty;
        // // The initialization of the order queue position may not occur when the modified quantity
        // // is smaller than the previous quantity, depending on the exchanges. It may need to
        // // implement exchange-specific specialization.
        // let init_q_pos = true;
        //
        // if exch_order.side == Side::Buy {
        //     // Check if the buy order price is greater than or equal to the current best ask.
        //     if exch_order.price_tick >= self.depth.best_ask_tick {
        //         self.buy_orders
        //             .get_mut(&prev_price_tick)
        //             .unwrap()
        //             .remove(&exch_order.order_id);
        //
        //         if exch_order.time_in_force == TimeInForce::GTX {
        //             exch_order.status = Status::Expired;
        //         } else {
        //             // Take the market.
        //             return self.fill(&mut exch_order, timestamp, false, self.depth.best_ask_tick);
        //         }
        //
        //         exch_order.exch_timestamp = timestamp;
        //         let local_recv_timestamp =
        //             timestamp + self.order_latency.response(timestamp, &exch_order);
        //         self.orders_to
        //             .append(exch_order.clone(), local_recv_timestamp);
        //         Ok(local_recv_timestamp)
        //     } else {
        //         // The exchange accepts this order.
        //         if prev_price_tick != exch_order.price_tick {
        //             self.buy_orders
        //                 .get_mut(&prev_price_tick)
        //                 .unwrap()
        //                 .remove(&exch_order.order_id);
        //             self.buy_orders
        //                 .entry(exch_order.price_tick)
        //                 .or_insert(HashSet::new())
        //                 .insert(exch_order.order_id);
        //         }
        //         if init_q_pos || prev_price_tick != exch_order.price_tick {
        //             // Initialize the order's queue position.
        //             self.queue_model.new_order(&mut exch_order, &self.depth);
        //         }
        //         exch_order.status = Status::New;
        //
        //         exch_order.exch_timestamp = timestamp;
        //         let local_recv_timestamp =
        //             timestamp + self.order_latency.response(timestamp, &exch_order);
        //         self.orders_to
        //             .append(exch_order.clone(), local_recv_timestamp);
        //
        //         let mut order_borrowed = self.orders.borrow_mut();
        //         order_borrowed.insert(exch_order.order_id, exch_order);
        //
        //         Ok(local_recv_timestamp)
        //     }
        // } else {
        //     // Check if the sell order price is less than or equal to the current best bid.
        //     if exch_order.price_tick <= self.depth.best_bid_tick {
        //         self.sell_orders
        //             .get_mut(&prev_price_tick)
        //             .unwrap()
        //             .remove(&exch_order.order_id);
        //
        //         if exch_order.time_in_force == TimeInForce::GTX {
        //             exch_order.status = Status::Expired;
        //         } else {
        //             // Take the market.
        //             return self.fill(&mut exch_order, timestamp, false, self.depth.best_bid_tick);
        //         }
        //
        //         exch_order.exch_timestamp = timestamp;
        //         let local_recv_timestamp =
        //             timestamp + self.order_latency.response(timestamp, &exch_order);
        //         self.orders_to
        //             .append(exch_order.clone(), local_recv_timestamp);
        //         Ok(local_recv_timestamp)
        //     } else {
        //         // The exchange accepts this order.
        //         if prev_price_tick != exch_order.price_tick {
        //             self.sell_orders
        //                 .get_mut(&prev_price_tick)
        //                 .unwrap()
        //                 .remove(&exch_order.order_id);
        //             self.sell_orders
        //                 .entry(exch_order.price_tick)
        //                 .or_insert(HashSet::new())
        //                 .insert(exch_order.order_id);
        //         }
        //         if init_q_pos || prev_price_tick != exch_order.price_tick {
        //             // Initialize the order's queue position.
        //             self.queue_model.new_order(&mut exch_order, &self.depth);
        //         }
        //         exch_order.status = Status::New;
        //
        //         exch_order.exch_timestamp = timestamp;
        //         let local_recv_timestamp =
        //             timestamp + self.order_latency.response(timestamp, &exch_order);
        //         self.orders_to
        //             .append(exch_order.clone(), local_recv_timestamp);
        //
        //         let mut order_borrowed = self.orders.borrow_mut();
        //         order_borrowed.insert(exch_order.order_id, exch_order);
        //
        //         Ok(local_recv_timestamp)
        //     }
        // }
    }
}

impl<AT, LM, QM, MD, FM> Processor for PartialFillExchange<AT, LM, QM, MD, FM>
where
    AT: AssetType,
    LM: LatencyModel,
    QM: QueueModel<MD>,
    MD: MarketDepth + L2MarketDepth,
    FM: FeeModel,
{
    fn initialize_data(&mut self) -> Result<i64, BacktestError> {
        self.data = self.reader.next_data()?;
        for rn in 0..self.data.len() {
            if self.data[rn].is(EXCH_EVENT) {
                self.row_num = rn;
                return Ok(self.data[rn].exch_ts);
            }
        }
        Err(BacktestError::EndOfData)
    }

    fn process_data(&mut self) -> Result<(i64, i64), BacktestError> {
        let row_num = self.row_num;
        if self.data[row_num].is(EXCH_BID_DEPTH_CLEAR_EVENT) {
            self.depth.clear_depth(Side::Buy, self.data[row_num].px);
        } else if self.data[row_num].is(EXCH_ASK_DEPTH_CLEAR_EVENT) {
            self.depth.clear_depth(Side::Sell, self.data[row_num].px);
        } else if self.data[row_num].is(EXCH_DEPTH_CLEAR_EVENT) {
            self.depth.clear_depth(Side::None, 0.0);
        } else if self.data[row_num].is(EXCH_BID_DEPTH_EVENT)
            || self.data[row_num].is(EXCH_BID_DEPTH_SNAPSHOT_EVENT)
        {
            let (price_tick, prev_best_bid_tick, best_bid_tick, prev_qty, new_qty, timestamp) =
                self.depth.update_bid_depth(
                    self.data[row_num].px,
                    self.data[row_num].qty,
                    self.data[row_num].exch_ts,
                );
            self.on_bid_qty_chg(price_tick, prev_qty, new_qty);
            if best_bid_tick > prev_best_bid_tick {
                self.on_best_bid_update(prev_best_bid_tick, best_bid_tick, timestamp)?;
            }
        } else if self.data[row_num].is(EXCH_ASK_DEPTH_EVENT)
            || self.data[row_num].is(EXCH_ASK_DEPTH_SNAPSHOT_EVENT)
        {
            let (price_tick, prev_best_ask_tick, best_ask_tick, prev_qty, new_qty, timestamp) =
                self.depth.update_ask_depth(
                    self.data[row_num].px,
                    self.data[row_num].qty,
                    self.data[row_num].exch_ts,
                );
            self.on_ask_qty_chg(price_tick, prev_qty, new_qty);
            if best_ask_tick < prev_best_ask_tick {
                self.on_best_ask_update(prev_best_ask_tick, best_ask_tick, timestamp)?;
            }
        } else if self.data[row_num].is(EXCH_BUY_TRADE_EVENT) {
            let price_tick = (self.data[row_num].px / self.depth.tick_size()).round() as i64;
            let qty = self.data[row_num].qty;
            {
                let orders = self.orders.clone();
                let mut orders_borrowed = orders.borrow_mut();
                if self.depth.best_bid_tick() == INVALID_MIN
                    || (orders_borrowed.len() as i64) < price_tick - self.depth.best_bid_tick()
                {
                    for (_, order) in orders_borrowed.iter_mut() {
                        if order.side == Side::Sell {
                            self.check_if_sell_filled(
                                order,
                                price_tick,
                                qty,
                                self.data[row_num].exch_ts,
                            )?;
                        }
                    }
                } else {
                    for t in (self.depth.best_bid_tick() + 1)..=price_tick {
                        if let Some(order_ids) = self.sell_orders.get(&t) {
                            for order_id in order_ids.clone().iter() {
                                let order = orders_borrowed.get_mut(order_id).unwrap();
                                self.check_if_sell_filled(
                                    order,
                                    price_tick,
                                    qty,
                                    self.data[row_num].exch_ts,
                                )?;
                            }
                        }
                    }
                }
            }
            self.remove_filled_orders();
        } else if self.data[row_num].is(EXCH_SELL_TRADE_EVENT) {
            let price_tick = (self.data[row_num].px / self.depth.tick_size()).round() as i64;
            let qty = self.data[row_num].qty;
            {
                let orders = self.orders.clone();
                let mut orders_borrowed = orders.borrow_mut();
                if self.depth.best_ask_tick() == INVALID_MAX
                    || (orders_borrowed.len() as i64) < self.depth.best_ask_tick() - price_tick
                {
                    for (_, order) in orders_borrowed.iter_mut() {
                        if order.side == Side::Buy {
                            self.check_if_buy_filled(
                                order,
                                price_tick,
                                qty,
                                self.data[row_num].exch_ts,
                            )?;
                        }
                    }
                } else {
                    for t in (price_tick..self.depth.best_ask_tick()).rev() {
                        if let Some(order_ids) = self.buy_orders.get(&t) {
                            for order_id in order_ids.clone().iter() {
                                let order = orders_borrowed.get_mut(order_id).unwrap();
                                self.check_if_buy_filled(
                                    order,
                                    price_tick,
                                    qty,
                                    self.data[row_num].exch_ts,
                                )?;
                            }
                        }
                    }
                }
            }
            self.remove_filled_orders();
        }

        // Checks
        let mut next_ts = 0;
        for rn in (self.row_num + 1)..self.data.len() {
            if self.data[rn].is(EXCH_EVENT) {
                self.row_num = rn;
                next_ts = self.data[rn].exch_ts;
                break;
            }
        }

        if next_ts <= 0 {
            let next_data = self.reader.next_data()?;
            let next_row = &next_data[0];
            next_ts = next_row.exch_ts;
            let data = mem::replace(&mut self.data, next_data);
            self.reader.release(data);
            self.row_num = 0;
        }
        Ok((next_ts, i64::MAX))
    }

    fn process_recv_order(
        &mut self,
        timestamp: i64,
        _wait_resp_order_id: Option<OrderId>,
    ) -> Result<bool, BacktestError> {
        // Processes the order part.
        while !self.orders_from.is_empty() {
            let recv_timestamp = self.orders_from.earliest_timestamp().unwrap();
            if timestamp == recv_timestamp {
                let (order, _) = self.orders_from.pop_front().unwrap();
                self.process_recv_order_(order, recv_timestamp)?;
            } else {
                assert!(recv_timestamp > timestamp);
                break;
            }
        }
        Ok(false)
    }

    fn earliest_recv_order_timestamp(&self) -> i64 {
        self.orders_from.earliest_timestamp().unwrap_or(i64::MAX)
    }

    fn earliest_send_order_timestamp(&self) -> i64 {
        self.orders_to.earliest_timestamp().unwrap_or(i64::MAX)
    }
}
