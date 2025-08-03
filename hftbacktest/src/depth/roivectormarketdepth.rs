use std::collections::{HashMap, hash_map::Entry};

use super::{ApplySnapshot, INVALID_MAX, INVALID_MIN, L3MarketDepth, L3Order, MarketDepth};
use crate::{
    backtest::{BacktestError, data::Data},
    prelude::{L2MarketDepth, OrderId, Side},
    types::{BUY_EVENT, Event, SELL_EVENT},
};

/// L2/L3 market depth implementation based on a vector within the range of interest.
///
/// This is a variant of the HashMap-based market depth implementation, which only handles the
/// specific range of interest. By doing so, it improves performance, especially when the strategy
/// requires computing values based on the order book around the mid-price.
pub struct ROIVectorMarketDepth {
    pub tick_size: f64,
    pub lot_size: f64,
    pub timestamp: i64,
    pub ask_depth: Vec<f64>,
    pub bid_depth: Vec<f64>,
    pub best_bid_tick: i64,
    pub best_ask_tick: i64,
    pub low_bid_tick: i64,
    pub high_ask_tick: i64,
    pub roi_ub: i64,
    pub roi_lb: i64,
    pub orders: HashMap<OrderId, L3Order>,
}

#[inline(always)]
fn depth_below(depth: &[f64], start: i64, end: i64, roi_lb: i64, roi_ub: i64) -> i64 {
    let start = (start.min(roi_ub) - roi_lb) as usize;
    let end = (end.max(roi_lb) - roi_lb) as usize;
    for t in (end..start).rev() {
        if unsafe { *depth.get_unchecked(t) } > 0f64 {
            return t as i64 + roi_lb;
        }
    }
    INVALID_MIN
}

#[inline(always)]
fn depth_above(depth: &[f64], start: i64, end: i64, roi_lb: i64, roi_ub: i64) -> i64 {
    let start = (start.max(roi_lb) - roi_lb) as usize;
    let end = (end.min(roi_ub) - roi_lb) as usize;
    for t in (start + 1)..(end + 1) {
        if unsafe { *depth.get_unchecked(t) } > 0f64 {
            return t as i64 + roi_lb;
        }
    }
    INVALID_MAX
}

impl ROIVectorMarketDepth {
    /// Constructs an instance of `ROIVectorMarketDepth`.
    pub fn new(tick_size: f64, lot_size: f64, roi_lb: f64, roi_ub: f64) -> Self {
        let roi_lb = (roi_lb / tick_size).round() as i64;
        let roi_ub = (roi_ub / tick_size).round() as i64;
        let roi_range = (roi_ub + 1 - roi_lb) as usize;
        Self {
            tick_size,
            lot_size,
            timestamp: 0,
            ask_depth: {
                let mut v = (0..roi_range).map(|_| 0.0).collect::<Vec<_>>();
                v.shrink_to_fit();
                v
            },
            bid_depth: {
                let mut v = (0..roi_range).map(|_| 0.0).collect::<Vec<_>>();
                v.shrink_to_fit();
                v
            },
            best_bid_tick: INVALID_MIN,
            best_ask_tick: INVALID_MAX,
            low_bid_tick: INVALID_MAX,
            high_ask_tick: INVALID_MIN,
            roi_lb,
            roi_ub,
            orders: HashMap::new(),
        }
    }

    fn add(&mut self, order: L3Order) -> Result<(), BacktestError> {
        let order = match self.orders.entry(order.order_id) {
            Entry::Occupied(_) => return Err(BacktestError::OrderIdExist),
            Entry::Vacant(entry) => entry.insert(order),
        };
        if order.price_tick < self.roi_lb || order.price_tick > self.roi_ub {
            // This is outside the range of interest.
            return Ok(());
        }
        let t = (order.price_tick - self.roi_lb) as usize;
        if order.side == Side::Buy {
            unsafe {
                *self.bid_depth.get_unchecked_mut(t) += order.qty;
            }
        } else {
            unsafe {
                *self.ask_depth.get_unchecked_mut(t) += order.qty;
            }
        }
        Ok(())
    }

    /// Returns the bid market depth array, which contains the quantity at each price. Its length is
    /// `ROI upper bound in ticks + 1 - ROI lower bound in ticks`, the array contains the quantities
    /// at prices from the ROI lower bound to the ROI upper bound.
    /// The index is calculated as `price in ticks - ROI lower bound in ticks`.
    /// Respectively, the price is `(index + ROI lower bound in ticks) * tick_size`.
    pub fn bid_depth(&self) -> &[f64] {
        self.bid_depth.as_slice()
    }

    /// Returns the ask market depth array, which contains the quantity at each price. Its length is
    /// `ROI upper bound in ticks + 1 - ROI lower bound in ticks`, the array contains the quantities
    /// at prices from the ROI lower bound to the ROI upper bound.
    /// The index is calculated as `price in ticks - ROI lower bound in ticks`.
    /// Respectively, the price is `(index + ROI lower bound in ticks) * tick_size`.
    pub fn ask_depth(&self) -> &[f64] {
        self.ask_depth.as_slice()
    }

    /// Returns the lower and the upper bound of the range of interest, in price.
    pub fn roi(&self) -> (f64, f64) {
        (
            self.roi_lb as f64 * self.tick_size,
            self.roi_ub as f64 * self.tick_size,
        )
    }

    /// Returns the lower and the upper bound of the range of interest, in ticks.
    pub fn roi_tick(&self) -> (i64, i64) {
        (self.roi_lb, self.roi_ub)
    }
}

impl L2MarketDepth for ROIVectorMarketDepth {
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

        if price_tick < self.roi_lb || price_tick > self.roi_ub {
            // This is outside the range of interest.
            return (
                price_tick,
                prev_best_bid_tick,
                self.best_bid_tick,
                0.0,
                qty,
                timestamp,
            );
        }
        let t = (price_tick - self.roi_lb) as usize;
        unsafe {
            let v = self.bid_depth.get_unchecked_mut(t);
            prev_qty = *v;
            *v = qty;
        }

        if qty_lot == 0 {
            if price_tick == self.best_bid_tick {
                self.best_bid_tick = depth_below(
                    &self.bid_depth,
                    self.best_bid_tick,
                    self.low_bid_tick,
                    self.roi_lb,
                    self.roi_ub,
                );
                if self.best_bid_tick == INVALID_MIN {
                    self.low_bid_tick = INVALID_MAX
                }
            }
        } else {
            if price_tick > self.best_bid_tick {
                self.best_bid_tick = price_tick;
                if self.best_bid_tick >= self.best_ask_tick {
                    self.best_ask_tick = depth_above(
                        &self.ask_depth,
                        self.best_bid_tick,
                        self.high_ask_tick,
                        self.roi_lb,
                        self.roi_ub,
                    );
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

        if price_tick < self.roi_lb || price_tick > self.roi_ub {
            // This is outside the range of interest.
            return (
                price_tick,
                prev_best_ask_tick,
                self.best_ask_tick,
                0.0,
                qty,
                timestamp,
            );
        }
        let t = (price_tick - self.roi_lb) as usize;
        unsafe {
            let v = self.ask_depth.get_unchecked_mut(t);
            prev_qty = *v;
            *v = qty;
        }

        if qty_lot == 0 {
            if price_tick == self.best_ask_tick {
                self.best_ask_tick = depth_above(
                    &self.ask_depth,
                    self.best_ask_tick,
                    self.high_ask_tick,
                    self.roi_lb,
                    self.roi_ub,
                );
                if self.best_ask_tick == INVALID_MAX {
                    self.high_ask_tick = INVALID_MIN
                }
            }
        } else {
            if price_tick < self.best_ask_tick {
                self.best_ask_tick = price_tick;
                if self.best_bid_tick >= self.best_ask_tick {
                    self.best_bid_tick = depth_below(
                        &self.bid_depth,
                        self.best_ask_tick,
                        self.low_bid_tick,
                        self.roi_lb,
                        self.roi_ub,
                    );
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
                        let from = (clear_upto - self.roi_lb).max(0);
                        let to = self.best_bid_tick + 1 - self.roi_lb;
                        for t in from..to {
                            unsafe {
                                *self.bid_depth.get_unchecked_mut(t as usize) = 0.0;
                            }
                        }
                    }
                    let low_bid_tick = if self.low_bid_tick == INVALID_MAX {
                        self.roi_lb
                    } else {
                        self.low_bid_tick
                    };
                    let clear_upto = if clear_upto - 1 < self.roi_lb {
                        self.roi_lb
                    } else if clear_upto - 1 > self.roi_ub {
                        self.roi_ub
                    } else {
                        clear_upto - 1
                    };
                    self.best_bid_tick = depth_below(
                        &self.bid_depth,
                        clear_upto,
                        low_bid_tick,
                        self.roi_lb,
                        self.roi_ub,
                    );
                } else {
                    self.bid_depth.iter_mut().for_each(|q| *q = 0.0);
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
                        let from = self.best_ask_tick - self.roi_lb;
                        let to = (clear_upto + 1 - self.roi_ub).min(self.ask_depth.len() as i64);
                        for t in from..to {
                            unsafe {
                                *self.ask_depth.get_unchecked_mut(t as usize) = 0.0;
                            }
                        }
                    }
                    let high_ask_tick = if self.high_ask_tick == INVALID_MIN {
                        self.roi_ub
                    } else {
                        self.high_ask_tick
                    };
                    let clear_upto = if clear_upto + 1 < self.roi_lb {
                        self.roi_lb
                    } else if clear_upto + 1 > self.roi_ub {
                        self.roi_ub
                    } else {
                        clear_upto + 1
                    };
                    self.best_ask_tick = depth_above(
                        &self.ask_depth,
                        clear_upto,
                        high_ask_tick,
                        self.roi_lb,
                        self.roi_ub,
                    );
                } else {
                    self.ask_depth.iter_mut().for_each(|q| *q = 0.0);
                    self.best_ask_tick = INVALID_MAX;
                }
                if self.best_ask_tick == INVALID_MAX {
                    self.high_ask_tick = INVALID_MIN;
                }
            }
            Side::None => {
                self.bid_depth.iter_mut().for_each(|q| *q = 0.0);
                self.ask_depth.iter_mut().for_each(|q| *q = 0.0);
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

impl MarketDepth for ROIVectorMarketDepth {
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
        if self.best_bid_tick < self.roi_lb || self.best_bid_tick > self.roi_ub {
            // This is outside the range of interest.
            0.0
        } else {
            unsafe {
                *self
                    .bid_depth
                    .get_unchecked((self.best_bid_tick - self.roi_lb) as usize)
            }
        }
    }

    #[inline(always)]
    fn best_ask_qty(&self) -> f64 {
        if self.best_ask_tick < self.roi_lb || self.best_ask_tick > self.roi_ub {
            // This is outside the range of interest.
            f64::NAN
        } else {
            unsafe {
                *self
                    .ask_depth
                    .get_unchecked((self.best_ask_tick - self.roi_lb) as usize)
            }
        }
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
        if price_tick < self.roi_lb || price_tick > self.roi_ub {
            // This is outside the range of interest.
            f64::NAN
        } else {
            unsafe {
                *self
                    .bid_depth
                    .get_unchecked((price_tick - self.roi_lb) as usize)
            }
        }
    }

    #[inline(always)]
    fn ask_qty_at_tick(&self, price_tick: i64) -> f64 {
        if price_tick < self.roi_lb || price_tick > self.roi_ub {
            // This is outside the range of interest.
            f64::NAN
        } else {
            unsafe {
                *self
                    .ask_depth
                    .get_unchecked((price_tick - self.roi_lb) as usize)
            }
        }
    }
}

impl ApplySnapshot for ROIVectorMarketDepth {
    fn apply_snapshot(&mut self, data: &Data<Event>) {
        self.best_bid_tick = INVALID_MIN;
        self.best_ask_tick = INVALID_MAX;
        self.low_bid_tick = INVALID_MAX;
        self.high_ask_tick = INVALID_MIN;
        for qty in &mut self.bid_depth {
            *qty = 0.0;
        }
        for qty in &mut self.ask_depth {
            *qty = 0.0;
        }
        for row_num in 0..data.len() {
            let price = data[row_num].px;
            let qty = data[row_num].qty;

            let price_tick = (price / self.tick_size).round() as i64;
            if price_tick < self.roi_lb || price_tick > self.roi_ub {
                continue;
            }
            if data[row_num].ev & BUY_EVENT == BUY_EVENT {
                self.best_bid_tick = self.best_bid_tick.max(price_tick);
                self.low_bid_tick = self.low_bid_tick.min(price_tick);
                let t = (price_tick - self.roi_lb) as usize;
                unsafe {
                    *self.bid_depth.get_unchecked_mut(t) = qty;
                }
            } else if data[row_num].ev & SELL_EVENT == SELL_EVENT {
                self.best_ask_tick = self.best_ask_tick.min(price_tick);
                self.high_ask_tick = self.high_ask_tick.max(price_tick);
                let t = (price_tick - self.roi_lb) as usize;
                unsafe {
                    *self.ask_depth.get_unchecked_mut(t) = qty;
                }
            }
        }
    }

    fn snapshot(&self) -> Vec<Event> {
        unimplemented!();
    }
}

impl L3MarketDepth for ROIVectorMarketDepth {
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
                self.best_ask_tick = depth_above(
                    &self.ask_depth,
                    self.best_bid_tick,
                    self.high_ask_tick,
                    self.roi_lb,
                    self.roi_ub,
                );
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
                self.best_bid_tick = depth_below(
                    &self.bid_depth,
                    self.best_ask_tick,
                    self.low_bid_tick,
                    self.roi_lb,
                    self.roi_ub,
                );
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

            if !(order.price_tick < self.roi_lb || order.price_tick > self.roi_ub) {
                let t = (order.price_tick - self.roi_lb) as usize;
                let depth_qty = unsafe { self.bid_depth.get_unchecked_mut(t) };
                *depth_qty -= order.qty;
                if (*depth_qty / self.lot_size).round() as i64 == 0 {
                    *depth_qty = 0.0;
                    if order.price_tick == self.best_bid_tick {
                        self.best_bid_tick = depth_below(
                            &self.bid_depth,
                            self.best_bid_tick,
                            self.low_bid_tick,
                            self.roi_lb,
                            self.roi_ub,
                        );
                        if self.best_bid_tick == INVALID_MIN {
                            self.low_bid_tick = INVALID_MAX
                        }
                    }
                }
            }
            Ok((Side::Buy, prev_best_tick, self.best_bid_tick))
        } else {
            let prev_best_tick = self.best_ask_tick;

            if !(order.price_tick < self.roi_lb || order.price_tick > self.roi_ub) {
                let t = (order.price_tick - self.roi_lb) as usize;
                let depth_qty = unsafe { self.ask_depth.get_unchecked_mut(t) };
                *depth_qty -= order.qty;
                if (*depth_qty / self.lot_size).round() as i64 == 0 {
                    *depth_qty = 0.0;
                    if order.price_tick == self.best_ask_tick {
                        self.best_ask_tick = depth_above(
                            &self.ask_depth,
                            self.best_ask_tick,
                            self.high_ask_tick,
                            self.roi_lb,
                            self.roi_ub,
                        );
                        if self.best_ask_tick == INVALID_MAX {
                            self.high_ask_tick = INVALID_MIN
                        }
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
                if !(order.price_tick < self.roi_lb || order.price_tick > self.roi_ub) {
                    let t = (order.price_tick - self.roi_lb) as usize;
                    let depth_qty = unsafe { self.bid_depth.get_unchecked_mut(t) };
                    *depth_qty -= order.qty;
                    if (*depth_qty / self.lot_size).round() as i64 == 0 {
                        *depth_qty = 0.0;
                        if order.price_tick == self.best_bid_tick {
                            self.best_bid_tick = depth_below(
                                &self.bid_depth,
                                self.best_bid_tick,
                                self.low_bid_tick,
                                self.roi_lb,
                                self.roi_ub,
                            );
                            if self.best_bid_tick == INVALID_MIN {
                                self.low_bid_tick = INVALID_MAX
                            }
                        }
                    }
                }

                order.price_tick = price_tick;
                order.qty = qty;
                order.timestamp = timestamp;

                if !(price_tick < self.roi_lb || price_tick > self.roi_ub) {
                    let t = (price_tick - self.roi_lb) as usize;
                    let depth_qty = unsafe { self.bid_depth.get_unchecked_mut(t) };
                    *depth_qty += order.qty;

                    if price_tick > self.best_bid_tick {
                        self.best_bid_tick = price_tick;
                        if self.best_bid_tick >= self.best_ask_tick {
                            self.best_ask_tick = depth_above(
                                &self.ask_depth,
                                self.best_bid_tick,
                                self.high_ask_tick,
                                self.roi_lb,
                                self.roi_ub,
                            );
                        }
                    }
                    self.low_bid_tick = self.low_bid_tick.min(price_tick);
                }
                Ok((Side::Buy, prev_best_tick, self.best_bid_tick))
            } else {
                if !(order.price_tick < self.roi_lb || order.price_tick > self.roi_ub) {
                    let t = (order.price_tick - self.roi_lb) as usize;
                    let depth_qty = unsafe { self.bid_depth.get_unchecked_mut(t) };
                    *depth_qty += qty - order.qty;
                }
                order.qty = qty;
                Ok((Side::Buy, self.best_bid_tick, self.best_bid_tick))
            }
        } else {
            let prev_best_tick = self.best_ask_tick;
            let price_tick = (px / self.tick_size).round() as i64;
            if price_tick != order.price_tick {
                if !(order.price_tick < self.roi_lb || order.price_tick > self.roi_ub) {
                    let t = (order.price_tick - self.roi_lb) as usize;
                    let depth_qty = unsafe { self.ask_depth.get_unchecked_mut(t) };
                    *depth_qty -= order.qty;
                    if (*depth_qty / self.lot_size).round() as i64 == 0 {
                        *depth_qty = 0.0;
                        if order.price_tick == self.best_ask_tick {
                            self.best_ask_tick = depth_above(
                                &self.ask_depth,
                                self.best_ask_tick,
                                self.high_ask_tick,
                                self.roi_lb,
                                self.roi_ub,
                            );
                            if self.best_ask_tick == INVALID_MAX {
                                self.high_ask_tick = INVALID_MIN
                            }
                        }
                    }
                }

                order.price_tick = price_tick;
                order.qty = qty;
                order.timestamp = timestamp;

                if !(price_tick < self.roi_lb || price_tick > self.roi_ub) {
                    let t = (price_tick - self.roi_lb) as usize;
                    let depth_qty = unsafe { self.ask_depth.get_unchecked_mut(t) };
                    *depth_qty += order.qty;

                    if price_tick < self.best_ask_tick {
                        self.best_ask_tick = price_tick;
                        if self.best_bid_tick >= self.best_ask_tick {
                            self.best_bid_tick = depth_below(
                                &self.bid_depth,
                                self.best_ask_tick,
                                self.low_bid_tick,
                                self.roi_lb,
                                self.roi_ub,
                            );
                        }
                    }
                    self.high_ask_tick = self.high_ask_tick.max(price_tick);
                }
                Ok((Side::Sell, prev_best_tick, self.best_ask_tick))
            } else {
                if !(order.price_tick < self.roi_lb || order.price_tick > self.roi_ub) {
                    let t = (order.price_tick - self.roi_lb) as usize;
                    let depth_qty = unsafe { self.ask_depth.get_unchecked_mut(t) };
                    *depth_qty += qty - order.qty;
                }
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
        depth::{INVALID_MAX, INVALID_MIN, L3MarketDepth, MarketDepth, ROIVectorMarketDepth},
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
        let mut depth = ROIVectorMarketDepth::new(0.1, lot_size, 0.0, 2000.0);

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
        let mut depth = ROIVectorMarketDepth::new(0.1, lot_size, 0.0, 2000.0);

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
        let mut depth = ROIVectorMarketDepth::new(0.1, lot_size, 0.0, 2000.0);

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
        let mut depth = ROIVectorMarketDepth::new(0.1, lot_size, 0.0, 2000.0);

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
