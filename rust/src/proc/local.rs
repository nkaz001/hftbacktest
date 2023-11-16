use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::mem;
use crate::assettype::AssetType;
use crate::depth::MarketDepth;
use crate::Error;
use crate::models::LatencyModel;
use crate::order::{Order, OrderBus, OrdType, Side, Status, TimeInForce};
use crate::proc::proc::{LocalProcessor, Processor};
use crate::reader::{
    BUY,
    SELL,
    LOCAL_ASK_DEPTH_CLEAR_EVENT,
    LOCAL_ASK_DEPTH_EVENT,
    LOCAL_ASK_DEPTH_SNAPSHOT_EVENT,
    LOCAL_BID_DEPTH_CLEAR_EVENT,
    LOCAL_BID_DEPTH_EVENT,
    LOCAL_BID_DEPTH_SNAPSHOT_EVENT,
    LOCAL_EVENT,
    LOCAL_TRADE_EVENT,
    Reader,
    Data,
    Row
};
use crate::state::State;

pub struct Local<A, Q, L>
    where
        A: AssetType,
        Q: Clone,
        L: LatencyModel
{
    pub reader: Reader<Row>,
    pub data: Data<Row>,
    row_num: usize,

    pub orders: HashMap<i64, Order<Q>>,

    pub orders_to: OrderBus<Q>,
    pub orders_from: OrderBus<Q>,

    pub depth: MarketDepth,
    pub state: State<A>,
    order_latency: L,

    trades: Vec<Row>
}

impl<A, Q, L> Local<A, Q, L>
    where
        A: AssetType,
        Q: Clone + Default,
        L: LatencyModel
{
    pub fn new(
        reader: Reader<Row>,
        depth: MarketDepth,
        state: State<A>,
        order_latency: L,
        trade_len: usize,
        orders_to: OrderBus<Q>,
        orders_from: OrderBus<Q>
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
        }
    }

    fn process_recv_order_(
        &mut self,
        order: Order<Q>,
        _recv_timestamp: i64,
        _wait_resp: i64,
        next_timestamp: i64
    ) -> Result<i64, Error> {
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

        // Bypass next_timestamp
        Ok(next_timestamp)
    }

    pub fn clear_last_trades(&mut self) {
        self.trades.clear();
    }
}

impl<A, Q, L> LocalProcessor<A, Q> for Local<A, Q, L>
    where
        A: AssetType,
        Q: Clone + Default,
        L: LatencyModel
{
    fn submit_order(
        &mut self,
        order_id: i64,
        side: Side,
        price: f32,
        qty: f32,
        order_type: OrdType,
        time_in_force: TimeInForce,
        current_timestamp: i64
    ) -> Result<(), Error> {
        if self.orders.contains_key(&order_id) {
            return Err(Error::OrderAlreadyExist);
        }

        let price_tick = (price / self.depth.tick_size).round() as i32;
        let mut order = Order::new(
            order_id,
            price_tick,
            self.depth.tick_size,
            qty,
            side,
            order_type,
            time_in_force
        );
        order.req = Status::New;
        let exch_recv_timestamp = current_timestamp + self.order_latency.entry(current_timestamp, &order);

        self.orders_to.append(order.clone(), exch_recv_timestamp);
        self.orders.insert(order.order_id, order);
        Ok(())
    }

    fn cancel(&mut self, order_id: i64, current_timestamp: i64) -> Result<(), Error> {
        let order = self.orders.get_mut(&order_id).ok_or(Error::OrderNotFound)?;

        if order.req != Status::None {
            return Err(Error::OrderRequestInProcess);
        }

        order.req = Status::Canceled;
        let exch_recv_timestamp = current_timestamp + self.order_latency.entry(current_timestamp, order);

        self.orders_to.append(order.clone(), exch_recv_timestamp);
        Ok(())
    }

    fn clear_inactive_orders(&mut self) {
        self.orders.retain(
            |_, order| order.status != Status::Expired
                && order.status != Status::Filled
                && order.status != Status::Canceled
        )
    }

    fn state(&self) -> &State<A> {
        &self.state
    }

    fn depth(&self) -> &MarketDepth {
        &self.depth
    }

    fn orders(&self) -> &HashMap<i64, Order<Q>> {
        &self.orders
    }
}

impl<A, Q, L> Processor for Local<A, Q, L>
    where
        A: AssetType,
        Q: Clone + Default,
        L: LatencyModel
{
    fn initialize_data(&mut self) -> Result<i64, Error> {
        self.data = self.reader.next()?;
        for rn in 0..self.data.len() {
            if self.data[rn].ev & LOCAL_EVENT == LOCAL_EVENT {
                self.row_num = rn;
                return Ok(self.data[rn].local_ts);
            }
        }
        Err(Error::EndOfData)
    }

    fn process_data(&mut self) -> Result<(i64, i64), Error> {
        let row = &self.data[self.row_num];
        // Processes a depth event
        if row.ev & LOCAL_BID_DEPTH_CLEAR_EVENT == LOCAL_BID_DEPTH_CLEAR_EVENT {
            self.depth.clear_depth(BUY, row.px);
        } else if row.ev & LOCAL_ASK_DEPTH_CLEAR_EVENT == LOCAL_ASK_DEPTH_CLEAR_EVENT {
            self.depth.clear_depth(SELL, row.px);
        } else if row.ev & LOCAL_BID_DEPTH_EVENT == LOCAL_BID_DEPTH_EVENT
            || row.ev & LOCAL_BID_DEPTH_SNAPSHOT_EVENT == LOCAL_BID_DEPTH_SNAPSHOT_EVENT {
            self.depth.update_bid_depth(
                row.px,
                row.qty,
                row.local_ts
            );
        } else if row.ev & LOCAL_ASK_DEPTH_EVENT == LOCAL_ASK_DEPTH_EVENT
            || row.ev & LOCAL_ASK_DEPTH_SNAPSHOT_EVENT == LOCAL_ASK_DEPTH_SNAPSHOT_EVENT {
            self.depth.update_ask_depth(
                row.px,
                row.qty,
                row.local_ts
            );
        }
        // Processes a trade event
        else if row.ev & LOCAL_TRADE_EVENT == LOCAL_TRADE_EVENT {
            if self.trades.capacity() > 0 {
                self.trades.push(row.clone());
            }
        }

        // Checks
        let mut next_ts = 0;
        for rn in (self.row_num + 1)..self.data.len() {
            if self.data[rn].ev & LOCAL_EVENT == LOCAL_EVENT {
                self.row_num = rn;
                next_ts = self.data[rn].local_ts;
                break;
            }
        }

        if next_ts <= 0 {
            let next_data = unsafe {
                self.reader.next()?
            };
            let next_row = &next_data[0];
            next_ts = next_row.local_ts;
            let data = mem::replace(&mut self.data, next_data);
            self.reader.release(data);
            self.row_num = 0;
        }
        Ok((next_ts, i64::MAX))
    }

    fn process_recv_order(&mut self, timestamp: i64, wait_resp: i64) -> Result<i64, Error> {
        // Processes the order part.
        let mut next_timestamp = i64::MAX;
        while self.orders_from.len() > 0 {
            let recv_timestamp = self.orders_from.get_head_timestamp().unwrap();
            if timestamp == recv_timestamp {
                let order = self.orders_from.remove(0);
                next_timestamp = self.process_recv_order_(
                    order,
                    recv_timestamp,
                    wait_resp,
                    next_timestamp
                )?;
            } else {
                assert!(recv_timestamp > timestamp);
                break;
            }
        }
        Ok(next_timestamp)
    }

    fn frontmost_recv_order_timestamp(&self) -> i64 {
        self.orders_from.frontmost_timestamp()
    }

    fn frontmost_send_order_timestamp(&self) -> i64 {
        self.orders_to.frontmost_timestamp()
    }
}