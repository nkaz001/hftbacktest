use std::collections::{hash_map::Entry, HashMap};

use super::{ApplySnapshot, MarketDepth, INVALID_MAX, INVALID_MIN};
use crate::{
    backtest::reader::Data,
    prelude::L2MarketDepth,
    types::{Event, BUY, SELL},
};

/// L2 Market depth implementation based on a hash map.
///
/// This is considered more robust than a BTreeMap-based Market Depth. This is because in the
/// BTreeMap-based approach, missing depth feeds can lead to incorrect best bid or ask prices.
/// Specifically, when the best bid or ask is deleted, it may remain in the BTreeMap due to the
/// absence of corresponding depth feeds.
///
/// In contrast, a HashMap-based Market Depth tracks the latest best bid and ask prices, updating
/// them accordingly. This allows for natural refresh of market depth, even in cases where there are
/// missing feeds.
pub struct HashMapMarketDepth {
    pub tick_size: f32,
    pub lot_size: f32,
    pub timestamp: i64,
    pub ask_depth: HashMap<i32, f32>,
    pub bid_depth: HashMap<i32, f32>,
    pub best_bid_tick: i32,
    pub best_ask_tick: i32,
    pub low_bid_tick: i32,
    pub high_ask_tick: i32,
}

#[inline(always)]
fn depth_below(depth: &HashMap<i32, f32>, start: i32, end: i32) -> i32 {
    for t in (end..start).rev() {
        if *depth.get(&t).unwrap_or(&0f32) > 0f32 {
            return t;
        }
    }
    return INVALID_MIN;
}

#[inline(always)]
fn depth_above(depth: &HashMap<i32, f32>, start: i32, end: i32) -> i32 {
    for t in (start + 1)..(end + 1) {
        if *depth.get(&t).unwrap_or(&0f32) > 0f32 {
            return t;
        }
    }
    return INVALID_MAX;
}

impl HashMapMarketDepth {
    /// Constructs an instance of `HashMapMarketDepth`.
    pub fn new(tick_size: f32, lot_size: f32) -> Self {
        Self {
            tick_size,
            lot_size,
            timestamp: 0,
            ask_depth: HashMap::new(),
            bid_depth: HashMap::new(),
            best_bid_tick: INVALID_MIN,
            best_ask_tick: INVALID_MAX,
            low_bid_tick: INVALID_MAX,
            high_ask_tick: INVALID_MIN,
        }
    }
}

impl L2MarketDepth for HashMapMarketDepth {
    fn update_bid_depth(
        &mut self,
        price: f32,
        qty: f32,
        timestamp: i64,
    ) -> (i32, i32, i32, f32, f32, i64) {
        let price_tick = (price / self.tick_size).round() as i32;
        let qty_lot = (qty / self.lot_size).round() as i32;
        let prev_best_bid_tick = self.best_bid_tick;
        let prev_qty;
        match self.bid_depth.entry(price_tick) {
            Entry::Occupied(mut entry) => {
                prev_qty = *entry.get();
                if qty_lot > 0 {
                    *entry.get_mut() = qty;
                } else {
                    entry.remove();
                }
            }
            Entry::Vacant(entry) => {
                prev_qty = 0f32;
                if qty_lot > 0 {
                    entry.insert(qty);
                }
            }
        }

        if qty_lot == 0 {
            if price_tick == self.best_bid_tick {
                self.best_bid_tick =
                    depth_below(&self.bid_depth, self.best_bid_tick, self.low_bid_tick);
                if self.best_bid_tick == INVALID_MIN {
                    self.low_bid_tick = INVALID_MAX
                }
            }
        } else {
            if price_tick > self.best_bid_tick {
                self.best_bid_tick = price_tick;
                if self.best_bid_tick >= self.best_ask_tick {
                    self.best_ask_tick =
                        depth_above(&self.ask_depth, self.best_bid_tick, self.high_ask_tick);
                }
            }
            self.low_bid_tick = self.low_bid_tick.min(price_tick);
        }
        (
            price_tick,
            prev_best_bid_tick,
            self.best_bid_tick,
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
        let qty_lot = (qty / self.lot_size).round() as i32;
        let prev_best_ask_tick = self.best_ask_tick;
        let prev_qty;
        match self.ask_depth.entry(price_tick) {
            Entry::Occupied(mut entry) => {
                prev_qty = *entry.get();
                if qty_lot > 0 {
                    *entry.get_mut() = qty;
                } else {
                    entry.remove();
                }
            }
            Entry::Vacant(entry) => {
                prev_qty = 0f32;
                if qty_lot > 0 {
                    entry.insert(qty);
                }
            }
        }

        if qty_lot == 0 {
            if price_tick == self.best_ask_tick {
                self.best_ask_tick =
                    depth_above(&self.ask_depth, self.best_ask_tick, self.high_ask_tick);
                if self.best_ask_tick == INVALID_MAX {
                    self.high_ask_tick = INVALID_MIN
                }
            }
        } else {
            if price_tick < self.best_ask_tick {
                self.best_ask_tick = price_tick;
                if self.best_bid_tick >= self.best_ask_tick {
                    self.best_bid_tick =
                        depth_below(&self.bid_depth, self.best_ask_tick, self.low_bid_tick);
                }
            }
            self.high_ask_tick = self.high_ask_tick.max(price_tick);
        }
        (
            price_tick,
            prev_best_ask_tick,
            self.best_ask_tick,
            prev_qty,
            qty,
            timestamp,
        )
    }

    fn clear_depth(&mut self, side: i64, clear_upto_price: f32) {
        let clear_upto = (clear_upto_price / self.tick_size).round() as i32;
        if side == BUY {
            if self.best_bid_tick != INVALID_MIN {
                for t in clear_upto..(self.best_bid_tick + 1) {
                    if self.bid_depth.contains_key(&t) {
                        self.bid_depth.remove(&t);
                    }
                }
            }
            self.best_bid_tick = depth_below(&self.bid_depth, clear_upto - 1, self.low_bid_tick);
            if self.best_bid_tick == INVALID_MIN {
                self.low_bid_tick = INVALID_MAX;
            }
        } else if side == SELL {
            if self.best_ask_tick != INVALID_MAX {
                for t in self.best_ask_tick..(clear_upto + 1) {
                    if self.ask_depth.contains_key(&t) {
                        self.ask_depth.remove(&t);
                    }
                }
            }
            self.best_ask_tick = depth_above(&self.ask_depth, clear_upto + 1, self.high_ask_tick);
            if self.best_ask_tick == INVALID_MAX {
                self.high_ask_tick = INVALID_MIN;
            }
        } else {
            self.bid_depth.clear();
            self.ask_depth.clear();
            self.best_bid_tick = INVALID_MIN;
            self.best_ask_tick = INVALID_MAX;
            self.low_bid_tick = INVALID_MAX;
            self.high_ask_tick = INVALID_MIN;
        }
    }
}

impl MarketDepth for HashMapMarketDepth {
    #[inline(always)]
    fn best_bid(&self) -> f32 {
        self.best_bid_tick as f32 * self.tick_size
    }

    #[inline(always)]
    fn best_ask(&self) -> f32 {
        self.best_ask_tick as f32 * self.tick_size
    }

    #[inline(always)]
    fn best_bid_tick(&self) -> i32 {
        self.best_bid_tick
    }

    #[inline(always)]
    fn best_ask_tick(&self) -> i32 {
        self.best_ask_tick
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

impl ApplySnapshot<Event> for HashMapMarketDepth {
    fn apply_snapshot(&mut self, data: &Data<Event>) {
        self.best_bid_tick = INVALID_MIN;
        self.best_ask_tick = INVALID_MAX;
        self.low_bid_tick = INVALID_MAX;
        self.high_ask_tick = INVALID_MIN;
        self.bid_depth.clear();
        self.ask_depth.clear();
        for row_num in 0..data.len() {
            let price = data[row_num].px;
            let qty = data[row_num].qty;

            let price_tick = (price / self.tick_size).round() as i32;
            if data[row_num].ev & BUY == BUY {
                self.best_bid_tick = self.best_bid_tick.max(price_tick);
                self.low_bid_tick = self.low_bid_tick.min(price_tick);
                *self.bid_depth.entry(price_tick).or_insert(0f32) = qty;
            } else if data[row_num].ev & SELL == SELL {
                self.best_ask_tick = self.best_ask_tick.min(price_tick);
                self.high_ask_tick = self.high_ask_tick.max(price_tick);
                *self.ask_depth.entry(price_tick).or_insert(0f32) = qty;
            }
        }
    }
}
