use std::{
    cell::RefCell,
    collections::{hash_map::Entry, HashMap},
    rc::Rc,
};

use crate::ty::Order;

#[derive(Clone, Debug)]
pub struct OrderBus<Q>
where
    Q: Clone,
{
    order_list: Rc<RefCell<Vec<(Order<Q>, i64)>>>,
    orders: Rc<RefCell<HashMap<i64, i64>>>,
}

impl<Q> OrderBus<Q>
where
    Q: Clone,
{
    pub fn new() -> Self {
        Self {
            order_list: Default::default(),
            orders: Default::default(),
        }
    }

    pub fn frontmost_timestamp(&self) -> i64 {
        self.order_list
            .borrow()
            .get(0)
            .map(|(_order, ts)| *ts)
            .unwrap_or(i64::MAX)
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
