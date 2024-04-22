use std::{
    cell::RefCell,
    collections::{hash_map::Entry, HashMap},
    rc::Rc,
};

use crate::ty::Order;

/// Provides a bus for transporting backtesting orders between the exchange and the local model
/// based on the given timestamp.
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
    /// Constructs [`OrderBus`].
    pub fn new() -> Self {
        Self {
            order_list: Default::default(),
            orders: Default::default(),
        }
    }

    /// Returns the timestamp of the frontmost order in the bus.
    pub fn frontmost_timestamp(&self) -> Option<i64> {
        self.order_list.borrow().get(0).map(|(_order, ts)| *ts)
    }

    /// Appends the order to the bus with the timestamp.
    pub fn append(&mut self, order: Order<Q>, timestamp: i64) {
        *self.orders.borrow_mut().entry(order.order_id).or_insert(0) += 1;
        self.order_list.borrow_mut().push((order, timestamp));
    }

    /// Returns the timestamp of the given order id.
    pub fn get(&self, order_id: i64) -> Option<i64> {
        for (order, recv_ts) in self.order_list.borrow().iter() {
            if order.order_id == order_id {
                return Some(*recv_ts);
            }
        }
        None
    }

    /// Resets this to clear it.
    pub fn reset(&mut self) {
        self.order_list.borrow_mut().clear();
        self.orders.borrow_mut().clear();
    }

    /// Returns the number of orders in the bus.
    pub fn len(&self) -> usize {
        self.order_list.borrow().len()
    }

    /// Removes the order by the index.
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

    /// Checks if the order corresponding to the order id exists.
    pub fn contains_key(&self, order_id: i64) -> bool {
        self.orders.borrow().contains_key(&order_id)
    }
}
