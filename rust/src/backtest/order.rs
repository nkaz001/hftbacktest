use std::rc::Rc;
use std::cell::UnsafeCell;
use std::collections::VecDeque;

use crate::types::Order;

/// Provides a bus for transporting backtesting orders between the exchange and the local model
/// based on the given timestamp.
#[derive(Clone, Debug)]
pub struct OrderBus<Q>
where
    Q: Clone,
{
    order_list: Rc<UnsafeCell<VecDeque<(Order<Q>, i64)>>>,
}

impl<Q> OrderBus<Q>
where
    Q: Clone,
{
    /// Constructs an instance of `OrderBus`.
    pub fn new() -> Self {
        Self {
            order_list: Default::default(),
        }
    }

    /// Returns the timestamp of the earliest order in the bus.
    pub fn earliest_timestamp(&self) -> Option<i64> {
        unsafe { &*self.order_list.get() }.get(0).map(|(_order, ts)| *ts)
    }

    /// Appends the order to the bus with the timestamp.
    ///
    /// To prevent the timestamp of the order from becoming disordered, it enforces that the given
    /// timestamp must be equal to or greater than the latest timestamp in the bus.
    ///
    /// In crypto exchanges that use REST APIs, it may be still possible for order requests sent
    /// later to reach the matching engine before order requests sent earlier. However, for the
    /// purpose of simplifying the backtesting process, all requests and responses are assumed to be
    /// in order.
    pub fn append(&mut self, order: Order<Q>, timestamp: i64) {
        let latest_timestamp = {
            let order_list = unsafe { &*self.order_list.get() };
            let len = order_list.len();
            if len > 0 {
                let (_, timestamp) = order_list.get(len - 1).unwrap();
                *timestamp
            } else {
                0
            }
        };
        let timestamp = timestamp.max(latest_timestamp);
        unsafe { &mut *self.order_list.get() }.push_back((order, timestamp));
    }

    /// Resets this to clear it.
    pub fn reset(&mut self) {
        unsafe { &mut *self.order_list.get() }.clear();
    }

    /// Returns the number of orders in the bus.
    pub fn len(&self) -> usize {
        unsafe { &*self.order_list.get() }.len()
    }

    /// Removes the first order and its timestamp and returns it, or `None` if the bus is empty.
    pub fn pop_front(&mut self) -> Option<(Order<Q>, i64)> {
        unsafe { &mut *self.order_list.get() }.pop_front()
    }
}
