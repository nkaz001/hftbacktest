use std::mem;

use crate::{
    backtest::{
        assettype::AssetType,
        data::{Data, Reader},
        models::{
            FeeModel,
            L3FIFOQueueModel,
            L3OrderId,
            L3OrderSource,
            L3QueueModel,
            LatencyModel,
        },
        order::OrderBus,
        proc::Processor,
        state::State,
        BacktestError,
    },
    depth::{L3MarketDepth, INVALID_MAX, INVALID_MIN},
    types::{
        Event,
        Order,
        OrderId,
        Side,
        Status,
        TimeInForce,
        EXCH_ASK_ADD_ORDER_EVENT,
        EXCH_ASK_DEPTH_CLEAR_EVENT,
        EXCH_BID_ADD_ORDER_EVENT,
        EXCH_BID_DEPTH_CLEAR_EVENT,
        EXCH_CANCEL_ORDER_EVENT,
        EXCH_DEPTH_CLEAR_EVENT,
        EXCH_EVENT,
        EXCH_FILL_EVENT,
        EXCH_MODIFY_ORDER_EVENT,
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
pub struct L3NoPartialFillExchange<AT, LM, MD, FM>
where
    AT: AssetType,
    LM: LatencyModel,
    MD: L3MarketDepth,
    FM: FeeModel,
{
    reader: Reader<Event>,
    data: Data<Event>,
    row_num: usize,
    orders_to: OrderBus,
    orders_from: OrderBus,

    depth: MD,
    state: State<AT, FM>,
    order_latency: LM,
    queue_model: L3FIFOQueueModel,
}

impl<AT, LM, MD, FM> L3NoPartialFillExchange<AT, LM, MD, FM>
where
    AT: AssetType,
    LM: LatencyModel,
    MD: L3MarketDepth,
    FM: FeeModel,
    BacktestError: From<<MD as L3MarketDepth>::Error>,
{
    /// Constructs an instance of `NoPartialFillExchange`.
    pub fn new(
        reader: Reader<Event>,
        depth: MD,
        state: State<AT, FM>,
        order_latency: LM,
        orders_to: OrderBus,
        orders_from: OrderBus,
    ) -> Self {
        Self {
            reader,
            data: Data::empty(),
            row_num: 0,
            orders_to,
            orders_from,
            depth,
            state,
            order_latency,
            queue_model: L3FIFOQueueModel::new(),
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

    fn fill(
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
        let local_recv_timestamp =
            order.exch_timestamp + self.order_latency.response(timestamp, &order);

        self.state.apply_fill(order);
        self.orders_to.append(order.clone(), local_recv_timestamp);
        Ok(())
    }

    fn on_best_bid_update(
        &mut self,
        prev_best_tick: i64,
        new_best_tick: i64,
        timestamp: i64,
    ) -> Result<(), BacktestError> {
        // If the best has been significantly updated compared to the previous best, it would be
        // better to iterate orders dict instead of order price ladder.
        let mut filled_orders = Vec::new();
        if prev_best_tick == INVALID_MIN
            || (self.queue_model.orders.len() as i64) < new_best_tick - prev_best_tick
        {
            let mut filled_orders = Vec::new();
            for (order_id, &mut (side, price_tick)) in self.queue_model.orders.iter_mut() {
                if side == Side::Sell && price_tick <= new_best_tick {
                    let priority = self.queue_model.ask_queue.get_mut(&price_tick).unwrap();
                    let mut i = 0;
                    while i < priority.len() {
                        let order = priority.get(i).unwrap();
                        if order_id.is(order) {
                            filled_orders.push(priority.remove(i));
                            break;
                        }
                    }
                }
            }
        } else {
            for t in (prev_best_tick + 1)..=new_best_tick {
                if let Some(orders) = self.queue_model.ask_queue.get_mut(&t) {
                    let mut i = 0;
                    while i < orders.len() {
                        let order = orders.get(i).unwrap();
                        let order_source =
                            order.q.as_any().downcast_ref::<L3OrderSource>().unwrap();
                        if *order_source == L3OrderSource::Backtest {
                            filled_orders.push(orders.remove(i));
                        }
                    }
                }
            }
        }
        for mut order in filled_orders {
            let price_tick = order.price_tick;
            self.fill(&mut order, timestamp, true, price_tick)?;
        }
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
        let mut filled_orders = Vec::new();
        if prev_best_tick == INVALID_MAX
            || (self.queue_model.orders.len() as i64) < prev_best_tick - new_best_tick
        {
            for (order_id, &mut (side, price_tick)) in self.queue_model.orders.iter_mut() {
                if side == Side::Buy && price_tick >= new_best_tick {
                    let priority = self.queue_model.ask_queue.get_mut(&price_tick).unwrap();
                    let mut i = 0;
                    while i < priority.len() {
                        let order = priority.get(i).unwrap();
                        if order_id.is(order) {
                            filled_orders.push(priority.remove(i));
                            break;
                        }
                    }
                }
            }
        } else {
            for t in new_best_tick..prev_best_tick {
                if let Some(orders) = self.queue_model.bid_queue.get_mut(&t) {
                    let mut i = 0;
                    while i < orders.len() {
                        let order = orders.get(i).unwrap();
                        let order_source =
                            order.q.as_any().downcast_ref::<L3OrderSource>().unwrap();
                        if *order_source == L3OrderSource::Backtest {
                            filled_orders.push(orders.remove(i));
                        }
                    }
                }
            }
        }
        for mut order in filled_orders {
            let price_tick = order.price_tick;
            self.fill(&mut order, timestamp, true, price_tick)?;
        }
        Ok(())
    }

    fn ack_new(&mut self, mut order: Order, timestamp: i64) -> Result<(), BacktestError> {
        if self
            .queue_model
            .orders
            .contains_key(&L3OrderId::Backtest(order.order_id))
        {
            return Err(BacktestError::OrderIdExist);
        }

        if order.side == Side::Buy {
            // Checks if the buy order price is greater than or equal to the current best ask.
            if order.price_tick >= self.depth.best_ask_tick() {
                if order.time_in_force == TimeInForce::GTX {
                    order.status = Status::Expired;

                    order.exch_timestamp = timestamp;
                    let local_recv_timestamp =
                        timestamp + self.order_latency.response(timestamp, &order);
                    self.orders_to.append(order.clone(), local_recv_timestamp);
                    Ok(())
                } else {
                    // Takes the market.
                    self.fill(&mut order, timestamp, false, self.depth.best_ask_tick())
                }
            } else {
                // Initializes the order's queue position.
                order.status = Status::New;
                order.exch_timestamp = timestamp;

                self.queue_model
                    .add_order(L3OrderId::Backtest(order.order_id), order.clone())?;

                let local_recv_timestamp =
                    timestamp + self.order_latency.response(timestamp, &order);
                self.orders_to.append(order, local_recv_timestamp);
                Ok(())
            }
        } else {
            // Checks if the sell order price is less than or equal to the current best bid.
            if order.price_tick <= self.depth.best_bid_tick() {
                if order.time_in_force == TimeInForce::GTX {
                    order.status = Status::Expired;

                    order.exch_timestamp = timestamp;
                    let local_recv_timestamp =
                        timestamp + self.order_latency.response(timestamp, &order);
                    self.orders_to.append(order.clone(), local_recv_timestamp);
                    Ok(())
                } else {
                    // Takes the market.
                    self.fill(&mut order, timestamp, false, self.depth.best_bid_tick())
                }
            } else {
                // Initializes the order's queue position.
                order.status = Status::New;
                order.exch_timestamp = timestamp;

                self.queue_model
                    .add_order(L3OrderId::Backtest(order.order_id), order.clone())?;

                let local_recv_timestamp =
                    timestamp + self.order_latency.response(timestamp, &order);
                self.orders_to.append(order, local_recv_timestamp);
                Ok(())
            }
        }
    }

    fn ack_cancel(&mut self, mut order: Order, timestamp: i64) -> Result<(), BacktestError> {
        match self
            .queue_model
            .cancel_order(L3OrderId::Backtest(order.order_id))
        {
            Ok(mut exch_order) => {
                // Makes the response.
                exch_order.status = Status::Canceled;
                exch_order.exch_timestamp = timestamp;
                let local_recv_timestamp =
                    timestamp + self.order_latency.response(timestamp, &exch_order);
                self.orders_to
                    .append(exch_order.clone(), local_recv_timestamp);
                Ok(())
            }
            Err(BacktestError::OrderNotFound) => {
                order.req = Status::Rejected;
                order.exch_timestamp = timestamp;
                let local_recv_timestamp =
                    timestamp + self.order_latency.response(timestamp, &order);
                self.orders_to.append(order, local_recv_timestamp);
                return Ok(());
            }
            Err(e) => Err(e),
        }
    }

    fn ack_modify(&mut self, mut order: Order, timestamp: i64) -> Result<(), BacktestError> {
        todo!()
    }
}

impl<AT, LM, MD, FM> Processor for L3NoPartialFillExchange<AT, LM, MD, FM>
where
    AT: AssetType,
    LM: LatencyModel,
    MD: L3MarketDepth,
    FM: FeeModel,
    BacktestError: From<<MD as L3MarketDepth>::Error>,
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
            self.depth.clear_depth(Side::Buy);
        } else if self.data[row_num].is(EXCH_ASK_DEPTH_CLEAR_EVENT) {
            self.depth.clear_depth(Side::Sell);
        } else if self.data[row_num].is(EXCH_DEPTH_CLEAR_EVENT) {
            self.depth.clear_depth(Side::None);
        } else if self.data[row_num].is(EXCH_BID_ADD_ORDER_EVENT) {
            let (prev_best_bid_tick, best_bid_tick) = self.depth.add_buy_order(
                self.data[row_num].order_id,
                self.data[row_num].px,
                self.data[row_num].qty,
                self.data[row_num].exch_ts,
            )?;
            if best_bid_tick > prev_best_bid_tick {
                self.on_best_bid_update(
                    prev_best_bid_tick,
                    best_bid_tick,
                    self.data[row_num].exch_ts,
                )?;
            }
        } else if self.data[row_num].is(EXCH_ASK_ADD_ORDER_EVENT) {
            let (prev_best_ask_tick, best_ask_tick) = self.depth.add_sell_order(
                self.data[row_num].order_id,
                self.data[row_num].px,
                self.data[row_num].qty,
                self.data[row_num].exch_ts,
            )?;
            if best_ask_tick < prev_best_ask_tick {
                self.on_best_ask_update(
                    prev_best_ask_tick,
                    best_ask_tick,
                    self.data[row_num].exch_ts,
                )?;
            }
        } else if self.data[row_num].is(EXCH_MODIFY_ORDER_EVENT) {
            let (side, prev_best_tick, best_tick) = self.depth.modify_order(
                self.data[row_num].order_id,
                self.data[row_num].px,
                self.data[row_num].qty,
                self.data[row_num].exch_ts,
            )?;
            if side == Side::Buy {
                if best_tick > prev_best_tick {
                    self.on_best_bid_update(prev_best_tick, best_tick, self.data[row_num].exch_ts)?;
                }
            } else {
                if best_tick < prev_best_tick {
                    self.on_best_ask_update(prev_best_tick, best_tick, self.data[row_num].exch_ts)?;
                }
            }
        } else if self.data[row_num].is(EXCH_CANCEL_ORDER_EVENT) {
            let _ = self
                .depth
                .delete_order(self.data[row_num].order_id, self.data[row_num].exch_ts)?;
            self.queue_model
                .cancel_order(L3OrderId::Market(self.data[row_num].order_id))?;
        } else if self.data[row_num].is(EXCH_FILL_EVENT) {
            let filled_orders = self
                .queue_model
                .fill(L3OrderId::Market(self.data[row_num].order_id), false)?;
            let timestamp = self.data[row_num].exch_ts;
            for mut order in filled_orders {
                let price_tick = order.price_tick;
                self.fill(&mut order, timestamp, true, price_tick)?;
            }
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
        while self.orders_from.len() > 0 {
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
