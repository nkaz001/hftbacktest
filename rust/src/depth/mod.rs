use crate::{backtest::reader::Data, ty::Row};

pub mod btreemarketdepth;
pub mod hashmapmarketdepth;

pub const INVALID_MIN: i32 = i32::MIN;
pub const INVALID_MAX: i32 = i32::MAX;

pub trait MarketDepth {
    fn update_bid_depth(
        &mut self,
        price: f32,
        qty: f32,
        timestamp: i64,
    ) -> (i32, i32, i32, f32, f32, i64);

    fn update_ask_depth(
        &mut self,
        price: f32,
        qty: f32,
        timestamp: i64,
    ) -> (i32, i32, i32, f32, f32, i64);

    fn clear_depth(&mut self, side: i64, clear_upto_price: f32);

    fn best_bid(&self) -> f32;

    fn best_ask(&self) -> f32;

    fn best_bid_tick(&self) -> i32;

    fn best_ask_tick(&self) -> i32;

    fn tick_size(&self) -> f32;

    fn lot_size(&self) -> f32;
}

pub trait ApplySnapshot {
    fn apply_snapshot(&mut self, data: &Data<Row>);
}
