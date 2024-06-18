use std::collections::BTreeMap;

use super::{ApplySnapshot, L2MarketDepth, MarketDepth, INVALID_MAX, INVALID_MIN};
use crate::{
    backtest::reader::Data,
    types::{Event, BUY, SELL},
};

/// L2 Market depth implementation based on a B-Tree map.
///
/// If feed data is missing, it may result in the crossing of the best bid and ask, making it
/// impossible to restore them to the most recent values through natural refreshing.
/// Ensuring data integrity is imperative.
#[derive(Debug)]
pub struct BTreeMarketDepth {
    pub tick_size: f32,
    pub lot_size: f32,
    pub timestamp: i64,
    pub bid_depth: BTreeMap<i32, f32>,
    pub ask_depth: BTreeMap<i32, f32>,
}

impl BTreeMarketDepth {
    /// Constructs an instance of `BTreeMarketDepth`.
    pub fn new(tick_size: f32, lot_size: f32) -> Self {
        Self {
            tick_size,
            lot_size,
            timestamp: 0,
            bid_depth: Default::default(),
            ask_depth: Default::default(),
        }
    }
}

impl L2MarketDepth for BTreeMarketDepth {
    fn update_bid_depth(
        &mut self,
        price: f32,
        qty: f32,
        timestamp: i64,
    ) -> (i32, i32, i32, f32, f32, i64) {
        let price_tick = (price / self.tick_size).round() as i32;
        let prev_best_bid_tick = *self.bid_depth.keys().last().unwrap_or(&INVALID_MIN);
        let prev_qty = *self.bid_depth.get(&prev_best_bid_tick).unwrap_or(&0.0);

        if (qty / self.lot_size).round() as i32 == 0 {
            self.bid_depth.remove(&price_tick);
        } else {
            *self.bid_depth.entry(price_tick).or_insert(qty) = qty;
        }
        let best_bid_tick = *self.bid_depth.keys().last().unwrap_or(&INVALID_MIN);
        (
            price_tick,
            prev_best_bid_tick,
            best_bid_tick,
            prev_qty,
            qty,
            timestamp,
        )
    }

    fn update_ask_depth(
        &mut self,
        price: f32,
        qty: f32,
        timestamp: i64,
    ) -> (i32, i32, i32, f32, f32, i64) {
        let price_tick = (price / self.tick_size).round() as i32;
        let prev_best_ask_tick = *self.bid_depth.keys().next().unwrap_or(&INVALID_MAX);
        let prev_qty = *self.ask_depth.get(&prev_best_ask_tick).unwrap_or(&0.0);

        if (qty / self.lot_size).round() as i32 == 0 {
            self.ask_depth.remove(&price_tick);
        } else {
            *self.ask_depth.entry(price_tick).or_insert(qty) = qty;
        }
        let best_ask_tick = *self.ask_depth.keys().next().unwrap_or(&INVALID_MAX);
        (
            price_tick,
            prev_best_ask_tick,
            best_ask_tick,
            prev_qty,
            qty,
            timestamp,
        )
    }

    fn clear_depth(&mut self, side: i64, clear_upto_price: f32) {
        let clear_upto = (clear_upto_price / self.tick_size).round() as i32;
        if side == BUY {
            let best_bid_tick = self.best_bid_tick();
            if best_bid_tick != INVALID_MIN {
                for t in clear_upto..(best_bid_tick + 1) {
                    if self.bid_depth.contains_key(&t) {
                        self.bid_depth.remove(&t);
                    }
                }
            }
        } else if side == SELL {
            let best_ask_tick = self.best_ask_tick();
            if best_ask_tick != INVALID_MAX {
                for t in best_ask_tick..(clear_upto + 1) {
                    if self.ask_depth.contains_key(&t) {
                        self.ask_depth.remove(&t);
                    }
                }
            }
        } else {
            self.bid_depth.clear();
            self.ask_depth.clear();
        }
    }
}

impl MarketDepth for BTreeMarketDepth {
    #[inline(always)]
    fn best_bid(&self) -> f32 {
        self.best_bid_tick() as f32 * self.tick_size
    }

    #[inline(always)]
    fn best_ask(&self) -> f32 {
        self.best_ask_tick() as f32 * self.tick_size
    }

    #[inline(always)]
    fn best_bid_tick(&self) -> i32 {
        *self.bid_depth.keys().last().unwrap_or(&INVALID_MIN)
    }

    #[inline(always)]
    fn best_ask_tick(&self) -> i32 {
        *self.ask_depth.keys().next().unwrap_or(&INVALID_MAX)
    }

    #[inline(always)]
    fn tick_size(&self) -> f32 {
        self.tick_size
    }

    #[inline(always)]
    fn lot_size(&self) -> f32 {
        self.lot_size
    }

    #[inline(always)]
    fn bid_qty_at_tick(&self, price_tick: i32) -> f32 {
        *self.bid_depth.get(&price_tick).unwrap_or(&0.0)
    }

    #[inline(always)]
    fn ask_qty_at_tick(&self, price_tick: i32) -> f32 {
        *self.ask_depth.get(&price_tick).unwrap_or(&0.0)
    }
}

impl ApplySnapshot<Event> for BTreeMarketDepth {
    fn apply_snapshot(&mut self, data: &Data<Event>) {
        self.bid_depth.clear();
        self.ask_depth.clear();
        for row_num in 0..data.len() {
            let price = data[row_num].px;
            let qty = data[row_num].qty;

            let price_tick = (price / self.tick_size).round() as i32;
            if data[row_num].ev & BUY == BUY {
                *self.bid_depth.entry(price_tick).or_insert(0f32) = qty;
            } else if data[row_num].ev & SELL == SELL {
                *self.ask_depth.entry(price_tick).or_insert(0f32) = qty;
            }
        }
    }
}
