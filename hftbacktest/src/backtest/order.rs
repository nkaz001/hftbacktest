use std::{cell::UnsafeCell, collections::VecDeque, rc::Rc};

use crate::{backtest::models::LatencyModel, types::Order};

/// Provides a bus for transporting backtesting orders between the exchange and the local model
/// based on the given timestamp.
#[derive(Clone, Debug, Default)]
pub struct OrderBus {
    order_list: Rc<UnsafeCell<VecDeque<(Order, i64)>>>,
}

impl OrderBus {
    /// Constructs an instance of ``OrderBus``.
    pub fn new() -> Self {
        Default::default()
    }

    /// Returns the timestamp of the earliest order in the bus.
    pub fn earliest_timestamp(&self) -> Option<i64> {
        unsafe { &*self.order_list.get() }
            .front()
            .map(|(_order, ts)| *ts)
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
    pub fn append(&mut self, order: Order, timestamp: i64) {
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

    /// Returns ``true`` if the ``OrderBus`` is empty.
    pub fn is_empty(&self) -> bool {
        unsafe { &*self.order_list.get() }.is_empty()
    }

    /// Removes the first order and its timestamp and returns it, or ``None`` if the bus is empty.
    pub fn pop_front(&mut self) -> Option<(Order, i64)> {
        unsafe { &mut *self.order_list.get() }.pop_front()
    }
}

/// Provides a bidirectional order bus connecting the exchange to the local.
pub struct ExchToLocal<LM> {
    to_exch: OrderBus,
    to_local: OrderBus,
    order_latency: LM,
}

impl<LM> ExchToLocal<LM>
where
    LM: LatencyModel,
{
    /// Returns the timestamp of the earliest order to be received by the exchange from the local.
    pub fn earliest_recv_order_timestamp(&self) -> Option<i64> {
        self.to_exch.earliest_timestamp()
    }

    /// Returns the timestamp of the earliest order sent from the exchange to the local.
    pub fn earliest_send_order_timestamp(&self) -> Option<i64> {
        self.to_local.earliest_timestamp()
    }

    /// Responds to the local with the order processed by the exchange.
    pub fn respond(&mut self, order: Order) {
        let local_recv_timestamp =
            order.exch_timestamp + self.order_latency.response(order.exch_timestamp, &order);
        self.to_local.append(order, local_recv_timestamp);
    }

    /// Receives the order request from the local, which is expected to be received at
    /// `receipt_timestamp`.
    pub fn receive(&mut self, receipt_timestamp: i64) -> Option<Order> {
        if let Some(timestamp) = self.to_exch.earliest_timestamp() {
            if timestamp == receipt_timestamp {
                self.to_exch.pop_front().map(|(order, _)| order)
            } else {
                assert!(timestamp > receipt_timestamp);
                None
            }
        } else {
            None
        }
    }
}

/// Provides a bidirectional order bus connecting the local to the exchange.
pub struct LocalToExch<LM> {
    to_exch: OrderBus,
    to_local: OrderBus,
    order_latency: LM,
}

impl<LM> LocalToExch<LM>
where
    LM: LatencyModel,
{
    /// Returns the timestamp of the earliest order to be received by the local from the exchange.
    pub fn earliest_recv_order_timestamp(&self) -> Option<i64> {
        self.to_local.earliest_timestamp()
    }

    /// Returns the timestamp of the earliest order sent from the local to the exchange.
    pub fn earliest_send_order_timestamp(&self) -> Option<i64> {
        self.to_exch.earliest_timestamp()
    }

    /// Sends the order request to the exchange.
    /// If it is rejected before reaching the matching engine (as reflected in the order latency
    /// information), `reject` is invoked and the rejection response is appended to the local order
    /// bus.
    pub fn request<F>(&mut self, mut order: Order, mut reject: F)
    where
        F: FnMut(&mut Order),
    {
        let order_entry_latency = self.order_latency.entry(order.local_timestamp, &order);
        // Negative latency indicates that the order is rejected for technical reasons, and its
        // value represents the latency that the local experiences when receiving the rejection
        // notification.
        if order_entry_latency < 0 {
            // Rejects the order.
            reject(&mut order);
            let rej_recv_timestamp = order.local_timestamp - order_entry_latency;
            self.to_local.append(order, rej_recv_timestamp);
        } else {
            let exch_recv_timestamp = order.local_timestamp + order_entry_latency;
            self.to_exch.append(order, exch_recv_timestamp);
        }
    }

    /// Receives the order response from the exchange, which is expected to be received at
    /// `receipt_timestamp`.
    pub fn receive(&mut self, receipt_timestamp: i64) -> Option<Order> {
        if let Some(timestamp) = self.to_local.earliest_timestamp() {
            if timestamp == receipt_timestamp {
                self.to_local.pop_front().map(|(order, _)| order)
            } else {
                assert!(timestamp > receipt_timestamp);
                None
            }
        } else {
            None
        }
    }
}

/// Creates bidirectional order buses with the order latency model.
pub fn order_bus<LM>(order_latency: LM) -> (ExchToLocal<LM>, LocalToExch<LM>)
where
    LM: LatencyModel + Clone,
{
    let to_exch = OrderBus::new();
    let to_local = OrderBus::new();
    (
        ExchToLocal {
            to_exch: to_exch.clone(),
            to_local: to_local.clone(),
            order_latency: order_latency.clone(),
        },
        LocalToExch {
            to_exch,
            to_local,
            order_latency,
        },
    )
}
