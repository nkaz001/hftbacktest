use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use crate::depth::{INVALID_MAX, INVALID_MIN};
use crate::types::{Order, Side};

pub enum Error {
    NotExist
}

#[derive(PartialEq)]
pub enum OrderSource {
    MarketFeed,
    Backtesting,
}

pub struct L3Order {
    source: OrderSource,
    order_id: i64,
    priority: usize,
    order: Order<()>
}

type OrderWrapper = Rc<RefCell<L3Order>>;

pub struct L3MBOMarketDepth {
    pub tick_size: f32,
    pub lot_size: f32,
    pub timestamp: i64,
    pub bid_depth: BTreeMap<i32, f32>,
    pub ask_depth: BTreeMap<i32, f32>,
    pub orders: HashMap<i64, OrderWrapper>,
    pub priority: HashMap<i32, Vec<OrderWrapper>>,
}

impl L3MBOMarketDepth {
    pub fn add(&mut self, mut order: L3Order) {
        let priority = self.priority
            .entry(order.order.price_tick)
            .or_insert(Vec::new());
        if order.source == OrderSource::MarketFeed {
            if order.order.side == Side::Buy {
                *self.bid_depth
                    .entry(order.order.price_tick)
                    .or_insert(0.0) += order.order.qty;
            } else {
                *self.ask_depth
                    .entry(order.order.price_tick)
                    .or_insert(0.0) += order.order.qty;
            }
        }
        let order_priority = priority.len();
        order.priority = order_priority;
        let order_id = order.order_id;
        let order = Rc::new(RefCell::new(order));
        priority.push(order.clone());
        self.orders.insert(order_id, order);
    }

    pub fn delete(&mut self, order_id: i64) -> Result<(), Error> {
        let order = self.orders.remove(&order_id).ok_or(Error::NotExist)?;
        let order_ = order.borrow();
        if order_.source == OrderSource::MarketFeed {
            if order_.order.side == Side::Buy {
                let qty = self.bid_depth.get_mut(&order_.order.price_tick).unwrap();
                *qty -= order_.order.qty;
                if (*qty / self.lot_size).round() as i32 == 0 {
                    self.bid_depth.remove(&order_.order.price_tick).unwrap();
                }
            } else {
                let qty = self.ask_depth.get_mut(&order_.order.price_tick).unwrap();
                *qty -= order_.order.qty;
                if (*qty / self.lot_size).round() as i32 == 0 {
                    self.ask_depth.remove(&order_.order.price_tick).unwrap();
                }
            }
        }
        let order_priority = order_.priority;
        let price = order_.order.price_tick;
        let priority = self.priority.get_mut(&price).unwrap();
        priority.remove(order_priority);
        if priority.len() == 0 {
            self.priority.remove(&price);
        }
        Ok(())
    }

    pub fn execute(&mut self, order_id: i64) -> Result<Vec<OrderWrapper>, Error> {
        let order = self.orders.remove(&order_id).ok_or(Error::NotExist)?;
        let order_ = order.borrow();
        if order_.source == OrderSource::MarketFeed {
            if order_.order.side == Side::Buy {
                let qty = self.bid_depth.get_mut(&order_.order.price_tick).unwrap();
                *qty -= order_.order.qty;
                if (*qty / self.lot_size).round() as i32 == 0 {
                    self.bid_depth.remove(&order_.order.price_tick).unwrap();
                }
            } else {
                let qty = self.ask_depth.get_mut(&order_.order.price_tick).unwrap();
                *qty -= order_.order.qty;
                if (*qty / self.lot_size).round() as i32 == 0 {
                    self.ask_depth.remove(&order_.order.price_tick).unwrap();
                }
            }
        }
        let mut priority = self.priority
            .get_mut(&order_.order.price_tick)
            .unwrap();
        let mut filled = Vec::new();
        let mut i = 0;
        loop {
            let order_in_q = priority.get(i).unwrap().clone();
            let order_in_q_ = order_in_q.borrow();
            if order_in_q_.order_id == order_id {
                priority.remove(i);
                break;
            }
            if order_in_q_.source == OrderSource::Backtesting {
                let order_fill = priority.remove(i);
                filled.push(order_fill);
            } else {
                i += 1;
            }
        }
        Ok(filled)
    }

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