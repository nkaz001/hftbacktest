use std::collections::{HashMap, hash_map::Entry};

use crate::{
    backtest::{
        BacktestError,
        assettype::AssetType,
        models::{FeeModel, LatencyModel},
        order::LocalToExch,
        proc::{LocalProcessor, Processor},
        state::State,
    },
    depth::L3MarketDepth,
    types::{
        Event,
        LOCAL_ASK_ADD_ORDER_EVENT,
        LOCAL_ASK_DEPTH_CLEAR_EVENT,
        LOCAL_BID_ADD_ORDER_EVENT,
        LOCAL_BID_DEPTH_CLEAR_EVENT,
        LOCAL_CANCEL_ORDER_EVENT,
        LOCAL_DEPTH_CLEAR_EVENT,
        LOCAL_EVENT,
        LOCAL_MODIFY_ORDER_EVENT,
        LOCAL_TRADE_EVENT,
        OrdType,
        Order,
        OrderId,
        Side,
        StateValues,
        Status,
        TimeInForce,
    },
};

/// The Level3 Market-By-Order local model.
pub struct L3Local<AT, LM, MD, FM>
where
    AT: AssetType,
    LM: LatencyModel,
    MD: L3MarketDepth,
    FM: FeeModel,
{
    orders: HashMap<OrderId, Order>,
    order_l2e: LocalToExch<LM>,
    depth: MD,
    state: State<AT, FM>,
    trades: Vec<Event>,
    last_feed_latency: Option<(i64, i64)>,
    last_order_latency: Option<(i64, i64, i64)>,
}

impl<AT, LM, MD, FM> L3Local<AT, LM, MD, FM>
where
    AT: AssetType,
    LM: LatencyModel,
    MD: L3MarketDepth,
    FM: FeeModel,
{
    /// Constructs an instance of `L3Local`.
    pub fn new(
        depth: MD,
        state: State<AT, FM>,
        trade_len: usize,
        order_l2e: LocalToExch<LM>,
    ) -> Self {
        Self {
            orders: Default::default(),
            order_l2e,
            depth,
            state,
            trades: Vec::with_capacity(trade_len),
            last_feed_latency: None,
            last_order_latency: None,
        }
    }
}

impl<AT, LM, MD, FM> LocalProcessor<MD> for L3Local<AT, LM, MD, FM>
where
    AT: AssetType,
    LM: LatencyModel,
    MD: L3MarketDepth,
    FM: FeeModel,
    BacktestError: From<<MD as L3MarketDepth>::Error>,
{
    fn submit_order(
        &mut self,
        order_id: OrderId,
        side: Side,
        price: f64,
        qty: f64,
        order_type: OrdType,
        time_in_force: TimeInForce,
        current_timestamp: i64,
    ) -> Result<(), BacktestError> {
        if self.orders.contains_key(&order_id) {
            return Err(BacktestError::OrderIdExist);
        }

        let price_tick = (price / self.depth.tick_size()).round() as i64;
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

        self.order_l2e.request(order, |order| {
            order.req = Status::Rejected;
        });

        Ok(())
    }

    fn modify(
        &mut self,
        order_id: OrderId,
        price: f64,
        qty: f64,
        current_timestamp: i64,
    ) -> Result<(), BacktestError> {
        let order = self
            .orders
            .get_mut(&order_id)
            .ok_or(BacktestError::OrderNotFound)?;

        if order.req != Status::None {
            return Err(BacktestError::OrderRequestInProcess);
        }

        let orig_price_tick = order.price_tick;
        let orig_qty = order.qty;

        let price_tick = (price / self.depth.tick_size()).round() as i64;
        order.price_tick = price_tick;
        order.qty = qty;

        order.req = Status::Replaced;
        order.local_timestamp = current_timestamp;

        self.order_l2e.request(order.clone(), |order| {
            order.req = Status::Rejected;
            order.price_tick = orig_price_tick;
            order.qty = orig_qty;
        });

        Ok(())
    }

    fn cancel(&mut self, order_id: OrderId, current_timestamp: i64) -> Result<(), BacktestError> {
        let order = self
            .orders
            .get_mut(&order_id)
            .ok_or(BacktestError::OrderNotFound)?;

        if order.req != Status::None {
            return Err(BacktestError::OrderRequestInProcess);
        }

        order.req = Status::Canceled;
        order.local_timestamp = current_timestamp;

        self.order_l2e.request(order.clone(), |order| {
            order.req = Status::Rejected;
        });

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
        self.state_values().position
    }

    fn state_values(&self) -> &StateValues {
        self.state.values()
    }

    fn depth(&self) -> &MD {
        &self.depth
    }

    fn orders(&self) -> &HashMap<OrderId, Order> {
        &self.orders
    }

    fn last_trades(&self) -> &[Event] {
        self.trades.as_slice()
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

impl<AT, LM, MD, FM> Processor for L3Local<AT, LM, MD, FM>
where
    AT: AssetType,
    LM: LatencyModel,
    MD: L3MarketDepth,
    FM: FeeModel,
    BacktestError: From<<MD as L3MarketDepth>::Error>,
{
    fn event_seen_timestamp(&self, event: &Event) -> Option<i64> {
        event.is(LOCAL_EVENT).then_some(event.local_ts)
    }

    fn process(&mut self, ev: &Event) -> Result<(), BacktestError> {
        // Processes a depth event
        if ev.is(LOCAL_BID_DEPTH_CLEAR_EVENT) {
            self.depth.clear_orders(Side::Buy);
        } else if ev.is(LOCAL_ASK_DEPTH_CLEAR_EVENT) {
            self.depth.clear_orders(Side::Sell);
        } else if ev.is(LOCAL_DEPTH_CLEAR_EVENT) {
            self.depth.clear_orders(Side::None);
        } else if ev.is(LOCAL_BID_ADD_ORDER_EVENT) {
            self.depth
                .add_buy_order(ev.order_id, ev.px, ev.qty, ev.local_ts)?;
        } else if ev.is(LOCAL_ASK_ADD_ORDER_EVENT) {
            self.depth
                .add_sell_order(ev.order_id, ev.px, ev.qty, ev.local_ts)?;
        } else if ev.is(LOCAL_MODIFY_ORDER_EVENT) {
            self.depth
                .modify_order(ev.order_id, ev.px, ev.qty, ev.local_ts)?;
        } else if ev.is(LOCAL_CANCEL_ORDER_EVENT) {
            self.depth.delete_order(ev.order_id, ev.local_ts)?;
        }
        // Processes a trade event
        else if ev.is(LOCAL_TRADE_EVENT) && self.trades.capacity() > 0 {
            self.trades.push(ev.clone());
        }

        // Stores the current feed latency
        self.last_feed_latency = Some((ev.exch_ts, ev.local_ts));

        Ok(())
    }

    fn process_recv_order(
        &mut self,
        timestamp: i64,
        wait_resp_order_id: Option<OrderId>,
    ) -> Result<bool, BacktestError> {
        // Processes the order part.
        let mut wait_resp_order_received = false;
        while let Some(order) = self.order_l2e.receive(timestamp) {
            // Updates the order latency only if it has a valid exchange timestamp. When the
            // order is rejected before it reaches the matching engine, it has no exchange
            // timestamp. This situation occurs in crypto exchanges.
            if order.exch_timestamp > 0 {
                self.last_order_latency =
                    Some((order.local_timestamp, order.exch_timestamp, timestamp));
            }

            if let Some(wait_resp_order_id) = wait_resp_order_id {
                if order.order_id == wait_resp_order_id {
                    wait_resp_order_received = true;
                }
            }

            // Processes receiving order response.
            if order.status == Status::Filled {
                self.state.apply_fill(&order);
            }
            // Applies the received order response to the local orders.
            match self.orders.entry(order.order_id) {
                Entry::Occupied(mut entry) => {
                    let local_order = entry.get_mut();
                    if order.req == Status::Rejected {
                        if order.local_timestamp == local_order.local_timestamp {
                            if local_order.req == Status::New {
                                local_order.req = Status::None;
                                local_order.status = Status::Expired;
                            } else {
                                local_order.req = Status::None;
                            }
                        }
                    } else {
                        local_order.update(&order);
                    }
                }
                Entry::Vacant(entry) => {
                    if order.req != Status::Rejected {
                        entry.insert(order);
                    }
                }
            }
        }
        Ok(wait_resp_order_received)
    }

    fn earliest_recv_order_timestamp(&self) -> i64 {
        self.order_l2e
            .earliest_recv_order_timestamp()
            .unwrap_or(i64::MAX)
    }

    fn earliest_send_order_timestamp(&self) -> i64 {
        self.order_l2e
            .earliest_send_order_timestamp()
            .unwrap_or(i64::MAX)
    }
}
