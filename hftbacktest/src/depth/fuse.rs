use std::collections::{HashMap, hash_map::Entry};

//use tracing::debug;

use crate::{
    backtest::data::Data,
    depth::{ApplySnapshot, INVALID_MAX, INVALID_MIN, MarketDepth},
    prelude::{DEPTH_SNAPSHOT_EVENT, EXCH_EVENT, LOCAL_EVENT},
    types::{BUY_EVENT, DEPTH_EVENT, Event, SELL_EVENT, Side},
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
}

#[inline(always)]
fn depth_below(depth: &HashMap<i64, QtyTimestamp>, start: i64, end: i64) -> i64 {
    for t in (end..start).rev() {
        if depth.get(&t).map(|value| value.qty).unwrap_or(0.0) > 0f64 {
            return t;
        }
    }
    INVALID_MIN
}

#[inline(always)]
fn depth_above(depth: &HashMap<i64, QtyTimestamp>, start: i64, end: i64) -> i64 {
    for t in (start + 1)..(end + 1) {
        if depth.get(&t).map(|value| value.qty).unwrap_or(0.0) > 0f64 {
            return t;
        }
    }
    INVALID_MAX
}

impl FusedHashMapMarketDepth {
    /// Constructs an instance of `FusedHashMapMarketDepth`.
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
        }
    }

    pub fn update_bid_depth(&mut self, ev: Event) -> Vec<Event> {
        let mut result = Vec::new();

        let price_tick = (ev.px / self.tick_size).round() as i64;
        let qty_lot = (ev.qty / self.lot_size).round() as i64;

        if (price_tick >= self.best_bid_tick && ev.exch_ts < self.best_bid_timestamp)
            || (price_tick >= self.best_ask_tick && ev.exch_ts < self.best_ask_timestamp)
        {
            // debug!(
            //     ?ev,
            //     best_bid_tick = self.best_bid_tick,
            //     best_ask_tick = self.best_ask_tick,
            //     best_bid_timestamp = self.best_bid_timestamp,
            //     best_ask_timestamp = self.best_ask_timestamp,
            //     "update_bid_depth: attempts to update the BBO, but the event is outdated."
            // );
            return result;
        }

        match self.bid_depth.entry(price_tick) {
            Entry::Occupied(mut entry) => {
                let QtyTimestamp { qty: _, ts } = *entry.get();
                if ev.exch_ts >= ts {
                    if qty_lot > 0 {
                        *entry.get_mut() = QtyTimestamp {
                            qty: ev.qty,
                            ts: ev.exch_ts,
                        };
                    } else {
                        entry.remove();
                    }
                    result.push(ev.clone());
                } else {
                    // debug!(
                    //     ?ev,
                    //     ?ts,
                    //     "update_bid_depth: attempts to update the price level, \
                    //     but the event is outdated."
                    // );
                }
            }
            Entry::Vacant(entry) => {
                if qty_lot > 0 {
                    entry.insert(QtyTimestamp {
                        qty: ev.qty,
                        ts: ev.exch_ts,
                    });
                    result.push(ev.clone());
                }
            }
        }

        if qty_lot == 0 {
            if price_tick == self.best_bid_tick {
                self.best_bid_tick =
                    depth_below(&self.bid_depth, self.best_bid_tick, self.low_bid_tick);
                self.best_bid_timestamp = ev.exch_ts;
                if self.best_bid_tick == INVALID_MIN {
                    self.low_bid_tick = INVALID_MAX
                }
                // debug!(
                //     ?ev,
                //     best_bid_tick = self.best_bid_tick,
                //     best_ask_tick = self.best_ask_tick,
                //     "update_bid_depth: The BBO gets updated by deleting the prior BBO."
                // );
            }
        } else {
            if price_tick >= self.best_bid_tick {
                self.best_bid_tick = price_tick;
                self.best_bid_timestamp = ev.exch_ts;

                if price_tick >= self.best_ask_tick {
                    let prev_best_ask_tick = self.best_ask_tick;
                    self.best_ask_tick =
                        depth_above(&self.ask_depth, self.best_bid_tick, self.high_ask_tick);
                    self.best_ask_timestamp = ev.exch_ts;

                    let mut up_to_ask_tick = self.best_ask_tick;
                    if up_to_ask_tick == INVALID_MAX {
                        up_to_ask_tick = self.best_bid_tick + 1;
                        self.high_ask_tick = INVALID_MIN;
                    }

                    for t in prev_best_ask_tick..up_to_ask_tick {
                        if self.ask_depth.remove(&t).is_some() {
                            // debug!(
                            //     px = t as f64 * self.tick_size,
                            //     "update_bid_depth: Ask deletion event is generated \
                            //     by the best bid crossing."
                            // );
                            result.push(Event {
                                ev: SELL_EVENT | DEPTH_EVENT,
                                exch_ts: ev.exch_ts,
                                local_ts: ev.local_ts,
                                px: t as f64 * self.tick_size,
                                qty: 0.0,
                                order_id: 0,
                                ival: 0,
                                fval: 0.0,
                            });
                        }
                    }
                }
                // debug!(
                //     best_bid_tick = self.best_bid_tick,
                //     best_ask_tick = self.best_ask_tick,
                //     "update_bid_depth: The BBO gets updated."
                // );
            }
            self.low_bid_tick = self.low_bid_tick.min(price_tick);
        }
        result
    }

    pub fn update_ask_depth(&mut self, ev: Event) -> Vec<Event> {
        let mut result = Vec::new();

        let price_tick = (ev.px / self.tick_size).round() as i64;
        let qty_lot = (ev.qty / self.lot_size).round() as i64;

        if (price_tick <= self.best_ask_tick && ev.exch_ts < self.best_ask_timestamp)
            || (price_tick <= self.best_bid_tick && ev.exch_ts < self.best_bid_timestamp)
        {
            // debug!(
            //     price_tick,
            //     best_bid_tick = self.best_bid_tick,
            //     best_ask_tick = self.best_ask_tick,
            //     best_bid_timestamp = self.best_bid_timestamp,
            //     best_ask_timestamp = self.best_ask_timestamp,
            //     "update_ask_depth: attempts to update the BBO, but the event is outdated."
            // );
            return result;
        }

        match self.ask_depth.entry(price_tick) {
            Entry::Occupied(mut entry) => {
                let QtyTimestamp { qty: _, ts } = *entry.get();
                if ev.exch_ts >= ts {
                    if qty_lot > 0 {
                        *entry.get_mut() = QtyTimestamp {
                            qty: ev.qty,
                            ts: ev.exch_ts,
                        };
                    } else {
                        entry.remove();
                    }
                    result.push(ev.clone());
                } else {
                    // debug!(
                    //     ?ev,
                    //     ?ts,
                    //     "update_ask_depth: attempts to update the price level, \
                    //     but the event is outdated."
                    // );
                }
            }
            Entry::Vacant(entry) => {
                if qty_lot > 0 {
                    entry.insert(QtyTimestamp {
                        qty: ev.qty,
                        ts: ev.exch_ts,
                    });
                    result.push(ev.clone());
                }
            }
        }

        if qty_lot == 0 {
            if price_tick == self.best_ask_tick {
                self.best_ask_tick =
                    depth_above(&self.ask_depth, self.best_ask_tick, self.high_ask_tick);
                self.best_ask_timestamp = ev.exch_ts;
                if self.best_ask_tick == INVALID_MAX {
                    self.high_ask_tick = INVALID_MIN
                }
                // debug!(
                //     ?ev,
                //     best_bid_tick = self.best_bid_tick,
                //     best_ask_tick = self.best_ask_tick,
                //     "update_ask_depth: The BBO gets updated by deleting the prior BBO."
                // );
            }
        } else {
            if price_tick <= self.best_ask_tick {
                self.best_ask_tick = price_tick;
                self.best_ask_timestamp = ev.exch_ts;

                if price_tick <= self.best_bid_tick {
                    let prev_best_bid_tick = self.best_bid_tick;
                    self.best_bid_tick =
                        depth_below(&self.bid_depth, self.best_ask_tick, self.low_bid_tick);
                    self.best_bid_timestamp = ev.exch_ts;

                    let mut up_to_bid_tick = self.best_bid_tick;
                    if up_to_bid_tick == INVALID_MIN {
                        up_to_bid_tick = self.best_ask_tick - 1;
                        self.low_bid_tick = INVALID_MAX;
                    }

                    for t in (up_to_bid_tick + 1)..(prev_best_bid_tick + 1) {
                        if self.bid_depth.remove(&t).is_some() {
                            // debug!(
                            //     px = t as f64 * self.tick_size,
                            //     "update_ask_depth: Bid deletion event is generated \
                            //     by the best ask crossing."
                            // );
                            result.push(Event {
                                ev: BUY_EVENT | DEPTH_EVENT,
                                exch_ts: ev.exch_ts,
                                local_ts: ev.local_ts,
                                px: t as f64 * self.tick_size,
                                qty: 0.0,
                                order_id: 0,
                                ival: 0,
                                fval: 0.0,
                            });
                        }
                    }
                }
                // debug!(
                //     best_bid_tick = self.best_bid_tick,
                //     best_ask_tick = self.best_ask_tick,
                //     "update_ask_depth: The BBO gets updated."
                // );
            }
            self.high_ask_tick = self.high_ask_tick.max(price_tick);
        }
        result
    }

    pub fn clear_depth(&mut self, side: Side, clear_upto_price: f64, timestamp: i64) {
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
            self.best_bid_timestamp = timestamp;
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
            self.best_ask_timestamp = timestamp;
        } else {
            self.bid_depth.clear();
            self.ask_depth.clear();
            self.best_bid_tick = INVALID_MIN;
            self.best_ask_tick = INVALID_MAX;
            self.low_bid_tick = INVALID_MAX;
            self.high_ask_tick = INVALID_MIN;
            self.best_bid_timestamp = timestamp;
            self.best_ask_timestamp = timestamp;
        }
    }

    pub fn update_best_bid(&mut self, ev: Event) -> Vec<Event> {
        let mut result = Vec::new();
        let price_tick = (ev.px / self.tick_size).round() as i64;

        if ev.exch_ts < self.best_bid_timestamp
            || (price_tick >= self.best_ask_tick && ev.exch_ts < self.best_ask_timestamp)
        {
            // debug!(
            //     price_tick,
            //     best_bid_tick = self.best_bid_tick,
            //     best_ask_tick = self.best_ask_tick,
            //     best_bid_timestamp = self.best_bid_timestamp,
            //     best_ask_timestamp = self.best_ask_timestamp,
            //     "update_best_bid: attempts to update the BBO, but the event is outdated."
            // );
            return result;
        }

        match self.bid_depth.entry(price_tick) {
            Entry::Occupied(mut entry) => {
                let QtyTimestamp { qty, ts: _ } = *entry.get();
                if price_tick == self.best_bid_tick && ev.qty == qty {
                    self.best_bid_timestamp = ev.exch_ts;
                    return result;
                }
                *entry.get_mut() = QtyTimestamp {
                    qty: ev.qty,
                    ts: ev.exch_ts,
                };
            }
            Entry::Vacant(entry) => {
                entry.insert(QtyTimestamp {
                    qty: ev.qty,
                    ts: ev.exch_ts,
                });
            }
        }
        result.push(ev.clone());

        let prev_best_bid_tick = self.best_bid_tick;
        self.best_bid_tick = price_tick;
        if self.best_bid_tick < self.low_bid_tick {
            self.low_bid_tick = self.best_bid_tick;
        }
        self.best_bid_timestamp = ev.exch_ts;

        if price_tick >= self.best_ask_tick {
            let prev_best_ask_tick = self.best_ask_tick;
            self.best_ask_tick =
                depth_above(&self.ask_depth, self.best_bid_tick, self.high_ask_tick);
            self.best_ask_timestamp = ev.exch_ts;

            let mut up_to_ask_tick = self.best_ask_tick;
            if up_to_ask_tick == INVALID_MAX {
                up_to_ask_tick = self.best_bid_tick + 1;
                self.high_ask_tick = INVALID_MIN;
            }

            for t in prev_best_ask_tick..up_to_ask_tick {
                if self.ask_depth.remove(&t).is_some() {
                    // debug!(
                    //     px = t as f64 * self.tick_size,
                    //     "update_best_bid: Ask deletion event is generated \
                    //     by the best bid crossing."
                    // );
                    result.push(Event {
                        ev: SELL_EVENT | DEPTH_EVENT,
                        exch_ts: ev.exch_ts,
                        local_ts: ev.local_ts,
                        px: t as f64 * self.tick_size,
                        qty: 0.0,
                        order_id: 0,
                        ival: 0,
                        fval: 0.0,
                    });
                }
            }
        }
        // debug!(
        //     best_bid_tick = self.best_bid_tick,
        //     best_ask_tick = self.best_ask_tick,
        //     "update_best_bid: The BBO gets updated."
        // );

        if self.best_bid_tick < prev_best_bid_tick {
            for t in (self.best_bid_tick + 1)..(prev_best_bid_tick + 1) {
                if self.bid_depth.remove(&t).is_some() {
                    // debug!(
                    //     px = t as f64 * self.tick_size,
                    //     "update_best_bid: Bid deletion event is generated \
                    //     by the best bid backoff."
                    // );
                    result.push(Event {
                        ev: BUY_EVENT | DEPTH_EVENT,
                        exch_ts: ev.exch_ts,
                        local_ts: ev.local_ts,
                        px: t as f64 * self.tick_size,
                        qty: 0.0,
                        order_id: 0,
                        ival: 0,
                        fval: 0.0,
                    });
                }
            }
        }
        result
    }

    pub fn update_best_ask(&mut self, ev: Event) -> Vec<Event> {
        let mut result = Vec::new();
        let price_tick = (ev.px / self.tick_size).round() as i64;

        if ev.exch_ts < self.best_ask_timestamp
            || (price_tick <= self.best_bid_tick && ev.exch_ts < self.best_bid_timestamp)
        {
            // debug!(
            //     price_tick,
            //     best_bid_tick = self.best_bid_tick,
            //     best_ask_tick = self.best_ask_tick,
            //     best_bid_timestamp = self.best_bid_timestamp,
            //     best_ask_timestamp = self.best_ask_timestamp,
            //     "update_best_ask: attempts to update the BBO, but the event is outdated."
            // );
            return result;
        }

        match self.ask_depth.entry(price_tick) {
            Entry::Occupied(mut entry) => {
                let QtyTimestamp { qty, ts: _ } = *entry.get();
                if price_tick == self.best_ask_tick && ev.qty == qty {
                    self.best_ask_timestamp = ev.exch_ts;
                    return result;
                }
                *entry.get_mut() = QtyTimestamp {
                    qty: ev.qty,
                    ts: ev.exch_ts,
                };
            }
            Entry::Vacant(entry) => {
                entry.insert(QtyTimestamp {
                    qty: ev.qty,
                    ts: ev.exch_ts,
                });
            }
        }
        result.push(ev.clone());

        let prev_best_ask_tick = self.best_ask_tick;
        self.best_ask_tick = price_tick;
        if self.best_ask_tick > self.high_ask_tick {
            self.high_ask_tick = self.best_ask_tick;
        }
        self.best_ask_timestamp = ev.exch_ts;

        if price_tick <= self.best_bid_tick {
            let prev_best_bid_tick = self.best_bid_tick;
            self.best_bid_tick =
                depth_below(&self.bid_depth, self.best_ask_tick, self.low_bid_tick);
            self.best_bid_timestamp = ev.exch_ts;

            let mut up_to_bid_tick = self.best_bid_tick;
            if up_to_bid_tick == INVALID_MIN {
                up_to_bid_tick = self.best_ask_tick - 1;
                self.low_bid_tick = INVALID_MAX;
            }

            for t in (up_to_bid_tick + 1)..(prev_best_bid_tick + 1) {
                if self.bid_depth.remove(&t).is_some() {
                    // debug!(
                    //     px = t as f64 * self.tick_size,
                    //     "update_best_ask: Bid deletion event is generated \
                    //     by the best ask crossing."
                    // );
                    result.push(Event {
                        ev: BUY_EVENT | DEPTH_EVENT,
                        exch_ts: ev.exch_ts,
                        local_ts: ev.local_ts,
                        px: t as f64 * self.tick_size,
                        qty: 0.0,
                        order_id: 0,
                        ival: 0,
                        fval: 0.0,
                    });
                }
            }
        }
        // debug!(
        //     best_bid_tick = self.best_bid_tick,
        //     best_ask_tick = self.best_ask_tick,
        //     "update_best_ask: The BBO gets updated."
        // );

        if self.best_ask_tick > prev_best_ask_tick {
            for t in prev_best_ask_tick..self.best_ask_tick {
                if self.ask_depth.remove(&t).is_some() {
                    // debug!(
                    //     px = t as f64 * self.tick_size,
                    //     "update_best_ask: Ask deletion event is generated \
                    //     by the best ask backoff."
                    // );
                    result.push(Event {
                        ev: SELL_EVENT | DEPTH_EVENT,
                        exch_ts: ev.exch_ts,
                        local_ts: ev.local_ts,
                        px: t as f64 * self.tick_size,
                        qty: 0.0,
                        order_id: 0,
                        ival: 0,
                        fval: 0.0,
                    });
                }
            }
        }
        result
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
            let ts = data[row_num].exch_ts;

            let price_tick = (price / self.tick_size).round() as i64;
            if data[row_num].ev & BUY_EVENT == BUY_EVENT {
                if ts >= self.best_bid_timestamp {
                    self.best_bid_tick = self.best_bid_tick.max(price_tick);
                }
                self.low_bid_tick = self.low_bid_tick.min(price_tick);
                *self.bid_depth.entry(price_tick).or_default() = QtyTimestamp { qty, ts };
            } else if data[row_num].ev & SELL_EVENT == SELL_EVENT {
                if ts >= self.best_ask_timestamp {
                    self.best_ask_tick = self.best_ask_tick.min(price_tick);
                }
                self.high_ask_tick = self.high_ask_tick.max(price_tick);
                *self.ask_depth.entry(price_tick).or_default() = QtyTimestamp { qty, ts };
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
        for (px_tick, qty_ts) in bid_depth {
            events.push(Event {
                ev: EXCH_EVENT | LOCAL_EVENT | BUY_EVENT | DEPTH_SNAPSHOT_EVENT,
                exch_ts: qty_ts.ts,
                // todo: it's not a problem now, but it would be better to have valid timestamps.
                local_ts: 0,
                px: px_tick as f64 * self.tick_size,
                qty: qty_ts.qty,
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
        for (px_tick, qty_ts) in ask_depth {
            events.push(Event {
                ev: EXCH_EVENT | LOCAL_EVENT | SELL_EVENT | DEPTH_SNAPSHOT_EVENT,
                exch_ts: qty_ts.ts,
                // todo: it's not a problem now, but it would be better to have valid timestamps.
                local_ts: 0,
                px: px_tick as f64 * self.tick_size,
                qty: qty_ts.qty,
                order_id: 0,
                ival: 0,
                fval: 0.0,
            });
        }

        events
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
    fn best_bid_qty(&self) -> f64 {
        self.bid_depth
            .get(&self.best_bid_tick)
            .map(|value| value.qty)
            .unwrap_or(0.0)
    }

    #[inline(always)]
    fn best_ask_qty(&self) -> f64 {
        self.ask_depth
            .get(&self.best_ask_tick)
            .map(|value| value.qty)
            .unwrap_or(0.0)
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
            .map(|value| value.qty)
            .unwrap_or(0.0)
    }

    #[inline(always)]
    fn ask_qty_at_tick(&self, price_tick: i64) -> f64 {
        self.ask_depth
            .get(&price_tick)
            .map(|value| value.qty)
            .unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        depth::{FusedHashMapMarketDepth, MarketDepth},
        types::{BUY_EVENT, DEPTH_EVENT, Event, SELL_EVENT},
    };

    #[test]
    fn test_update_bid_depth() {
        let mut depth = FusedHashMapMarketDepth::new(0.1, 0.01);

        depth.update_bid_depth(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 10.1,
            qty: 0.01,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        depth.update_bid_depth(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 10.2,
            qty: 0.02,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.best_bid_tick(), 102);
        depth.update_bid_depth(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 0,
            local_ts: 0,
            px: 10.2,
            qty: 0.03,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.bid_qty_at_tick(102), 0.02);
        depth.update_bid_depth(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 0,
            local_ts: 0,
            px: 10.3,
            qty: 0.03,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.best_bid_tick(), 102);
        depth.update_bid_depth(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 2,
            local_ts: 2,
            px: 10.3,
            qty: 0.03,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.best_bid_tick(), 103);
        depth.update_bid_depth(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 10.3,
            qty: 0.0,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.best_bid_tick(), 103);
        assert_eq!(depth.bid_qty_at_tick(103), 0.03);
        depth.update_bid_depth(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 2,
            local_ts: 2,
            px: 10.3,
            qty: 0.0,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.best_bid_tick(), 102);
    }

    #[test]
    fn test_update_ask_depth() {
        let mut depth = FusedHashMapMarketDepth::new(0.1, 0.01);

        depth.update_ask_depth(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 10.2,
            qty: 0.02,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        depth.update_ask_depth(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 10.1,
            qty: 0.01,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.best_ask_tick(), 101);
        depth.update_ask_depth(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 0,
            local_ts: 0,
            px: 10.1,
            qty: 0.03,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.ask_qty_at_tick(101), 0.01);
        depth.update_ask_depth(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 0,
            local_ts: 0,
            px: 10.0,
            qty: 0.03,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.best_ask_tick(), 101);
        depth.update_ask_depth(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 2,
            local_ts: 2,
            px: 10.0,
            qty: 0.03,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.best_ask_tick(), 100);
        depth.update_ask_depth(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 10.0,
            qty: 0.0,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.best_ask_tick(), 100);
        assert_eq!(depth.ask_qty_at_tick(100), 0.03);
        depth.update_ask_depth(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 2,
            local_ts: 2,
            px: 10.0,
            qty: 0.0,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.best_ask_tick(), 101);
    }

    #[test]
    fn test_update_bid_ask_depth_cross() {
        let mut depth = FusedHashMapMarketDepth::new(0.1, 0.01);

        depth.update_bid_depth(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 10.1,
            qty: 0.01,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        depth.update_bid_depth(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 10.2,
            qty: 0.02,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        depth.update_ask_depth(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 10.3,
            qty: 0.02,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        depth.update_ask_depth(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 10.4,
            qty: 0.01,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });

        depth.update_ask_depth(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 3,
            local_ts: 3,
            px: 10.2,
            qty: 0.01,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.best_bid_tick(), 101);
        assert_eq!(depth.best_ask_tick(), 102);

        depth.update_bid_depth(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 5,
            local_ts: 5,
            px: 10.2,
            qty: 0.03,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.best_bid_tick(), 102);
        assert_eq!(depth.best_ask_tick(), 103);

        depth.update_ask_depth(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 4,
            local_ts: 4,
            px: 10.2,
            qty: 0.01,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.best_bid_tick(), 102);
        assert_eq!(depth.best_ask_tick(), 103);
        depth.update_ask_depth(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 4,
            local_ts: 4,
            px: 10.2,
            qty: 0.0,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });

        depth.update_ask_depth(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 7,
            local_ts: 7,
            px: 10.3,
            qty: 0.01,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        depth.update_bid_depth(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 6,
            local_ts: 6,
            px: 10.3,
            qty: 0.01,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.best_bid_tick(), 102);
        assert_eq!(depth.best_ask_tick(), 103);
    }

    #[test]
    fn test_update_best_bid() {
        let mut depth = FusedHashMapMarketDepth::new(0.1, 0.01);

        depth.update_bid_depth(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 10.1,
            qty: 0.01,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        depth.update_bid_depth(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 10.2,
            qty: 0.02,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        depth.update_ask_depth(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 10.3,
            qty: 0.02,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        depth.update_ask_depth(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 10.4,
            qty: 0.01,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });

        depth.update_best_bid(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 0,
            local_ts: 0,
            px: 10.3,
            qty: 0.03,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.best_bid_tick(), 102);
        depth.update_best_bid(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 2,
            local_ts: 2,
            px: 10.2,
            qty: 0.03,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.bid_qty_at_tick(102), 0.03);
        depth.update_best_bid(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 3,
            local_ts: 3,
            px: 10.3,
            qty: 0.01,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.best_bid_tick(), 103);
        assert_eq!(depth.bid_qty_at_tick(103), 0.01);
        assert_eq!(depth.best_ask_tick(), 104);

        depth.update_bid_depth(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 5,
            local_ts: 5,
            px: 10.1,
            qty: 0.01,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        depth.update_bid_depth(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 5,
            local_ts: 5,
            px: 10.2,
            qty: 0.02,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        depth.update_best_bid(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 2,
            local_ts: 2,
            px: 10.1,
            qty: 0.05,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.best_bid_tick(), 103);
        assert_eq!(depth.best_ask_tick(), 104);
        assert_eq!(depth.bid_qty_at_tick(101), 0.01);

        depth.update_best_bid(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 6,
            local_ts: 6,
            px: 10.1,
            qty: 0.05,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.best_bid_tick(), 101);
    }

    #[test]
    fn test_update_best_ask() {
        let mut depth = FusedHashMapMarketDepth::new(0.1, 0.01);

        depth.update_bid_depth(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 10.1,
            qty: 0.01,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        depth.update_bid_depth(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 10.2,
            qty: 0.02,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        depth.update_ask_depth(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 10.3,
            qty: 0.02,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        depth.update_ask_depth(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 10.4,
            qty: 0.01,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });

        depth.update_best_ask(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 0,
            local_ts: 0,
            px: 10.2,
            qty: 0.03,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.best_ask_tick(), 103);
        depth.update_best_ask(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 2,
            local_ts: 2,
            px: 10.3,
            qty: 0.03,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.ask_qty_at_tick(103), 0.03);
        depth.update_best_ask(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 3,
            local_ts: 3,
            px: 10.2,
            qty: 0.01,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.best_bid_tick(), 101);
        assert_eq!(depth.ask_qty_at_tick(102), 0.01);
        assert_eq!(depth.best_ask_tick(), 102);

        depth.update_ask_depth(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 5,
            local_ts: 5,
            px: 10.3,
            qty: 0.02,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        depth.update_ask_depth(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 5,
            local_ts: 5,
            px: 10.4,
            qty: 0.01,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        depth.update_best_ask(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 2,
            local_ts: 2,
            px: 10.4,
            qty: 0.05,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.best_bid_tick(), 101);
        assert_eq!(depth.best_ask_tick(), 102);
        assert_eq!(depth.ask_qty_at_tick(104), 0.01);

        depth.update_best_ask(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 6,
            local_ts: 6,
            px: 10.4,
            qty: 0.05,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        assert_eq!(depth.best_ask_tick(), 104);
    }

    #[test]
    fn test_bound_after_bbo_update() {
        // Test code for the issue: https://github.com/nkaz001/hftbacktest/issues/244
        let mut depth = FusedHashMapMarketDepth::new(1.0, 1.0);

        depth.update_ask_depth(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 102.0,
            qty: 1.0,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        depth.update_bid_depth(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 100.0,
            qty: 1.0,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });

        depth.update_best_ask(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 99.0,
            qty: 1.0,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        depth.update_best_bid(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 98.0,
            qty: 1.0,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });

        assert_eq!(depth.best_ask_tick, 99);
        assert_eq!(depth.best_bid_tick, 98);
        assert_eq!(depth.low_bid_tick, 98);
        assert_eq!(depth.high_ask_tick, 102);

        let mut depth = FusedHashMapMarketDepth::new(1.0, 1.0);

        depth.update_ask_depth(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 102.0,
            qty: 1.0,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        depth.update_bid_depth(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 100.0,
            qty: 1.0,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });

        depth.update_best_bid(Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 103.0,
            qty: 1.0,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });
        depth.update_best_ask(Event {
            ev: SELL_EVENT | DEPTH_EVENT,
            exch_ts: 1,
            local_ts: 1,
            px: 104.0,
            qty: 1.0,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        });

        assert_eq!(depth.best_ask_tick, 104);
        assert_eq!(depth.best_bid_tick, 103);
        assert_eq!(depth.low_bid_tick, 100);
        assert_eq!(depth.high_ask_tick, 104);
    }
}
