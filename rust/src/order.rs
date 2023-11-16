use std::cell::RefCell;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum Side {
    Buy = 1,
    Sell = -1
}

impl Side {
    pub fn as_f64(&self) -> f64 {
        match self {
            Side::Buy => 1f64,
            Side::Sell => -1f64
        }
    }

    pub fn as_f32(&self) -> f32 {
        match self {
            Side::Buy => 1f32,
            Side::Sell => -1f32
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum Status {
    None = 0,
    New = 1,
    Expired = 2,
    Filled = 3,
    Canceled = 4,
    PartiallyFilled = 5
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum TimeInForce {
    GTC = 0,
    GTX = 1,
    FOK = 2,
    IOC = 3,
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum OrdType {
    Limit = 0,
    Market = 1,
}

#[derive(Clone, Debug)]
pub struct Order<Q> where Q: Sized + Clone {
    pub qty: f32,
    pub leaves_qty: f32,
    pub price_tick: i32,
    pub tick_size: f32,
    pub side: Side,
    pub time_in_force: TimeInForce,
    pub exch_timestamp: i64,
    pub status: Status,
    pub local_timestamp: i64,
    pub req: Status,
    pub exec_price_tick: i32,
    pub exec_qty: f32,
    pub order_id: i64,
    pub q: Q,
    pub maker: bool,
    pub order_type: OrdType
}

impl<Q> Order<Q> where Q: Clone + Default {
    pub fn new(
        order_id: i64,
        price_tick: i32,
        tick_size: f32,
        qty: f32,
        side: Side,
        order_type: OrdType,
        time_in_force: TimeInForce,
    ) -> Self {
        Self {
            qty,
            leaves_qty: qty,
            price_tick,
            tick_size,
            side,
            time_in_force,
            exch_timestamp: 0,
            status: Status::None,
            local_timestamp: 0,
            req: Status::None,
            exec_price_tick: 0,
            exec_qty: 0.0,
            order_id,
            q: Q::default(),
            maker: false,
            order_type,
        }
    }

    pub fn price(&self) -> f32 {
        self.price_tick as f32 * self.tick_size
    }

    pub fn exec_price(&self) -> f32 {
        self.exec_price_tick as f32 * self.tick_size
    }

    pub fn cancellable(&self) -> bool {
        self.status == Status::New && self.req == Status::None
    }
}

#[derive(Clone, Debug)]
pub struct OrderBus<Q> where Q: Clone {
    order_list: Rc<RefCell<Vec<(Order<Q>, i64)>>>,
    orders: Rc<RefCell<HashMap<i64, i64>>>,
}

impl<Q> OrderBus<Q> where Q: Clone {
    pub fn new() -> Self {
        Self {
            order_list: Default::default(),
            orders: Default::default(),
        }
    }

    pub fn frontmost_timestamp(&self) -> i64 {
        self.order_list.borrow().get(0).map(|(_order, ts)| *ts).unwrap_or(i64::MAX)
    }

    pub fn append(&mut self, order: Order<Q>, timestamp: i64) {
        *self.orders.borrow_mut().entry(order.order_id).or_insert(0) += 1;
        self.order_list.borrow_mut().push((order, timestamp));
    }

    pub fn get_head_timestamp(&self) -> Option<i64> {
        if let Some((_order, recv_ts)) = self.order_list.borrow().get(0) {
            Some(*recv_ts)
        } else {
            None
        }
    }

    pub fn get(&self, order_id: i64) -> Option<i64> {
        for (order, recv_ts) in self.order_list.borrow().iter() {
            if order.order_id == order_id {
                return Some(*recv_ts);
            }
        }
        None
    }

    pub fn reset(&mut self) {
        self.order_list.borrow_mut().clear();
        self.orders.borrow_mut().clear();
    }

    pub fn len(&self) -> usize {
        self.order_list.borrow().len()
    }

    pub fn remove(&mut self, index: usize) -> Order<Q> {
        let (order, _) = self.order_list.borrow_mut().remove(index);
        if let Entry::Occupied(mut entry) = self.orders.borrow_mut().entry(order.order_id) {
            let value = entry.get_mut();
            *value -= 1;
            if *value <= 0 {
                entry.remove();
            }
        }
        order
    }

    pub fn contains_key(&self, order_id: i64) -> bool {
        self.orders.borrow().contains_key(&order_id)
    }
}