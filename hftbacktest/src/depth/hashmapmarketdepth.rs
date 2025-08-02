use std::collections::{HashMap, hash_map::Entry};

use super::{ApplySnapshot, INVALID_MAX, INVALID_MIN, L3MarketDepth, L3Order, MarketDepth};
use crate::{
    backtest::{BacktestError, data::Data},
    prelude::{L2MarketDepth, OrderId, Side},
    types::{BUY_EVENT, DEPTH_SNAPSHOT_EVENT, EXCH_EVENT, Event, LOCAL_EVENT, SELL_EVENT},
};

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
pub struct HashMapMarketDepth {
    pub tick_size: f64,
    pub lot_size: f64,
    pub timestamp: i64,
    pub ask_depth: HashMap<i64, f64>,
    pub bid_depth: HashMap<i64, f64>,
    pub best_bid_tick: i64,
    pub best_ask_tick: i64,
    pub low_bid_tick: i64,
    pub high_ask_tick: i64,
    pub orders: HashMap<OrderId, L3Order>,
}

#[inline(always)]
fn depth_below(depth: &HashMap<i64, f64>, start: i64, end: i64) -> i64 {
    for t in (end..start).rev() {
        if *depth.get(&t).unwrap_or(&0f64) > 0f64 {
            return t;
        }
    }
    INVALID_MIN
}

#[inline(always)]
fn depth_above(depth: &HashMap<i64, f64>, start: i64, end: i64) -> i64 {
    for t in (start + 1)..(end + 1) {
        if *depth.get(&t).unwrap_or(&0f64) > 0f64 {
            return t;
        }
    }
    INVALID_MAX
}

impl HashMapMarketDepth {
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
            *self.bid_depth.entry(order.price_tick).or_insert(0.0) += order.qty;
        } else {
            *self.ask_depth.entry(order.price_tick).or_insert(0.0) += order.qty;
        }
        Ok(())
    }
}

impl L2MarketDepth for HashMapMarketDepth {
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
                prev_qty = *entry.get();
                if qty_lot > 0 {
                    *entry.get_mut() = qty;
                } else {
                    entry.remove();
                }
            }
            Entry::Vacant(entry) => {
                prev_qty = 0f64;
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
                prev_qty = *entry.get();
                if qty_lot > 0 {
                    *entry.get_mut() = qty;
                } else {
                    entry.remove();
                }
            }
            Entry::Vacant(entry) => {
                prev_qty = 0f64;
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

    fn clear_depth(&mut self, side: Side, clear_upto_price: f64) {
        match side {
            Side::Buy => {
                if clear_upto_price.is_finite() {
                    let clear_upto = (clear_upto_price / self.tick_size).round() as i64;
                    if self.best_bid_tick != INVALID_MIN {
                        for t in clear_upto..(self.best_bid_tick + 1) {
                            if self.bid_depth.contains_key(&t) {
                                self.bid_depth.remove(&t);
                            }
                        }
                    }
                    self.best_bid_tick =
                        depth_below(&self.bid_depth, clear_upto - 1, self.low_bid_tick);
                } else {
                    self.bid_depth.clear();
                    self.best_bid_tick = INVALID_MIN;
                }
                if self.best_bid_tick == INVALID_MIN {
                    self.low_bid_tick = INVALID_MAX;
                }
            }
            Side::Sell => {
                if clear_upto_price.is_finite() {
                    let clear_upto = (clear_upto_price / self.tick_size).round() as i64;
                    if self.best_ask_tick != INVALID_MAX {
                        for t in self.best_ask_tick..(clear_upto + 1) {
                            if self.ask_depth.contains_key(&t) {
                                self.ask_depth.remove(&t);
                            }
                        }
                    }
                    self.best_ask_tick =
                        depth_above(&self.ask_depth, clear_upto + 1, self.high_ask_tick);
                } else {
                    self.ask_depth.clear();
                    self.best_ask_tick = INVALID_MAX;
                }
                if self.best_ask_tick == INVALID_MAX {
                    self.high_ask_tick = INVALID_MIN;
                }
            }
            Side::None => {
                self.bid_depth.clear();
                self.ask_depth.clear();
                self.best_bid_tick = INVALID_MIN;
                self.best_ask_tick = INVALID_MAX;
                self.low_bid_tick = INVALID_MAX;
                self.high_ask_tick = INVALID_MIN;
            }
            Side::Unsupported => {
                unreachable!();
            }
        }
    }
}

impl MarketDepth for HashMapMarketDepth {
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
        *self.bid_depth.get(&self.best_bid_tick).unwrap_or(&0.0)
    }

    #[inline(always)]
    fn best_ask_qty(&self) -> f64 {
        *self.ask_depth.get(&self.best_ask_tick).unwrap_or(&0.0)
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
        *self.bid_depth.get(&price_tick).unwrap_or(&0.0)
    }

    #[inline(always)]
    fn ask_qty_at_tick(&self, price_tick: i64) -> f64 {
        *self.ask_depth.get(&price_tick).unwrap_or(&0.0)
    }
}

impl ApplySnapshot for HashMapMarketDepth {
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

            let price_tick = (price / self.tick_size).round() as i64;
            if data[row_num].ev & BUY_EVENT == BUY_EVENT {
                self.best_bid_tick = self.best_bid_tick.max(price_tick);
                self.low_bid_tick = self.low_bid_tick.min(price_tick);
                *self.bid_depth.entry(price_tick).or_insert(0f64) = qty;
            } else if data[row_num].ev & SELL_EVENT == SELL_EVENT {
                self.best_ask_tick = self.best_ask_tick.min(price_tick);
                self.high_ask_tick = self.high_ask_tick.max(price_tick);
                *self.ask_depth.entry(price_tick).or_insert(0f64) = qty;
            }
        }
    }

    fn snapshot(&self) -> Vec<Event> {
        let mut events = Vec::new();

        let mut bid_depth = self
            .bid_depth
            .iter()
            .map(|(&px_tick, &qty)| (px_tick, qty))
            .collect::<Vec<_>>();
        bid_depth.sort_by(|a, b| b.0.cmp(&a.0));
        for (px_tick, qty) in bid_depth {
            events.push(Event {
                ev: EXCH_EVENT | LOCAL_EVENT | BUY_EVENT | DEPTH_SNAPSHOT_EVENT,
                // todo: it's not a problem now, but it would be better to have valid timestamps.
                exch_ts: 0,
                local_ts: 0,
                px: px_tick as f64 * self.tick_size,
                qty,
                order_id: 0,
                ival: 0,
                fval: 0.0,
            });
        }

        let mut ask_depth = self
            .ask_depth
            .iter()
            .map(|(&px_tick, &qty)| (px_tick, qty))
            .collect::<Vec<_>>();
        ask_depth.sort_by(|a, b| a.0.cmp(&b.0));
        for (px_tick, qty) in ask_depth {
            events.push(Event {
                ev: EXCH_EVENT | LOCAL_EVENT | SELL_EVENT | DEPTH_SNAPSHOT_EVENT,
                // todo: it's not a problem now, but it would be better to have valid timestamps.
                exch_ts: 0,
                local_ts: 0,
                px: px_tick as f64 * self.tick_size,
                qty,
                order_id: 0,
                ival: 0,
                fval: 0.0,
            });
        }

        events
    }
}

impl L3MarketDepth for HashMapMarketDepth {
    type Error = BacktestError;

    fn add_buy_order(
        &mut self,
        order_id: OrderId,
        px: f64,
        qty: f64,
        timestamp: i64,
    ) -> Result<(i64, i64), Self::Error> {
        let price_tick = (px / self.tick_size).round() as i64;
        self.add(L3Order {
            order_id,
            side: Side::Buy,
            price_tick,
            qty,
            timestamp,
        })?;
        let prev_best_tick = self.best_bid_tick;
        if price_tick > self.best_bid_tick {
            self.best_bid_tick = price_tick;
            if self.best_bid_tick >= self.best_ask_tick {
                self.best_ask_tick =
                    depth_above(&self.ask_depth, self.best_bid_tick, self.high_ask_tick);
            }
        }
        self.low_bid_tick = self.low_bid_tick.min(price_tick);
        Ok((prev_best_tick, self.best_bid_tick))
    }

    fn add_sell_order(
        &mut self,
        order_id: OrderId,
        px: f64,
        qty: f64,
        timestamp: i64,
    ) -> Result<(i64, i64), Self::Error> {
        let price_tick = (px / self.tick_size).round() as i64;
        self.add(L3Order {
            order_id,
            side: Side::Sell,
            price_tick,
            qty,
            timestamp,
        })?;
        let prev_best_tick = self.best_ask_tick;
        if price_tick < self.best_ask_tick {
            self.best_ask_tick = price_tick;
            if self.best_bid_tick >= self.best_ask_tick {
                self.best_bid_tick =
                    depth_below(&self.bid_depth, self.best_ask_tick, self.low_bid_tick);
            }
        }
        self.high_ask_tick = self.high_ask_tick.max(price_tick);
        Ok((prev_best_tick, self.best_ask_tick))
    }

    fn delete_order(
        &mut self,
        order_id: OrderId,
        _timestamp: i64,
    ) -> Result<(Side, i64, i64), Self::Error> {
        let order = self
            .orders
            .remove(&order_id)
            .ok_or(BacktestError::OrderNotFound)?;
        if order.side == Side::Buy {
            let prev_best_tick = self.best_bid_tick;

            let depth_qty = self.bid_depth.get_mut(&order.price_tick).unwrap();
            *depth_qty -= order.qty;
            if (*depth_qty / self.lot_size).round() as i64 == 0 {
                self.bid_depth.remove(&order.price_tick).unwrap();
                if order.price_tick == self.best_bid_tick {
                    self.best_bid_tick =
                        depth_below(&self.bid_depth, self.best_bid_tick, self.low_bid_tick);
                    if self.best_bid_tick == INVALID_MIN {
                        self.low_bid_tick = INVALID_MAX
                    }
                }
            }
            Ok((Side::Buy, prev_best_tick, self.best_bid_tick))
        } else {
            let prev_best_tick = self.best_ask_tick;

            let depth_qty = self.ask_depth.get_mut(&order.price_tick).unwrap();
            *depth_qty -= order.qty;
            if (*depth_qty / self.lot_size).round() as i64 == 0 {
                self.ask_depth.remove(&order.price_tick).unwrap();
                if order.price_tick == self.best_ask_tick {
                    self.best_ask_tick =
                        depth_above(&self.ask_depth, self.best_ask_tick, self.high_ask_tick);
                    if self.best_ask_tick == INVALID_MAX {
                        self.high_ask_tick = INVALID_MIN
                    }
                }
            }
            Ok((Side::Sell, prev_best_tick, self.best_ask_tick))
        }
    }

    fn modify_order(
        &mut self,
        order_id: OrderId,
        px: f64,
        qty: f64,
        timestamp: i64,
    ) -> Result<(Side, i64, i64), Self::Error> {
        let order = self
            .orders
            .get_mut(&order_id)
            .ok_or(BacktestError::OrderNotFound)?;
        if order.side == Side::Buy {
            let prev_best_tick = self.best_bid_tick;
            let price_tick = (px / self.tick_size).round() as i64;
            if price_tick != order.price_tick {
                let depth_qty = self.bid_depth.get_mut(&order.price_tick).unwrap();
                *depth_qty -= order.qty;
                if (*depth_qty / self.lot_size).round() as i64 == 0 {
                    self.bid_depth.remove(&order.price_tick).unwrap();
                    if order.price_tick == self.best_bid_tick {
                        self.best_bid_tick =
                            depth_below(&self.bid_depth, self.best_bid_tick, self.low_bid_tick);
                        if self.best_bid_tick == INVALID_MIN {
                            self.low_bid_tick = INVALID_MAX
                        }
                    }
                }

                order.price_tick = price_tick;
                order.qty = qty;
                order.timestamp = timestamp;

                *self.bid_depth.entry(order.price_tick).or_insert(0.0) += order.qty;

                if price_tick > self.best_bid_tick {
                    self.best_bid_tick = price_tick;
                    if self.best_bid_tick >= self.best_ask_tick {
                        self.best_ask_tick =
                            depth_above(&self.ask_depth, self.best_bid_tick, self.high_ask_tick);
                    }
                }
                self.low_bid_tick = self.low_bid_tick.min(price_tick);
                Ok((Side::Buy, prev_best_tick, self.best_bid_tick))
            } else {
                let depth_qty = self.bid_depth.get_mut(&order.price_tick).unwrap();
                *depth_qty += qty - order.qty;
                order.qty = qty;
                Ok((Side::Buy, self.best_bid_tick, self.best_bid_tick))
            }
        } else {
            let prev_best_tick = self.best_ask_tick;
            let price_tick = (px / self.tick_size).round() as i64;
            if price_tick != order.price_tick {
                let depth_qty = self.ask_depth.get_mut(&order.price_tick).unwrap();
                *depth_qty -= order.qty;
                if (*depth_qty / self.lot_size).round() as i64 == 0 {
                    self.ask_depth.remove(&order.price_tick).unwrap();
                    if order.price_tick == self.best_ask_tick {
                        self.best_ask_tick =
                            depth_above(&self.ask_depth, self.best_ask_tick, self.high_ask_tick);
                        if self.best_ask_tick == INVALID_MAX {
                            self.high_ask_tick = INVALID_MIN
                        }
                    }
                }

                order.price_tick = price_tick;
                order.qty = qty;
                order.timestamp = timestamp;

                *self.ask_depth.entry(order.price_tick).or_insert(0.0) += order.qty;

                if price_tick < self.best_ask_tick {
                    self.best_ask_tick = price_tick;
                    if self.best_bid_tick >= self.best_ask_tick {
                        self.best_bid_tick =
                            depth_below(&self.bid_depth, self.best_ask_tick, self.low_bid_tick);
                    }
                }
                self.high_ask_tick = self.high_ask_tick.max(price_tick);
                Ok((Side::Sell, prev_best_tick, self.best_ask_tick))
            } else {
                let depth_qty = self.ask_depth.get_mut(&order.price_tick).unwrap();
                *depth_qty += qty - order.qty;
                order.qty = qty;
                Ok((Side::Sell, self.best_ask_tick, self.best_ask_tick))
            }
        }
    }

    fn clear_orders(&mut self, side: Side) {
        match side {
            Side::Buy => {
                L2MarketDepth::clear_depth(self, side, f64::NEG_INFINITY);
                let order_ids: Vec<_> = self
                    .orders
                    .iter()
                    .filter(|(_, order)| order.side == Side::Buy)
                    .map(|(order_id, _)| *order_id)
                    .collect();
                order_ids
                    .iter()
                    .for_each(|order_id| _ = self.orders.remove(order_id).unwrap());
            }
            Side::Sell => {
                L2MarketDepth::clear_depth(self, side, f64::INFINITY);
                let order_ids: Vec<_> = self
                    .orders
                    .iter()
                    .filter(|(_, order)| order.side == Side::Sell)
                    .map(|(order_id, _)| *order_id)
                    .collect();
                order_ids
                    .iter()
                    .for_each(|order_id| _ = self.orders.remove(order_id).unwrap());
            }
            Side::None => {
                L2MarketDepth::clear_depth(self, side, f64::NAN);
                self.orders.clear();
            }
            Side::Unsupported => {
                unreachable!();
            }
        }
    }

    fn orders(&self) -> &HashMap<OrderId, L3Order> {
        &self.orders
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        depth::{HashMapMarketDepth, INVALID_MAX, INVALID_MIN, L3MarketDepth, MarketDepth},
        types::Side,
    };

    macro_rules! assert_eq_qty {
        ( $a:expr, $b:expr, $lot_size:ident ) => {{
            assert_eq!(
                ($a / $lot_size).round() as i64,
                ($b / $lot_size).round() as i64
            );
        }};
    }

    #[test]
    fn test_l3_add_delete_buy_order() {
        let lot_size = 0.001;
        let mut depth = HashMapMarketDepth::new(0.1, lot_size);

        let (prev_best, best) = depth.add_buy_order(1, 500.1, 0.001, 0).unwrap();
        assert_eq!(prev_best, INVALID_MIN);
        assert_eq!(best, 5001);
        assert_eq!(depth.best_bid_tick(), 5001);
        assert_eq_qty!(depth.bid_qty_at_tick(5001), 0.001, lot_size);

        assert!(depth.add_buy_order(1, 500.2, 0.001, 0).is_err());

        let (prev_best, best) = depth.add_buy_order(2, 500.3, 0.005, 0).unwrap();
        assert_eq!(prev_best, 5001);
        assert_eq!(best, 5003);
        assert_eq!(depth.best_bid_tick(), 5003);
        assert_eq_qty!(depth.bid_qty_at_tick(5003), 0.005, lot_size);

        let (prev_best, best) = depth.add_buy_order(3, 500.1, 0.005, 0).unwrap();
        assert_eq!(prev_best, 5003);
        assert_eq!(best, 5003);
        assert_eq!(depth.best_bid_tick(), 5003);
        assert_eq_qty!(depth.bid_qty_at_tick(5001), 0.006, lot_size);

        let (prev_best, best) = depth.add_buy_order(4, 500.5, 0.005, 0).unwrap();
        assert_eq!(prev_best, 5003);
        assert_eq!(best, 5005);
        assert_eq!(depth.best_bid_tick(), 5005);
        assert_eq_qty!(depth.bid_qty_at_tick(5005), 0.005, lot_size);

        assert!(depth.delete_order(10, 0).is_err());

        let (side, prev_best, best) = depth.delete_order(2, 0).unwrap();
        assert_eq!(side, Side::Buy);
        assert_eq!(prev_best, 5005);
        assert_eq!(best, 5005);
        assert_eq!(depth.best_bid_tick(), 5005);
        assert_eq_qty!(depth.bid_qty_at_tick(5003), 0.0, lot_size);

        let (side, prev_best, best) = depth.delete_order(4, 0).unwrap();
        assert_eq!(side, Side::Buy);
        assert_eq!(prev_best, 5005);
        assert_eq!(best, 5001);
        assert_eq!(depth.best_bid_tick(), 5001);
        assert_eq_qty!(depth.bid_qty_at_tick(5005), 0.0, lot_size);

        let (side, prev_best, best) = depth.delete_order(3, 0).unwrap();
        assert_eq!(side, Side::Buy);
        assert_eq!(prev_best, 5001);
        assert_eq!(best, 5001);
        assert_eq!(depth.best_bid_tick(), 5001);
        assert_eq_qty!(depth.bid_qty_at_tick(5001), 0.001, lot_size);

        let (side, prev_best, best) = depth.delete_order(1, 0).unwrap();
        assert_eq!(side, Side::Buy);
        assert_eq!(prev_best, 5001);
        assert_eq!(best, INVALID_MIN);
        assert_eq!(depth.best_bid_tick(), INVALID_MIN);
        assert_eq_qty!(depth.bid_qty_at_tick(5001), 0.0, lot_size);
    }

    #[test]
    fn test_l3_add_delete_sell_order() {
        let lot_size = 0.001;
        let mut depth = HashMapMarketDepth::new(0.1, lot_size);

        let (prev_best, best) = depth.add_sell_order(1, 500.1, 0.001, 0).unwrap();
        assert_eq!(prev_best, INVALID_MAX);
        assert_eq!(best, 5001);
        assert_eq!(depth.best_ask_tick(), 5001);
        assert_eq_qty!(depth.ask_qty_at_tick(5001), 0.001, lot_size);

        assert!(depth.add_sell_order(1, 500.2, 0.001, 0).is_err());

        let (prev_best, best) = depth.add_sell_order(2, 499.3, 0.005, 0).unwrap();
        assert_eq!(prev_best, 5001);
        assert_eq!(best, 4993);
        assert_eq!(depth.best_ask_tick(), 4993);
        assert_eq_qty!(depth.ask_qty_at_tick(4993), 0.005, lot_size);

        let (prev_best, best) = depth.add_sell_order(3, 500.1, 0.005, 0).unwrap();
        assert_eq!(prev_best, 4993);
        assert_eq!(best, 4993);
        assert_eq!(depth.best_ask_tick(), 4993);
        assert_eq_qty!(depth.ask_qty_at_tick(5001), 0.006, lot_size);

        let (prev_best, best) = depth.add_sell_order(4, 498.5, 0.005, 0).unwrap();
        assert_eq!(prev_best, 4993);
        assert_eq!(best, 4985);
        assert_eq!(depth.best_ask_tick(), 4985);
        assert_eq_qty!(depth.ask_qty_at_tick(4985), 0.005, lot_size);

        assert!(depth.delete_order(10, 0).is_err());

        let (side, prev_best, best) = depth.delete_order(2, 0).unwrap();
        assert_eq!(side, Side::Sell);
        assert_eq!(prev_best, 4985);
        assert_eq!(best, 4985);
        assert_eq!(depth.best_ask_tick(), 4985);
        assert_eq_qty!(depth.ask_qty_at_tick(4993), 0.0, lot_size);

        let (side, prev_best, best) = depth.delete_order(4, 0).unwrap();
        assert_eq!(side, Side::Sell);
        assert_eq!(prev_best, 4985);
        assert_eq!(best, 5001);
        assert_eq!(depth.best_ask_tick(), 5001);
        assert_eq_qty!(depth.ask_qty_at_tick(4985), 0.0, lot_size);

        let (side, prev_best, best) = depth.delete_order(3, 0).unwrap();
        assert_eq!(side, Side::Sell);
        assert_eq!(prev_best, 5001);
        assert_eq!(best, 5001);
        assert_eq!(depth.best_ask_tick(), 5001);
        assert_eq_qty!(depth.ask_qty_at_tick(5001), 0.001, lot_size);

        let (side, prev_best, best) = depth.delete_order(1, 0).unwrap();
        assert_eq!(side, Side::Sell);
        assert_eq!(prev_best, 5001);
        assert_eq!(best, INVALID_MAX);
        assert_eq!(depth.best_ask_tick(), INVALID_MAX);
        assert_eq_qty!(depth.ask_qty_at_tick(5001), 0.0, lot_size);
    }

    #[test]
    fn test_l3_modify_buy_order() {
        let lot_size = 0.001;
        let mut depth = HashMapMarketDepth::new(0.1, lot_size);

        depth.add_buy_order(1, 500.1, 0.001, 0).unwrap();
        depth.add_buy_order(2, 500.3, 0.005, 0).unwrap();
        depth.add_buy_order(3, 500.1, 0.005, 0).unwrap();
        depth.add_buy_order(4, 500.5, 0.005, 0).unwrap();

        assert!(depth.modify_order(10, 500.5, 0.001, 0).is_err());

        let (side, prev_best, best) = depth.modify_order(2, 500.5, 0.001, 0).unwrap();
        assert_eq!(side, Side::Buy);
        assert_eq!(prev_best, 5005);
        assert_eq!(best, 5005);
        assert_eq!(depth.best_bid_tick(), 5005);
        assert_eq_qty!(depth.bid_qty_at_tick(5005), 0.006, lot_size);

        let (side, prev_best, best) = depth.modify_order(2, 500.7, 0.002, 0).unwrap();
        assert_eq!(side, Side::Buy);
        assert_eq!(prev_best, 5005);
        assert_eq!(best, 5007);
        assert_eq!(depth.best_bid_tick(), 5007);
        assert_eq_qty!(depth.bid_qty_at_tick(5005), 0.005, lot_size);
        assert_eq_qty!(depth.bid_qty_at_tick(5007), 0.002, lot_size);

        let (side, prev_best, best) = depth.modify_order(2, 500.6, 0.002, 0).unwrap();
        assert_eq!(side, Side::Buy);
        assert_eq!(prev_best, 5007);
        assert_eq!(best, 5006);
        assert_eq!(depth.best_bid_tick(), 5006);
        assert_eq_qty!(depth.bid_qty_at_tick(5007), 0.0, lot_size);

        let _ = depth.delete_order(4, 0).unwrap();
        let (side, prev_best, best) = depth.modify_order(2, 500.0, 0.002, 0).unwrap();
        assert_eq!(side, Side::Buy);
        assert_eq!(prev_best, 5006);
        assert_eq!(best, 5001);
        assert_eq!(depth.best_bid_tick(), 5001);
        assert_eq_qty!(depth.bid_qty_at_tick(5006), 0.0, lot_size);
        assert_eq_qty!(depth.bid_qty_at_tick(5000), 0.002, lot_size);
    }

    #[test]
    fn test_l3_modify_sell_order() {
        let lot_size = 0.001;
        let mut depth = HashMapMarketDepth::new(0.1, lot_size);

        depth.add_sell_order(1, 500.1, 0.001, 0).unwrap();
        depth.add_sell_order(2, 499.3, 0.005, 0).unwrap();
        depth.add_sell_order(3, 500.1, 0.005, 0).unwrap();
        depth.add_sell_order(4, 498.5, 0.005, 0).unwrap();

        assert!(depth.modify_order(10, 500.5, 0.001, 0).is_err());

        let (side, prev_best, best) = depth.modify_order(2, 498.5, 0.001, 0).unwrap();
        assert_eq!(side, Side::Sell);
        assert_eq!(prev_best, 4985);
        assert_eq!(best, 4985);
        assert_eq!(depth.best_ask_tick(), 4985);
        assert_eq_qty!(depth.ask_qty_at_tick(4985), 0.006, lot_size);

        let (side, prev_best, best) = depth.modify_order(2, 497.7, 0.002, 0).unwrap();
        assert_eq!(side, Side::Sell);
        assert_eq!(prev_best, 4985);
        assert_eq!(best, 4977);
        assert_eq!(depth.best_ask_tick(), 4977);
        assert_eq_qty!(depth.ask_qty_at_tick(4985), 0.005, lot_size);
        assert_eq_qty!(depth.ask_qty_at_tick(4977), 0.002, lot_size);

        let (side, prev_best, best) = depth.modify_order(2, 498.1, 0.002, 0).unwrap();
        assert_eq!(side, Side::Sell);
        assert_eq!(prev_best, 4977);
        assert_eq!(best, 4981);
        assert_eq!(depth.best_ask_tick(), 4981);
        assert_eq_qty!(depth.ask_qty_at_tick(4977), 0.0, lot_size);

        let _ = depth.delete_order(4, 0).unwrap();
        let (side, prev_best, best) = depth.modify_order(2, 500.2, 0.002, 0).unwrap();
        assert_eq!(side, Side::Sell);
        assert_eq!(prev_best, 4981);
        assert_eq!(best, 5001);
        assert_eq!(depth.best_ask_tick(), 5001);
        assert_eq_qty!(depth.ask_qty_at_tick(4981), 0.0, lot_size);
        assert_eq_qty!(depth.ask_qty_at_tick(5002), 0.002, lot_size);
    }
}
