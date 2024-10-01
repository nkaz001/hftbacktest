use std::collections::HashMap;

use hftbacktest::{
    prelude::{Side, DEPTH_SNAPSHOT_EVENT, EXCH_EVENT, INVALID_MAX, INVALID_MIN, LOCAL_EVENT},
    types::{Event, BUY_EVENT, SELL_EVENT},
};

pub struct QtyTimestamp {
    qty: f64,
    ts: i64,
}

impl Default for QtyTimestamp {
    fn default() -> Self {
        Self { qty: 0.0, ts: 0 }
    }
}

pub struct FusedHashMapMarketDepth {
    pub tick_size: f64,
    pub ask_depth: HashMap<i64, QtyTimestamp>,
    pub bid_depth: HashMap<i64, QtyTimestamp>,
    pub best_bid_tick: i64,
    pub best_ask_tick: i64,
    pub best_bid_timestamp: i64,
    pub best_ask_timestamp: i64,
    pub low_bid_tick: i64,
    pub high_ask_tick: i64,
}

#[inline(always)]
fn depth_below(depth: &HashMap<i64, QtyTimestamp>, start: i64, end: i64) -> i64 {
    for t in (end..start).rev() {
        if depth.get(&t).map(|q| q.qty).unwrap_or(0f64) > 0f64 {
            return t;
        }
    }
    INVALID_MIN
}

#[inline(always)]
fn depth_above(depth: &HashMap<i64, QtyTimestamp>, start: i64, end: i64) -> i64 {
    for t in (start + 1)..(end + 1) {
        if depth.get(&t).map(|q| q.qty).unwrap_or(0f64) > 0f64 {
            return t;
        }
    }
    INVALID_MAX
}

impl FusedHashMapMarketDepth {
    /// Constructs an instance of `FusedHashMapMarketDepth`.
    pub fn new(tick_size: f64) -> Self {
        Self {
            tick_size,
            ask_depth: HashMap::new(),
            bid_depth: HashMap::new(),
            best_bid_tick: INVALID_MIN,
            best_ask_tick: INVALID_MAX,
            best_bid_timestamp: 0,
            best_ask_timestamp: 0,
            low_bid_tick: INVALID_MAX,
            high_ask_tick: INVALID_MIN,
        }
    }

    pub fn update_bid_depth(&mut self, price: f64, qty: f64, timestamp: i64) -> bool {
        let price_tick = (price / self.tick_size).round() as i64;
        let depth = self.bid_depth.entry(price_tick).or_default();
        if timestamp >= depth.ts {
            depth.qty = qty;
            depth.ts = timestamp;
        } else {
            return false;
        }

        if qty == 0.0 {
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
        true
    }

    pub fn update_ask_depth(&mut self, price: f64, qty: f64, timestamp: i64) -> bool {
        let price_tick = (price / self.tick_size).round() as i64;
        let depth = self.ask_depth.entry(price_tick).or_default();
        if timestamp >= depth.ts {
            depth.qty = qty;
            depth.ts = timestamp;
        } else {
            return false;
        }

        if qty == 0.0 {
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
        true
    }

    pub fn clear_depth(&mut self, side: Side, clear_upto_price: f64) {
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

    pub fn snapshot(&self) -> Vec<Event> {
        let mut events = Vec::new();

        let mut bid_depth = self
            .bid_depth
            .iter()
            .filter(|(&px_tick, _)| px_tick <= self.best_bid_tick)
            .map(|(&px_tick, depth)| (px_tick, depth))
            .collect::<Vec<_>>();
        bid_depth.sort_by(|a, b| b.0.cmp(&a.0));
        for (px_tick, qty) in bid_depth {
            events.push(Event {
                ev: EXCH_EVENT | LOCAL_EVENT | BUY_EVENT | DEPTH_SNAPSHOT_EVENT,
                exch_ts: qty.ts,
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
            .filter(|(&px_tick, _)| px_tick >= self.best_ask_tick)
            .map(|(&px_tick, depth)| (px_tick, depth))
            .collect::<Vec<_>>();
        ask_depth.sort_by(|a, b| a.0.cmp(&b.0));
        for (px_tick, qty) in ask_depth {
            events.push(Event {
                ev: EXCH_EVENT | LOCAL_EVENT | SELL_EVENT | DEPTH_SNAPSHOT_EVENT,
                exch_ts: qty.ts,
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

    pub fn update_best_bid(&mut self, px: f64, qty: f64, timestamp: i64) -> bool {
        let price_tick = (px / self.tick_size).round() as i64;
        let depth = self.bid_depth.entry(price_tick).or_default();
        if timestamp > depth.ts {
            depth.qty = qty;
            depth.ts = timestamp;
        } else {
            return false;
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
        true
    }

    pub fn update_best_ask(&mut self, px: f64, qty: f64, timestamp: i64) -> bool {
        let price_tick = (px / self.tick_size).round() as i64;
        let depth = self.ask_depth.entry(price_tick).or_default();
        if timestamp > depth.ts {
            depth.qty = qty;
            depth.ts = timestamp;
        } else {
            return false;
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
        true
    }
}

#[cfg(test)]
mod tests {
    use crate::fuse::FusedHashMapMarketDepth;

    #[test]
    fn test_update_bid_depth() {
        let mut depth = FusedHashMapMarketDepth::new(0.1);

        depth.update_bid_depth(10.1, 0.01, 1);
        depth.update_bid_depth(10.2, 0.02, 1);
        assert_eq!(depth.best_bid_tick, 102);
        depth.update_bid_depth(10.2, 0.03, 0);
        depth.update_bid_depth(10.3, 0.03, 0);
        assert_eq!(depth.best_bid_tick, 102);
        depth.update_bid_depth(10.3, 0.03, 2);
        assert_eq!(depth.best_bid_tick, 103);
        depth.update_bid_depth(10.3, 0.0, 1);
        assert_eq!(depth.best_bid_tick, 103);
        depth.update_bid_depth(10.3, 0.0, 2);
        assert_eq!(depth.best_bid_tick, 102);
    }

    #[test]
    fn test_update_ask_depth() {
        let mut depth = FusedHashMapMarketDepth::new(0.1);

        depth.update_ask_depth(10.2, 0.02, 1);
        depth.update_ask_depth(10.1, 0.01, 1);
        assert_eq!(depth.best_ask_tick, 101);
        depth.update_ask_depth(10.1, 0.03, 0);
        depth.update_ask_depth(10.0, 0.03, 0);
        assert_eq!(depth.best_ask_tick, 101);
        depth.update_ask_depth(10.0, 0.03, 2);
        assert_eq!(depth.best_ask_tick, 100);
        depth.update_ask_depth(10.0, 0.0, 1);
        assert_eq!(depth.best_ask_tick, 100);
        depth.update_ask_depth(10.0, 0.0, 2);
        assert_eq!(depth.best_ask_tick, 101);
    }

    #[test]
    fn test_update_bid_ask_depth_cross() {
        let mut depth = FusedHashMapMarketDepth::new(0.1);

        depth.update_bid_depth(10.1, 0.01, 1);
        depth.update_bid_depth(10.2, 0.02, 1);
        depth.update_ask_depth(10.3, 0.02, 1);
        depth.update_ask_depth(10.4, 0.01, 1);

        depth.update_ask_depth(10.2, 0.01, 3);
        assert_eq!(depth.best_bid_tick, 101);
        assert_eq!(depth.best_ask_tick, 102);

        depth.update_bid_depth(10.2, 0.03, 5);
        assert_eq!(depth.best_bid_tick, 102);
        assert_eq!(depth.best_ask_tick, 103);

        depth.update_ask_depth(10.2, 0.01, 4);
        assert_eq!(depth.best_bid_tick, 102);
        assert_eq!(depth.best_ask_tick, 103);
        depth.update_ask_depth(10.2, 0.0, 4);

        depth.update_ask_depth(10.3, 0.01, 7);
        depth.update_bid_depth(10.3, 0.01, 6);
        assert_eq!(depth.best_bid_tick, 102);
        assert_eq!(depth.best_ask_tick, 103);
    }

    #[test]
    fn test_update_best_bid() {
        let mut depth = FusedHashMapMarketDepth::new(0.1);

        depth.update_bid_depth(10.1, 0.01, 1);
        depth.update_bid_depth(10.2, 0.02, 1);
        depth.update_ask_depth(10.3, 0.02, 1);
        depth.update_ask_depth(10.4, 0.01, 1);

        depth.update_best_bid(10.3, 0.03, 0);
        assert_eq!(depth.best_bid_tick, 102);
        depth.update_best_bid(10.2, 0.03, 2);
        depth.update_best_bid(10.3, 0.01, 3);
        assert_eq!(depth.best_bid_tick, 103);
        assert_eq!(depth.best_ask_tick, 104);

        depth.update_bid_depth(10.1, 0.01, 5);
        depth.update_bid_depth(10.2, 0.02, 5);
        depth.update_best_bid(10.1, 0.05, 2);
        assert_eq!(depth.best_bid_tick, 103);
        assert_eq!(depth.best_ask_tick, 104);

        depth.update_best_bid(10.1, 0.05, 6);
        assert_eq!(depth.best_bid_tick, 101);
    }

    #[test]
    fn test_update_best_ask() {
        let mut depth = FusedHashMapMarketDepth::new(0.1);

        depth.update_bid_depth(10.1, 0.01, 1);
        depth.update_bid_depth(10.2, 0.02, 1);
        depth.update_ask_depth(10.3, 0.02, 1);
        depth.update_ask_depth(10.4, 0.01, 1);

        depth.update_best_ask(10.2, 0.03, 0);
        assert_eq!(depth.best_ask_tick, 103);
        depth.update_best_ask(10.3, 0.03, 2);
        depth.update_best_ask(10.2, 0.01, 3);
        assert_eq!(depth.best_bid_tick, 101);
        assert_eq!(depth.best_ask_tick, 102);

        depth.update_ask_depth(10.3, 0.02, 5);
        depth.update_ask_depth(10.4, 0.01, 5);
        depth.update_best_ask(10.4, 0.05, 2);
        assert_eq!(depth.best_bid_tick, 101);
        assert_eq!(depth.best_ask_tick, 102);

        depth.update_best_ask(10.4, 0.05, 6);
        assert_eq!(depth.best_ask_tick, 104);
    }
}
