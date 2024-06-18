use std::{
    collections::{hash_map::Entry, HashMap},
    mem,
};

use crate::{
    backtest::{
        assettype::AssetType,
        models::LatencyModel,
        order::OrderBus,
        proc::proc::{LocalProcessor, Processor},
        reader::{Data, Reader},
        state::State,
        BacktestError,
    },
    depth::{L2MarketDepth, MarketDepth},
    types::{
        Event,
        OrdType,
        Order,
        Side,
        StateValues,
        Status,
        TimeInForce,
        BUY,
        LOCAL_ASK_DEPTH_CLEAR_EVENT,
        LOCAL_ASK_DEPTH_EVENT,
        LOCAL_ASK_DEPTH_SNAPSHOT_EVENT,
        LOCAL_BID_DEPTH_CLEAR_EVENT,
        LOCAL_BID_DEPTH_EVENT,
        LOCAL_BID_DEPTH_SNAPSHOT_EVENT,
        LOCAL_EVENT,
        LOCAL_TRADE_EVENT,
        SELL,
        WAIT_ORDER_RESPONSE_ANY,
    },
};

/// The local model.
pub struct Local<AT, LM, MD>
where
    AT: AssetType,
    LM: LatencyModel,
    MD: MarketDepth,
{
    reader: Reader<Event>,
    data: Data<Event>,
    row_num: usize,
    orders: HashMap<i64, Order>,
    orders_to: OrderBus,
    orders_from: OrderBus,
    depth: MD,
    state: State<AT>,
    order_latency: LM,
    trades: Vec<Event>,
    last_feed_latency: Option<(i64, i64)>,
    last_order_latency: Option<(i64, i64, i64)>,
}

impl<AT, LM, MD> Local<AT, LM, MD>
where
    AT: AssetType,
    LM: LatencyModel,
    MD: MarketDepth,
{
    /// Constructs an instance of `Local`.
    pub fn new(
        reader: Reader<Event>,
        depth: MD,
        state: State<AT>,
        order_latency: LM,
        trade_len: usize,
        orders_to: OrderBus,
        orders_from: OrderBus,
    ) -> Self {
        Self {
            reader,
            data: Data::empty(),
            row_num: 0,
            orders: Default::default(),
            orders_to,
            orders_from,
            depth,
            state,
            order_latency,
            trades: Vec::with_capacity(trade_len),
            last_feed_latency: None,
            last_order_latency: None,
        }
    }

    fn process_recv_order_(&mut self, order: Order) -> Result<(), BacktestError> {
        if order.status == Status::Filled {
            self.state.apply_fill(&order);
        }
        // Applies the received order response to the local orders.
        match self.orders.entry(order.order_id) {
            Entry::Occupied(mut entry) => {
                *entry.get_mut() = order;
            }
            Entry::Vacant(entry) => {
                entry.insert(order);
            }
        }
        Ok(())
    }
}

impl<AT, LM, MD> LocalProcessor<MD, Event> for Local<AT, LM, MD>
where
    AT: AssetType,
    LM: LatencyModel,
    MD: MarketDepth + L2MarketDepth,
{
    fn submit_order(
        &mut self,
        order_id: i64,
        side: Side,
        price: f32,
        qty: f32,
        order_type: OrdType,
        time_in_force: TimeInForce,
        current_timestamp: i64,
    ) -> Result<(), BacktestError> {
        if self.orders.contains_key(&order_id) {
            return Err(BacktestError::OrderIdExist);
        }

        let price_tick = (price / self.depth.tick_size()).round() as i32;
        let mut order = Order::new(
            order_id,
            price_tick,
            self.depth.tick_size(),
            qty,
            side,
            order_type,
            time_in_force,
        );
        order.req = Status::New;
        order.local_timestamp = current_timestamp;
        self.orders.insert(order.order_id, order.clone());

        let order_entry_latency = self.order_latency.entry(current_timestamp, &order);
        // Negative latency indicates that the order is rejected for technical reasons, and its
        // value represents the latency that the local experiences when receiving the rejection
        // notification.
        if order_entry_latency < 0 {
            // Rejects the order.
            let rej_recv_timestamp = current_timestamp - order_entry_latency;
            self.orders_from.append(order, rej_recv_timestamp);
        } else {
            let exch_recv_timestamp = current_timestamp + order_entry_latency;
            self.orders_to.append(order, exch_recv_timestamp);
        }
        Ok(())
    }

    fn cancel(&mut self, order_id: i64, current_timestamp: i64) -> Result<(), BacktestError> {
        let order = self
            .orders
            .get_mut(&order_id)
            .ok_or(BacktestError::OrderNotFound)?;

        if order.req != Status::None {
            return Err(BacktestError::OrderRequestInProcess);
        }

        order.req = Status::Canceled;
        order.local_timestamp = current_timestamp;
        let order_entry_latency = self.order_latency.entry(current_timestamp, order);
        // Negative latency indicates that the order is rejected for technical reasons, and its
        // value represents the latency that the local experiences when receiving the rejection
        // notification.
        if order_entry_latency < 0 {
            // Rejects the order.
            let rej_recv_timestamp = current_timestamp - order_entry_latency;
            self.orders_from.append(order.clone(), rej_recv_timestamp);
        } else {
            let exch_recv_timestamp = current_timestamp + order_entry_latency;
            self.orders_to.append(order.clone(), exch_recv_timestamp);
        }
        Ok(())
    }

    fn clear_inactive_orders(&mut self) {
        self.orders.retain(|_, order| {
            order.status != Status::Expired
                && order.status != Status::Filled
                && order.status != Status::Canceled
        })
    }

    fn position(&self) -> f64 {
        self.state.position
    }

    fn state_values(&self) -> StateValues {
        StateValues {
            position: self.state.position,
            balance: self.state.balance,
            fee: self.state.fee,
            trade_num: self.state.trade_num,
            trade_qty: self.state.trade_qty,
            trade_amount: self.state.trade_amount,
        }
    }

    fn depth(&self) -> &MD {
        &self.depth
    }

    fn orders(&self) -> &HashMap<i64, Order> {
        &self.orders
    }

    fn trade(&self) -> &Vec<Event> {
        &self.trades
    }

    fn clear_last_trades(&mut self) {
        self.trades.clear();
    }

    fn feed_latency(&self) -> Option<(i64, i64)> {
        self.last_feed_latency
    }

    fn order_latency(&self) -> Option<(i64, i64, i64)> {
        self.last_order_latency
    }
}

impl<AT, LM, MD> Processor for Local<AT, LM, MD>
where
    AT: AssetType,
    LM: LatencyModel,
    MD: MarketDepth + L2MarketDepth,
{
    fn initialize_data(&mut self) -> Result<i64, BacktestError> {
        self.data = self.reader.next()?;
        for rn in 0..self.data.len() {
            if self.data[rn].is(LOCAL_EVENT) {
                self.row_num = rn;
                let tmp = self.data[rn].local_ts;
                return Ok(tmp);
            }
        }
        Err(BacktestError::EndOfData)
    }

    fn process_data(&mut self) -> Result<(i64, i64), BacktestError> {
        let ev = &self.data[self.row_num];
        // Processes a depth event
        if ev.is(LOCAL_BID_DEPTH_CLEAR_EVENT) {
            self.depth.clear_depth(BUY, ev.px);
        } else if ev.is(LOCAL_ASK_DEPTH_CLEAR_EVENT) {
            self.depth.clear_depth(SELL, ev.px);
        } else if ev.is(LOCAL_BID_DEPTH_EVENT) || ev.is(LOCAL_BID_DEPTH_SNAPSHOT_EVENT) {
            self.depth.update_bid_depth(ev.px, ev.qty, ev.local_ts);
        } else if ev.is(LOCAL_ASK_DEPTH_EVENT) || ev.is(LOCAL_ASK_DEPTH_SNAPSHOT_EVENT) {
            self.depth.update_ask_depth(ev.px, ev.qty, ev.local_ts);
        }
        // Processes a trade event
        else if ev.is(LOCAL_TRADE_EVENT) {
            if self.trades.capacity() > 0 {
                self.trades.push(ev.clone());
            }
        }

        // Stores the current feed latency
        self.last_feed_latency = Some((ev.exch_ts, ev.local_ts));

        // Checks
        let mut next_ts = 0;
        for rn in (self.row_num + 1)..self.data.len() {
            if self.data[rn].is(LOCAL_EVENT) {
                self.row_num = rn;
                next_ts = self.data[rn].local_ts;
                break;
            }
        }

        if next_ts <= 0 {
            let next_data = self.reader.next()?;
            let next_row = &next_data[0];
            next_ts = next_row.local_ts;
            let data = mem::replace(&mut self.data, next_data);
            self.reader.release(data);
            self.row_num = 0;
        }

        Ok((next_ts, i64::MAX))
    }

    fn process_recv_order(
        &mut self,
        timestamp: i64,
        wait_resp_order_id: i64,
    ) -> Result<bool, BacktestError> {
        // Processes the order part.
        let mut wait_resp_order_received = false;
        while self.orders_from.len() > 0 {
            let recv_timestamp = self.orders_from.earliest_timestamp().unwrap();
            if timestamp == recv_timestamp {
                let (order, _) = self.orders_from.pop_front().unwrap();
                self.last_order_latency =
                    Some((order.local_timestamp, order.exch_timestamp, recv_timestamp));
                if order.order_id == wait_resp_order_id
                    || wait_resp_order_id == WAIT_ORDER_RESPONSE_ANY
                {
                    wait_resp_order_received = true;
                }
                self.process_recv_order_(order)?;
            } else {
                assert!(recv_timestamp > timestamp);
                break;
            }
        }
        Ok(wait_resp_order_received)
    }

    fn earliest_recv_order_timestamp(&self) -> i64 {
        self.orders_from.earliest_timestamp().unwrap_or(i64::MAX)
    }

    fn earliest_send_order_timestamp(&self) -> i64 {
        self.orders_to.earliest_timestamp().unwrap_or(i64::MAX)
    }
}
