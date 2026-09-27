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

/// The exchange model with partial fills.
///
/// * Support order types: [OrdType::Limit](crate::types::OrdType::Limit)
/// * Support time-in-force: [`TimeInForce::GTC`], [`TimeInForce::FOK`], [`TimeInForce::IOC`],
///   [`TimeInForce::GTX`]
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
    // key: order_id, value: Order
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

    /// Whether executing `exec_qty` against `order` completes it, decided in lots exactly as
    /// [`fill()`](Self::fill) decides `Status::Filled`: the remainder `leaves_qty - exec_qty`
    /// rounds to zero lots. A float comparison (`exec_qty >= leaves_qty`) disagrees with `fill()`
    /// when the leaves carry float dust (0.05 - 0.03 = 0.020000000000000004 against a lot-exact
    /// exec of 0.02): `fill()` marks the order Filled, but it is never pushed to `filled_orders`,
    /// stays in the book, and the next matching event fails with `InvalidOrderStatus`.
    fn completes(&self, order: &Order, exec_qty: f64) -> bool {
        ((order.leaves_qty - exec_qty) / self.depth.lot_size()).round() <= 0f64
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
                return self.fill::<true>(
                    order,
                    timestamp,
                    true,
                    order.price_tick,
                    order.leaves_qty,
                );
            }
            Ordering::Equal => {
                // Updates the order's queue position.
                self.queue_model.trade(order, qty, &self.depth);
                let filled_qty = self.queue_model.is_filled(order, &self.depth);
                if filled_qty > 0.0 {
                    // q_ahead is negative since is_filled is true and its value represents the
                    // executable quantity of this order after execution in the queue ahead of this
                    // order.
                    let exec_qty = if self.completes(order, filled_qty) {
                        self.filled_orders.push(order.order_id);
                        order.leaves_qty
                    } else {
                        filled_qty
                    };
                    return self.fill::<true>(order, timestamp, true, order.price_tick, exec_qty);
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
                return self.fill::<true>(
                    order,
                    timestamp,
                    true,
                    order.price_tick,
                    order.leaves_qty,
                );
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
                    let exec_qty = if self.completes(order, filled_qty) {
                        self.filled_orders.push(order.order_id);
                        order.leaves_qty
                    } else {
                        filled_qty
                    };
                    return self.fill::<true>(order, timestamp, true, order.price_tick, exec_qty);
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
                        self.fill::<true>(
                            order,
                            timestamp,
                            true,
                            order.price_tick,
                            order.leaves_qty,
                        )?;
                    }
                }
            } else {
                for t in (prev_best_tick + 1)..=new_best_tick {
                    if let Some(order_ids) = self.sell_orders.get(&t) {
                        for order_id in order_ids.clone().iter() {
                            self.filled_orders.push(*order_id);
                            let order = orders_borrowed.get_mut(order_id).unwrap();
                            self.fill::<true>(
                                order,
                                timestamp,
                                true,
                                order.price_tick,
                                order.leaves_qty,
                            )?;
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
                        self.fill::<true>(
                            order,
                            timestamp,
                            true,
                            order.price_tick,
                            order.leaves_qty,
                        )?;
                    }
                }
            } else {
                for t in new_best_tick..prev_best_tick {
                    if let Some(order_ids) = self.buy_orders.get(&t) {
                        for order_id in order_ids.clone().iter() {
                            self.filled_orders.push(*order_id);
                            let order = orders_borrowed.get_mut(order_id).unwrap();
                            self.fill::<true>(
                                order,
                                timestamp,
                                true,
                                order.price_tick,
                                order.leaves_qty,
                            )?;
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
                                            self.fill::<false>(
                                                order, timestamp, false, t, exec_qty,
                                            )?;
                                            if order.status == Status::Filled {
                                                return Ok(());
                                            }
                                        }
                                    }
                                    unreachable!();
                                } else {
                                    order.status = Status::Expired;
                                    order.exch_timestamp = timestamp;
                                    Ok(())
                                }
                            }
                            TimeInForce::IOC => {
                                // The order must be executed immediately.
                                for t in self.depth.best_ask_tick()..=order.price_tick {
                                    let qty = self.depth.ask_qty_at_tick(t);
                                    if qty > 0.0 {
                                        let exec_qty = qty.min(order.leaves_qty);
                                        self.fill::<false>(order, timestamp, false, t, exec_qty)?;
                                    }
                                    if order.status == Status::Filled {
                                        return Ok(());
                                    }
                                }
                                order.status = Status::Expired;
                                order.exch_timestamp = timestamp;
                                Ok(())
                            }
                            TimeInForce::GTC => {
                                // Takes the market.
                                for t in self.depth.best_ask_tick()..order.price_tick {
                                    let qty = self.depth.ask_qty_at_tick(t);
                                    if qty > 0.0 {
                                        let exec_qty = qty.min(order.leaves_qty);
                                        self.fill::<false>(order, timestamp, false, t, exec_qty)?;
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
                                self.fill::<false>(order, timestamp, false, price_tick, leaves_qty)
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
                    // todo: set the proper upper bound.
                    for t in self.depth.best_ask_tick()..(self.depth.best_ask_tick() + 100) {
                        let qty = self.depth.ask_qty_at_tick(t);
                        if qty > 0.0 {
                            let exec_qty = qty.min(order.leaves_qty);
                            self.fill::<false>(order, timestamp, false, t, exec_qty)?;
                        }
                        if order.status == Status::Filled {
                            return Ok(());
                        }
                    }
                    order.status = Status::Expired;
                    order.exch_timestamp = timestamp;
                    Ok(())
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
                                            self.fill::<false>(
                                                order, timestamp, false, t, exec_qty,
                                            )?;
                                            if order.status == Status::Filled {
                                                return Ok(());
                                            }
                                        }
                                    }
                                    unreachable!();
                                } else {
                                    order.status = Status::Expired;
                                    order.exch_timestamp = timestamp;
                                    Ok(())
                                }
                            }
                            TimeInForce::IOC => {
                                // The order must be executed immediately.
                                for t in (order.price_tick..=self.depth.best_bid_tick()).rev() {
                                    let qty = self.depth.bid_qty_at_tick(t);
                                    if qty > 0.0 {
                                        let exec_qty = qty.min(order.leaves_qty);
                                        self.fill::<false>(order, timestamp, false, t, exec_qty)?;
                                    }
                                    if order.status == Status::Filled {
                                        return Ok(());
                                    }
                                }
                                order.status = Status::Expired;
                                order.exch_timestamp = timestamp;
                                Ok(())
                            }
                            TimeInForce::GTC => {
                                // Takes the market.
                                for t in (order.price_tick..=self.depth.best_bid_tick()).rev() {
                                    let qty = self.depth.bid_qty_at_tick(t);
                                    if qty > 0.0 {
                                        let exec_qty = qty.min(order.leaves_qty);
                                        self.fill::<false>(order, timestamp, false, t, exec_qty)?;
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
                                self.fill::<false>(order, timestamp, false, price_tick, leaves_qty)
                            }
                            _ => {
                                unreachable!();
                            }
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
                    // todo: set the proper lower bound.
                    for t in ((self.depth.best_bid_tick() - 100)..=self.depth.best_bid_tick()).rev()
                    {
                        let qty = self.depth.bid_qty_at_tick(t);
                        if qty > 0.0 {
                            let exec_qty = qty.min(order.leaves_qty);
                            self.fill::<false>(order, timestamp, false, t, exec_qty)?;
                        }
                        if order.status == Status::Filled {
                            return Ok(());
                        }
                    }
                    order.status = Status::Expired;
                    order.exch_timestamp = timestamp;
                    Ok(())
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
            let mut cancel_order = order.clone();
            self.ack_cancel(&mut cancel_order, timestamp)?;
            self.ack_new(order, timestamp)?;
            // todo: Status::Replaced or Status::New?
        } else {
            let mut order_borrowed = self.orders.borrow_mut();
            let exch_order = order_borrowed.get_mut(&order.order_id);
            let exch_order = exch_order.unwrap();

            exch_order.qty = order.qty;
            exch_order.leaves_qty = order.qty;
            exch_order.exch_timestamp = timestamp;
            // todo: Status::Replaced or Status::New?
            exch_order.status = Status::New;
            order.leaves_qty = order.qty;
            order.exch_timestamp = timestamp;
            order.status = Status::New;
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        backtest::{
            assettype::LinearAsset,
            models::{CommonFees, ConstantLatency, RiskAdverseQueueModel, TradingValueFeeModel},
            order::order_bus,
        },
        depth::HashMapMarketDepth,
    };

    const TICK: f64 = 0.1;
    const LOT: f64 = 0.01;

    type Exch = PartialFillExchange<
        LinearAsset,
        ConstantLatency,
        RiskAdverseQueueModel<HashMapMarketDepth>,
        HashMapMarketDepth,
        TradingValueFeeModel<CommonFees>,
    >;

    fn event(ev: u64, ts: i64, px: f64, qty: f64) -> Event {
        Event {
            ev,
            exch_ts: ts,
            local_ts: ts,
            px,
            qty,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        }
    }

    /// A book of 99.9 x 2.0 bid / 100.1 x 2.0 ask and one resting GTX order of `qty` at the
    /// displayed level on `side`, 2.0 of displayed quantity ahead of it.
    fn resting(side: Side, qty: f64) -> Exch {
        let (order_e2l, _order_l2e) = order_bus(ConstantLatency::new(1, 1));
        let mut exch = PartialFillExchange::new(
            HashMapMarketDepth::new(TICK, LOT),
            State::new(
                LinearAsset::new(1.0),
                TradingValueFeeModel::new(CommonFees::new(0.0, 0.0)),
            ),
            RiskAdverseQueueModel::new(),
            order_e2l,
        );
        exch.process(&event(EXCH_BID_DEPTH_EVENT, 1, 99.9, 2.0))
            .unwrap();
        exch.process(&event(EXCH_ASK_DEPTH_EVENT, 1, 100.1, 2.0))
            .unwrap();
        let price_tick = if side == Side::Buy { 999 } else { 1001 };
        let mut order = Order::new(
            1,
            price_tick,
            TICK,
            qty,
            side,
            OrdType::Limit,
            TimeInForce::GTX,
        );
        exch.ack_new(&mut order, 2).unwrap();
        assert_eq!(order.status, Status::New);
        exch
    }

    /// A print that trades against the resting order's level.
    fn print(exch: &mut Exch, side: Side, ts: i64, qty: f64) -> Result<(), BacktestError> {
        if side == Side::Buy {
            exch.process(&event(EXCH_SELL_TRADE_EVENT, ts, 99.9, qty))
        } else {
            exch.process(&event(EXCH_BUY_TRADE_EVENT, ts, 100.1, qty))
        }
    }

    fn level(exch: &Exch, side: Side) -> usize {
        let (map, tick) = if side == Side::Buy {
            (&exch.buy_orders, 999)
        } else {
            (&exch.sell_orders, 1001)
        };
        map.get(&tick).map_or(0, |ids| ids.len())
    }

    /// Defect #5b: 0.05 filled as `first` then the lot-exact rest leaves a float remainder a
    /// hair above the second exec (0.05 - 0.03 = 0.020000000000000004 against 2 lots = 0.02;
    /// 0.05 - 0.04 = 0.010000000000000002 against 1 lot = 0.01), so `filled_qty >= leaves_qty`
    /// is false while `fill()`'s own lot rounding marks the order Filled. The order must still be
    /// removed, or the next print at the level fills a Filled order (`InvalidOrderStatus`).
    fn dust_remainder_is_removed(side: Side, first: f64, rest: f64) {
        assert!(0.05 - first > rest); // the float dust this case depends on
        let mut exch = resting(side, 0.05);
        print(&mut exch, side, 10, 2.0 + first).unwrap(); // 2.0 ahead + the first piece
        {
            let orders = exch.orders.borrow();
            let order = orders
                .get(&1)
                .expect("a remainder of a lot or more stays resting");
            assert_eq!(order.status, Status::PartiallyFilled);
            assert_eq!(order.leaves_qty, 0.05 - first);
        }
        assert_eq!(level(&exch, side), 1);
        print(&mut exch, side, 20, rest).unwrap(); // the lot-exact remainder
        print(&mut exch, side, 30, 0.5).expect("no double fill on the next print");
        assert!(exch.orders.borrow().is_empty());
        assert_eq!(level(&exch, side), 0);
    }

    #[test]
    fn dust_remainder_is_removed_buy() {
        dust_remainder_is_removed(Side::Buy, 0.03, 0.02);
        dust_remainder_is_removed(Side::Buy, 0.04, 0.01);
    }

    #[test]
    fn dust_remainder_is_removed_sell() {
        dust_remainder_is_removed(Side::Sell, 0.03, 0.02);
        dust_remainder_is_removed(Side::Sell, 0.04, 0.01);
    }

    /// Defect #5: a print that fills exactly the resting leaves removes the order.
    #[test]
    fn exact_fill_is_removed() {
        for side in [Side::Buy, Side::Sell] {
            let mut exch = resting(side, 1.0);
            print(&mut exch, side, 10, 3.0).unwrap(); // 2.0 ahead + exactly the 1.0
            assert!(exch.orders.borrow().is_empty());
            assert_eq!(level(&exch, side), 0);
            print(&mut exch, side, 20, 0.5).unwrap();
        }
    }
}
