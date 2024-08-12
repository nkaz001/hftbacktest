use std::collections::{hash_map::Entry, HashMap};

use super::{ApplySnapshot, L1MarketDepth, L3Order, MarketDepth, INVALID_MAX, INVALID_MIN};
use crate::{
    backtest::{data::Data, BacktestError},
    prelude::{L2MarketDepth, Side, DEPTH_SNAPSHOT_EVENT, EXCH_EVENT, LOCAL_EVENT},
    types::{Event, OrderId, BUY_EVENT, SELL_EVENT},
};

struct QtyTimestamp {
    qty: f64,
    timestamp: i64,
}

impl Default for QtyTimestamp {
    fn default() -> Self {
        Self {
            qty: 0.0,
            timestamp: 0,
        }
    }
}

/// L2/L3 Market depth implementation based on a hash map.
///
/// This is considered more robust than a BTreeMap-based Market Depth when it comes to L2 feed.
/// This is because in the BTreeMap-based approach, missing depth feeds can lead to incorrect best
/// bid or ask prices.
/// Specifically, when the best bid or ask is deleted, it may remain in the BTreeMap due to the
/// absence of corresponding depth feeds.
///
/// In contrast, a HashMap-based Market Depth tracks the latest best bid and ask prices, updating
/// them accordingly. This allows for natural refresh of market depth, even in cases where there are
/// missing feeds.
pub struct FusedHashMapMarketDepth {
    pub tick_size: f64,
    pub lot_size: f64,
    pub timestamp: i64,
    pub ask_depth: HashMap<i64, QtyTimestamp>,
    pub bid_depth: HashMap<i64, QtyTimestamp>,
    pub best_bid_tick: i64,
    pub best_ask_tick: i64,
    pub best_bid_timestamp: i64,
    pub best_ask_timestamp: i64,
    pub low_bid_tick: i64,
    pub high_ask_tick: i64,
    pub orders: HashMap<OrderId, L3Order>,
}

#[inline(always)]
fn depth_below(depth: &HashMap<i64, QtyTimestamp>, start: i64, end: i64) -> i64 {
    for t in (end..start).rev() {
        if depth.get(&t).unwrap_or(&Default::default()).qty > 0f64 {
            return t;
        }
    }
    INVALID_MIN
}

#[inline(always)]
fn depth_above(depth: &HashMap<i64, QtyTimestamp>, start: i64, end: i64) -> i64 {
    for t in (start + 1)..(end + 1) {
        if depth.get(&t).unwrap_or(&Default::default()).qty > 0f64 {
            return t;
        }
    }
    INVALID_MAX
}

impl FusedHashMapMarketDepth {
    /// Constructs an instance of `HashMapMarketDepth`.
    pub fn new(tick_size: f64, lot_size: f64) -> Self {
        Self {
            tick_size,
            lot_size,
            timestamp: 0,
            ask_depth: HashMap::new(),
            bid_depth: HashMap::new(),
            best_bid_tick: INVALID_MIN,
            best_ask_tick: INVALID_MAX,
            best_bid_timestamp: 0,
            best_ask_timestamp: 0,
            low_bid_tick: INVALID_MAX,
            high_ask_tick: INVALID_MIN,
            orders: HashMap::new(),
        }
    }

    fn add(&mut self, order: L3Order) -> Result<(), BacktestError> {
        let order = match self.orders.entry(order.order_id) {
            Entry::Occupied(_) => return Err(BacktestError::OrderIdExist),
            Entry::Vacant(entry) => entry.insert(order),
        };
        if order.side == Side::Buy {
            self.bid_depth
                .entry(order.price_tick)
                .or_insert(Default::default())
                .qty += order.qty;
        } else {
            self.ask_depth
                .entry(order.price_tick)
                .or_insert(Default::default())
                .qty += order.qty;
        }
        Ok(())
    }
}

impl L2MarketDepth for FusedHashMapMarketDepth {
    fn update_bid_depth(
        &mut self,
        price: f64,
        qty: f64,
        timestamp: i64,
    ) -> (i64, i64, i64, f64, f64, i64) {
        let price_tick = (price / self.tick_size).round() as i64;
        let qty_lot = (qty / self.lot_size).round() as i64;
        let prev_best_bid_tick = self.best_bid_tick;
        let prev_qty;
        match self.bid_depth.entry(price_tick) {
            Entry::Occupied(mut entry) => {
                let QtyTimestamp {
                    qty: prev_qty_,
                    timestamp: prev_timestamp,
                } = *entry.get();
                prev_qty = prev_qty_;
                if timestamp > prev_timestamp {
                    if qty_lot > 0 {
                        *entry.get_mut() = QtyTimestamp { qty, timestamp };
                    } else {
                        entry.remove();
                    }
                }
            }
            Entry::Vacant(entry) => {
                prev_qty = 0f64;
                if qty_lot > 0 {
                    entry.insert(QtyTimestamp { qty, timestamp });
                }
            }
        }

        if qty_lot == 0 {
            if price_tick == self.best_bid_tick && timestamp >= self.best_bid_timestamp {
                self.best_bid_tick =
                    depth_below(&self.bid_depth, self.best_bid_tick, self.low_bid_tick);
                self.best_bid_timestamp = timestamp;
                if self.best_bid_tick == INVALID_MIN {
                    self.low_bid_tick = INVALID_MAX
                }
            }
        } else {
            if price_tick >= self.best_bid_tick && timestamp >= self.best_bid_timestamp {
                self.best_bid_tick = price_tick;
                self.best_bid_timestamp = timestamp;
                if self.best_bid_tick >= self.best_ask_tick {
                    if timestamp >= self.best_ask_timestamp {
                        self.best_ask_tick =
                            depth_above(&self.ask_depth, self.best_bid_tick, self.high_ask_tick);
                        self.best_ask_timestamp = timestamp;
                    } else {
                        self.best_bid_tick =
                            depth_below(&self.bid_depth, self.best_ask_tick, self.low_bid_tick);
                        self.best_bid_timestamp = self.best_ask_timestamp;
                    }
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
        price: f64,
        qty: f64,
        timestamp: i64,
    ) -> (i64, i64, i64, f64, f64, i64) {
        let price_tick = (price / self.tick_size).round() as i64;
        let qty_lot = (qty / self.lot_size).round() as i64;
        let prev_best_ask_tick = self.best_ask_tick;
        let prev_qty;
        match self.ask_depth.entry(price_tick) {
            Entry::Occupied(mut entry) => {
                let QtyTimestamp {
                    qty: prev_qty_,
                    timestamp: prev_timestamp,
                } = *entry.get();
                prev_qty = prev_qty_;
                if timestamp > prev_timestamp {
                    if qty_lot > 0 {
                        *entry.get_mut() = QtyTimestamp { qty, timestamp };
                    } else {
                        entry.remove();
                    }
                }
            }
            Entry::Vacant(entry) => {
                prev_qty = 0f64;
                if qty_lot > 0 {
                    entry.insert(QtyTimestamp { qty, timestamp });
                }
            }
        }

        if qty_lot == 0 {
            if price_tick == self.best_ask_tick && timestamp >= self.best_ask_timestamp {
                self.best_ask_tick =
                    depth_above(&self.ask_depth, self.best_ask_tick, self.high_ask_tick);
                self.best_ask_timestamp = timestamp;
                if self.best_ask_tick == INVALID_MAX {
                    self.high_ask_tick = INVALID_MIN
                }
            }
        } else {
            if price_tick <= self.best_ask_tick && timestamp >= self.best_ask_timestamp {
                self.best_ask_tick = price_tick;
                self.best_ask_timestamp = timestamp;
                if self.best_bid_tick >= self.best_ask_tick {
                    if timestamp >= self.best_bid_timestamp {
                        self.best_bid_tick =
                            depth_below(&self.bid_depth, self.best_ask_tick, self.low_bid_tick);
                        self.best_bid_timestamp = timestamp;
                    } else {
                        self.best_ask_tick =
                            depth_above(&self.ask_depth, self.best_bid_tick, self.high_ask_tick);
                        self.best_ask_timestamp = self.best_bid_timestamp;
                    }
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

    fn clear_depth(&mut self, side: Side, clear_upto_price: f64) {
        let clear_upto = (clear_upto_price / self.tick_size).round() as i64;
        if side == Side::Buy {
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
        } else if side == Side::Sell {
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

impl MarketDepth for FusedHashMapMarketDepth {
    #[inline(always)]
    fn best_bid(&self) -> f64 {
        if self.best_bid_tick == INVALID_MIN {
            f64::NAN
        } else {
            self.best_bid_tick as f64 * self.tick_size
        }
    }

    #[inline(always)]
    fn best_ask(&self) -> f64 {
        if self.best_ask_tick == INVALID_MAX {
            f64::NAN
        } else {
            self.best_ask_tick as f64 * self.tick_size
        }
    }

    #[inline(always)]
    fn best_bid_tick(&self) -> i64 {
        self.best_bid_tick
    }

    #[inline(always)]
    fn best_ask_tick(&self) -> i64 {
        self.best_ask_tick
    }

    #[inline(always)]
    fn tick_size(&self) -> f64 {
        self.tick_size
    }

    #[inline(always)]
    fn lot_size(&self) -> f64 {
        self.lot_size
    }

    #[inline(always)]
    fn bid_qty_at_tick(&self, price_tick: i64) -> f64 {
        self.bid_depth
            .get(&price_tick)
            .unwrap_or(&Default::default())
            .qty
    }

    #[inline(always)]
    fn ask_qty_at_tick(&self, price_tick: i64) -> f64 {
        self.ask_depth
            .get(&price_tick)
            .unwrap_or(&Default::default())
            .qty
    }
}

impl ApplySnapshot for FusedHashMapMarketDepth {
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
            let timestamp = data[row_num].exch_ts;

            let price_tick = (price / self.tick_size).round() as i64;
            if data[row_num].ev & BUY_EVENT == BUY_EVENT {
                self.best_bid_tick = self.best_bid_tick.max(price_tick);
                self.low_bid_tick = self.low_bid_tick.min(price_tick);
                *self
                    .bid_depth
                    .entry(price_tick)
                    .or_insert(Default::default()) = QtyTimestamp { qty, timestamp };
            } else if data[row_num].ev & SELL_EVENT == SELL_EVENT {
                self.best_ask_tick = self.best_ask_tick.min(price_tick);
                self.high_ask_tick = self.high_ask_tick.max(price_tick);
                *self
                    .ask_depth
                    .entry(price_tick)
                    .or_insert(Default::default()) = QtyTimestamp { qty, timestamp };
            }
        }
    }

    fn snapshot(&self) -> Vec<Event> {
        let mut events = Vec::new();

        let mut bid_depth = self
            .bid_depth
            .iter()
            .map(|(&px_tick, qty)| (px_tick, qty))
            .collect::<Vec<_>>();
        bid_depth.sort_by(|a, b| b.0.cmp(&a.0));
        for (px_tick, qty) in bid_depth {
            events.push(Event {
                ev: EXCH_EVENT | LOCAL_EVENT | BUY_EVENT | DEPTH_SNAPSHOT_EVENT,
                exch_ts: qty.timestamp,
                // todo: it's not a problem now, but it would be better to have valid timestamps.
                local_ts: 0,
                px: px_tick as f64 * self.tick_size,
                qty: qty.qty,
                order_id: 0,
                ival: 0,
                fval: 0.0,
            });
        }

        let mut ask_depth = self
            .ask_depth
            .iter()
            .map(|(&px_tick, qty)| (px_tick, qty))
            .collect::<Vec<_>>();
        ask_depth.sort_by(|a, b| a.0.cmp(&b.0));
        for (px_tick, qty) in ask_depth {
            events.push(Event {
                ev: EXCH_EVENT | LOCAL_EVENT | SELL_EVENT | DEPTH_SNAPSHOT_EVENT,
                exch_ts: qty.timestamp,
                // todo: it's not a problem now, but it would be better to have valid timestamps.
                local_ts: 0,
                px: px_tick as f64 * self.tick_size,
                qty: qty.qty,
                order_id: 0,
                ival: 0,
                fval: 0.0,
            });
        }

        events
    }
}

impl L1MarketDepth for FusedHashMapMarketDepth {
    fn update_best_bid(
        &mut self,
        px: f64,
        qty: f64,
        timestamp: i64,
    ) -> (i64, i64, i64, f64, f64, i64) {
        let price_tick = (px / self.tick_size).round() as i64;
        let prev_best_bid_tick = self.best_bid_tick;
        let prev_qty;
        match self.bid_depth.entry(price_tick) {
            Entry::Occupied(mut entry) => {
                let QtyTimestamp {
                    qty: prev_qty_,
                    timestamp: prev_timestamp,
                } = *entry.get();
                prev_qty = prev_qty_;
                if timestamp > prev_timestamp {
                    *entry.get_mut() = QtyTimestamp { qty, timestamp };
                }
            }
            Entry::Vacant(entry) => {
                prev_qty = 0f64;
                entry.insert(QtyTimestamp { qty, timestamp });
            }
        }

        if timestamp >= self.best_bid_timestamp {
            self.best_bid_tick = price_tick;
            self.best_bid_timestamp = timestamp;
            if self.best_bid_tick >= self.best_ask_tick {
                if timestamp >= self.best_ask_timestamp {
                    self.best_ask_tick =
                        depth_above(&self.ask_depth, self.best_bid_tick, self.high_ask_tick);
                    self.best_ask_timestamp = timestamp;
                } else {
                    self.best_bid_tick =
                        depth_below(&self.bid_depth, self.best_ask_tick, self.low_bid_tick);
                    self.best_bid_timestamp = self.best_ask_timestamp;
                }
            }
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

    fn update_best_ask(
        &mut self,
        px: f64,
        qty: f64,
        timestamp: i64,
    ) -> (i64, i64, i64, f64, f64, i64) {
        let price_tick = (px / self.tick_size).round() as i64;
        let qty_lot = (qty / self.lot_size).round() as i64;
        let prev_best_ask_tick = self.best_ask_tick;
        let prev_qty;
        match self.ask_depth.entry(price_tick) {
            Entry::Occupied(mut entry) => {
                let QtyTimestamp {
                    qty: prev_qty_,
                    timestamp: prev_timestamp,
                } = *entry.get();
                prev_qty = prev_qty_;
                if timestamp > prev_timestamp {
                    *entry.get_mut() = QtyTimestamp { qty, timestamp };
                }
            }
            Entry::Vacant(entry) => {
                prev_qty = 0f64;
                entry.insert(QtyTimestamp { qty, timestamp });
            }
        }

        if timestamp >= self.best_ask_timestamp {
            self.best_ask_tick = price_tick;
            self.best_ask_timestamp = timestamp;
            if self.best_bid_tick >= self.best_ask_tick {
                if timestamp >= self.best_bid_timestamp {
                    self.best_bid_tick =
                        depth_below(&self.bid_depth, self.best_ask_tick, self.low_bid_tick);
                    self.best_bid_timestamp = timestamp;
                } else {
                    self.best_ask_tick =
                        depth_above(&self.ask_depth, self.best_bid_tick, self.high_ask_tick);
                    self.best_ask_timestamp = self.best_bid_timestamp;
                }
            }
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
}

#[cfg(test)]
mod tests {
    use crate::depth::{fuse::FusedHashMapMarketDepth, L1MarketDepth, L2MarketDepth, MarketDepth};

    #[test]
    fn test_update_bid_depth() {
        let mut depth = FusedHashMapMarketDepth::new(0.1, 0.01);

        depth.update_bid_depth(10.1, 0.01, 1);
        depth.update_bid_depth(10.2, 0.02, 1);
        assert_eq!(depth.best_bid_tick(), 102);
        depth.update_bid_depth(10.2, 0.03, 0);
        assert_eq!(depth.bid_qty_at_tick(102), 0.02);
        depth.update_bid_depth(10.3, 0.03, 0);
        assert_eq!(depth.best_bid_tick(), 102);
        depth.update_bid_depth(10.3, 0.03, 2);
        assert_eq!(depth.best_bid_tick(), 103);
        depth.update_bid_depth(10.3, 0.0, 1);
        assert_eq!(depth.best_bid_tick(), 103);
        assert_eq!(depth.bid_qty_at_tick(103), 0.03);
        depth.update_bid_depth(10.3, 0.0, 2);
        assert_eq!(depth.best_bid_tick(), 102);
    }

    #[test]
    fn test_update_ask_depth() {
        let mut depth = FusedHashMapMarketDepth::new(0.1, 0.01);

        depth.update_ask_depth(10.2, 0.02, 1);
        depth.update_ask_depth(10.1, 0.01, 1);
        assert_eq!(depth.best_ask_tick(), 101);
        depth.update_ask_depth(10.1, 0.03, 0);
        assert_eq!(depth.ask_qty_at_tick(101), 0.01);
        depth.update_ask_depth(10.0, 0.03, 0);
        assert_eq!(depth.best_ask_tick(), 101);
        depth.update_ask_depth(10.0, 0.03, 2);
        assert_eq!(depth.best_ask_tick(), 100);
        depth.update_ask_depth(10.0, 0.0, 1);
        assert_eq!(depth.best_ask_tick(), 100);
        assert_eq!(depth.ask_qty_at_tick(100), 0.03);
        depth.update_ask_depth(10.0, 0.0, 2);
        assert_eq!(depth.best_ask_tick(), 101);
    }

    #[test]
    fn test_update_bid_ask_depth_cross() {
        let mut depth = FusedHashMapMarketDepth::new(0.1, 0.01);

        depth.update_bid_depth(10.1, 0.01, 1);
        depth.update_bid_depth(10.2, 0.02, 1);
        depth.update_ask_depth(10.3, 0.02, 1);
        depth.update_ask_depth(10.4, 0.01, 1);

        depth.update_ask_depth(10.2, 0.01, 3);
        assert_eq!(depth.best_bid_tick(), 101);
        assert_eq!(depth.best_ask_tick(), 102);

        depth.update_bid_depth(10.2, 0.03, 5);
        assert_eq!(depth.best_bid_tick(), 102);
        assert_eq!(depth.best_ask_tick(), 103);

        depth.update_ask_depth(10.2, 0.01, 4);
        assert_eq!(depth.best_bid_tick(), 102);
        assert_eq!(depth.best_ask_tick(), 103);
        depth.update_ask_depth(10.2, 0.0, 4);

        depth.update_ask_depth(10.3, 0.01, 7);
        depth.update_bid_depth(10.3, 0.01, 6);
        assert_eq!(depth.best_bid_tick(), 102);
        assert_eq!(depth.best_ask_tick(), 103);
    }

    #[test]
    fn test_update_best_bid() {
        let mut depth = FusedHashMapMarketDepth::new(0.1, 0.01);

        depth.update_bid_depth(10.1, 0.01, 1);
        depth.update_bid_depth(10.2, 0.02, 1);
        depth.update_ask_depth(10.3, 0.02, 1);
        depth.update_ask_depth(10.4, 0.01, 1);

        depth.update_best_bid(10.3, 0.03, 0);
        assert_eq!(depth.best_bid_tick(), 102);
        depth.update_best_bid(10.2, 0.03, 2);
        assert_eq!(depth.bid_qty_at_tick(102), 0.03);
        depth.update_best_bid(10.3, 0.01, 3);
        assert_eq!(depth.best_bid_tick(), 103);
        assert_eq!(depth.bid_qty_at_tick(103), 0.01);
        assert_eq!(depth.best_ask_tick(), 104);

        depth.update_bid_depth(10.1, 0.01, 5);
        depth.update_bid_depth(10.2, 0.02, 5);
        depth.update_best_bid(10.1, 0.05, 2);
        assert_eq!(depth.best_bid_tick(), 103);
        assert_eq!(depth.best_ask_tick(), 104);
        assert_eq!(depth.bid_qty_at_tick(101), 0.01);

        depth.update_best_bid(10.1, 0.05, 6);
        assert_eq!(depth.best_bid_tick(), 101);
    }

    #[test]
    fn test_update_best_ask() {
        let mut depth = FusedHashMapMarketDepth::new(0.1, 0.01);

        depth.update_bid_depth(10.1, 0.01, 1);
        depth.update_bid_depth(10.2, 0.02, 1);
        depth.update_ask_depth(10.3, 0.02, 1);
        depth.update_ask_depth(10.4, 0.01, 1);

        depth.update_best_ask(10.2, 0.03, 0);
        assert_eq!(depth.best_ask_tick(), 103);
        depth.update_best_ask(10.3, 0.03, 2);
        assert_eq!(depth.ask_qty_at_tick(103), 0.03);
        depth.update_best_ask(10.2, 0.01, 3);
        assert_eq!(depth.best_bid_tick(), 101);
        assert_eq!(depth.ask_qty_at_tick(102), 0.01);
        assert_eq!(depth.best_ask_tick(), 102);

        depth.update_ask_depth(10.3, 0.02, 5);
        depth.update_ask_depth(10.4, 0.01, 5);
        depth.update_best_ask(10.4, 0.05, 2);
        assert_eq!(depth.best_bid_tick(), 101);
        assert_eq!(depth.best_ask_tick(), 102);
        assert_eq!(depth.ask_qty_at_tick(104), 0.01);

        depth.update_best_ask(10.4, 0.05, 6);
        assert_eq!(depth.best_ask_tick(), 104);
    }
}
