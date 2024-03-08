use std::collections::HashMap;

use crate::{
    backtest::state::StateValues,
    ty::{OrdType, Order, Row, TimeInForce},
};

pub mod backtest;
pub mod connector;
pub mod depth;
mod error;
pub mod live;
pub mod ty;

pub trait Interface<Q, MD>
where
    Q: Sized + Clone,
{
    type Error;

    fn current_timestamp(&self) -> i64;

    fn position(&self, asset_no: usize) -> f64;

    fn state_values(&self, asset_no: usize) -> StateValues;

    fn depth(&self, asset_no: usize) -> &MD;

    fn trade(&self, asset_no: usize) -> &Vec<Row>;

    fn clear_last_trades(&mut self, asset_no: Option<usize>);

    fn orders(&self, asset_no: usize) -> &HashMap<i64, Order<Q>>;

    fn submit_buy_order(
        &mut self,
        asset_no: usize,
        order_id: i64,
        price: f32,
        qty: f32,
        time_in_force: TimeInForce,
        order_type: OrdType,
        wait: bool,
    ) -> Result<bool, Self::Error>;

    fn submit_sell_order(
        &mut self,
        asset_no: usize,
        order_id: i64,
        price: f32,
        qty: f32,
        time_in_force: TimeInForce,
        order_type: OrdType,
        wait: bool,
    ) -> Result<bool, Self::Error>;

    fn cancel(&mut self, asset_no: usize, order_id: i64, wait: bool) -> Result<bool, Self::Error>;

    fn clear_inactive_orders(&mut self, asset_no: Option<usize>);

    fn elapse(&mut self, duration: i64) -> Result<bool, Self::Error>;

    fn elapse_bt(&mut self, duration: i64) -> Result<bool, Self::Error>;

    fn close(&mut self) -> Result<(), Self::Error>;
}
