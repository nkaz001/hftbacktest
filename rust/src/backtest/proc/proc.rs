use std::collections::HashMap;

use crate::{
    backtest::{state::StateValues, Error},
    depth::MarketDepth,
    ty::{OrdType, Order, Event, Side, TimeInForce},
};

pub trait LocalProcessor<Q, MD>: Processor
where
    Q: Clone,
    MD: MarketDepth,
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
    ) -> Result<(), Error>;
    fn cancel(&mut self, order_id: i64, current_timestamp: i64) -> Result<(), Error>;
    fn clear_inactive_orders(&mut self);
    fn position(&self) -> f64;
    fn state_values(&self) -> StateValues;
    fn depth(&self) -> &MD;
    fn orders(&self) -> &HashMap<i64, Order<Q>>;
    fn trade(&self) -> &Vec<Event>;
    fn clear_last_trades(&mut self);
}

pub trait Processor {
    fn initialize_data(&mut self) -> Result<i64, Error>;
    fn process_data(&mut self) -> Result<(i64, i64), Error>;
    fn process_recv_order(&mut self, timestamp: i64, wait_resp: i64) -> Result<i64, Error>;
    fn frontmost_recv_order_timestamp(&self) -> i64;
    fn frontmost_send_order_timestamp(&self) -> i64;
}
