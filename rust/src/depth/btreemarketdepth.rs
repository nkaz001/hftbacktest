use std::collections::{BTreeMap, HashMap};
use std::collections::hash_map::Entry;

use super::{ApplySnapshot, L2MarketDepth, MarketDepth, INVALID_MAX, INVALID_MIN, L3Order, L3MarketDepth};
use crate::{
    backtest::reader::Data,
    types::{Event, BUY, SELL},
};
use crate::backtest::BacktestError;
use crate::prelude::Side;

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
    pub best_bid_tick: i32,
    pub best_ask_tick: i32,
    pub orders: HashMap<i64, L3Order>,
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
            best_bid_tick: INVALID_MIN,
            best_ask_tick: INVALID_MAX,
            orders: Default::default()
        }
    }

    #[cfg(feature = "unstable_l3")]
    fn add(&mut self, order: L3Order) -> Result<(), BacktestError> {
        if order.side == Side::Buy {
            *self.bid_depth.entry(order.price_tick).or_insert(0.0) += order.qty;
        } else {
            *self.ask_depth.entry(order.price_tick).or_insert(0.0) += order.qty;
        }
        match self.orders.entry(order.order_id) {
            Entry::Occupied(_) => Err(BacktestError::OrderIdExist),
            Entry::Vacant(entry) => {
                entry.insert(order);
                Ok(())
            }
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
        self.best_bid_tick = *self.bid_depth.keys().last().unwrap_or(&INVALID_MIN);
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
        let prev_best_ask_tick = *self.bid_depth.keys().next().unwrap_or(&INVALID_MAX);
        let prev_qty = *self.ask_depth.get(&prev_best_ask_tick).unwrap_or(&0.0);

        if (qty / self.lot_size).round() as i32 == 0 {
            self.ask_depth.remove(&price_tick);
        } else {
            *self.ask_depth.entry(price_tick).or_insert(qty) = qty;
        }
        self.best_ask_tick = *self.ask_depth.keys().next().unwrap_or(&INVALID_MAX);
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
            let best_bid_tick = self.best_bid_tick();
            if best_bid_tick != INVALID_MIN {
                for t in clear_upto..(best_bid_tick + 1) {
                    if self.bid_depth.contains_key(&t) {
                        self.bid_depth.remove(&t);
                    }
                }
            }
            self.best_bid_tick = *self.bid_depth.keys().last().unwrap_or(&INVALID_MIN);
        } else if side == SELL {
            let best_ask_tick = self.best_ask_tick();
            if best_ask_tick != INVALID_MAX {
                for t in best_ask_tick..(clear_upto + 1) {
                    if self.ask_depth.contains_key(&t) {
                        self.ask_depth.remove(&t);
                    }
                }
            }
            self.best_ask_tick = *self.ask_depth.keys().next().unwrap_or(&INVALID_MAX);
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
        self.best_bid_tick = *self.bid_depth.keys().last().unwrap_or(&INVALID_MIN);
        self.best_ask_tick = *self.ask_depth.keys().next().unwrap_or(&INVALID_MAX);
    }
}

#[cfg(feature = "unstable_l3")]
impl L3MarketDepth for BTreeMarketDepth {
    type Error = BacktestError;

    fn add_buy_order(
        &mut self,
        order_id: i64,
        px: f32,
        qty: f32,
        timestamp: i64,
    ) -> Result<(i32, i32), Self::Error> {
        let price_tick = (px / self.tick_size).round() as i32;
        self.add(L3Order {
            order_id,
            side: Side::Buy,
            price_tick,
            qty,
            timestamp,
        })?;
        let prev_best_tick = self.best_bid_tick;
        if price_tick > self.best_bid_tick {
            self.best_bid_tick = *self.bid_depth.keys().last().unwrap_or(&INVALID_MIN);
        }
        Ok((prev_best_tick, self.best_bid_tick))
    }

    fn add_sell_order(
        &mut self,
        order_id: i64,
        px: f32,
        qty: f32,
        timestamp: i64,
    ) -> Result<(i32, i32), Self::Error> {
        let price_tick = (px / self.tick_size).round() as i32;
        self.add(L3Order {
            order_id,
            side: Side::Sell,
            price_tick,
            qty,
            timestamp,
        })?;
        let prev_best_tick = self.best_ask_tick;
        if price_tick < self.best_ask_tick {
            self.best_ask_tick = *self.ask_depth.keys().next().unwrap_or(&INVALID_MAX);
        }
        Ok((prev_best_tick, self.best_ask_tick))
    }

    fn delete_order(
        &mut self,
        order_id: i64,
        _timestamp: i64,
    ) -> Result<(i64, i32, i32), Self::Error> {
        let order = self
            .orders
            .remove(&order_id)
            .ok_or(BacktestError::OrderNotFound)?;
        if order.side == Side::Buy {
            let prev_best_tick = self.best_bid_tick;

            let depth_qty = self.bid_depth.get_mut(&order.price_tick).unwrap();
            *depth_qty -= order.qty;
            if (*depth_qty / self.lot_size).round() as i32 == 0 {
                self.bid_depth.remove(&order.price_tick).unwrap();
                if order.price_tick == self.best_bid_tick {
                    self.best_bid_tick = *self.bid_depth.keys().next().unwrap_or(&INVALID_MIN);
                }
            }
            Ok((SELL, prev_best_tick, self.best_bid_tick))
        } else {
            let prev_best_tick = self.best_ask_tick;

            let depth_qty = self.ask_depth.get_mut(&order.price_tick).unwrap();
            *depth_qty -= order.qty;
            if (*depth_qty / self.lot_size).round() as i32 == 0 {
                self.ask_depth.remove(&order.price_tick).unwrap();
                if order.price_tick == self.best_ask_tick {
                    self.best_ask_tick = *self.ask_depth.keys().next().unwrap_or(&INVALID_MAX);
                }
            }
            Ok((SELL, prev_best_tick, self.best_ask_tick))
        }
    }

    fn modify_order(
        &mut self,
        order_id: i64,
        px: f32,
        qty: f32,
        timestamp: i64,
    ) -> Result<(i64, i32, i32), Self::Error> {
        let order = self
            .orders
            .get_mut(&order_id)
            .ok_or(BacktestError::OrderNotFound)?;
        if order.side == Side::Buy {
            let price_tick = (px / self.tick_size).round() as i32;
            if price_tick != order.price_tick {
                let depth_qty = self.bid_depth.get_mut(&order.price_tick).unwrap();
                *depth_qty -= order.qty;
                if (*depth_qty / self.lot_size).round() as i32 == 0 {
                    self.bid_depth.remove(&order.price_tick).unwrap();
                }

                order.price_tick = price_tick;
                order.qty = qty;
                order.timestamp = timestamp;

                *self.bid_depth.entry(order.price_tick).or_insert(0.0) += order.qty;

                let prev_best_tick = self.best_bid_tick;
                if price_tick > self.best_bid_tick {
                    self.best_bid_tick = *self.bid_depth.keys().last().unwrap_or(&INVALID_MIN);
                }
                Ok((BUY, prev_best_tick, self.best_bid_tick))
            } else {
                let depth_qty = self.bid_depth.get_mut(&order.price_tick).unwrap();
                *depth_qty += qty - order.qty;
                order.qty = qty;
                Ok((BUY, self.best_bid_tick, self.best_bid_tick))
            }
        } else {
            let price_tick = (px / self.tick_size).round() as i32;
            if price_tick != order.price_tick {
                let depth_qty = self.ask_depth.get_mut(&order.price_tick).unwrap();
                *depth_qty -= order.qty;
                if (*depth_qty / self.lot_size).round() as i32 == 0 {
                    self.bid_depth.remove(&order.price_tick).unwrap();
                }

                order.price_tick = price_tick;
                order.qty = qty;
                order.timestamp = timestamp;

                *self.ask_depth.entry(order.price_tick).or_insert(0.0) += order.qty;

                let prev_best_tick = self.best_ask_tick;
                if price_tick < self.best_ask_tick {
                    self.best_ask_tick = *self.ask_depth.keys().next().unwrap_or(&INVALID_MAX);
                }
                Ok((SELL, prev_best_tick, self.best_ask_tick))
            } else {
                let depth_qty = self.ask_depth.get_mut(&order.price_tick).unwrap();
                *depth_qty += qty - order.qty;
                order.qty = qty;
                Ok((SELL, self.best_ask_tick, self.best_ask_tick))
            }
        }
    }

    fn clear_depth(&mut self, side: i64) {
        if side == BUY {
            self.bid_depth.clear();
        } else if side == SELL {
            self.ask_depth.clear();
        } else {
            self.bid_depth.clear();
            self.ask_depth.clear();
        }
    }

    fn orders(&self) -> &HashMap<i64, L3Order> {
        &self.orders
    }
}