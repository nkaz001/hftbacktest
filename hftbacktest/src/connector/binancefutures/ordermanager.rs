use std::{
    collections::{hash_map::Entry, HashMap},
    sync::{Arc, Mutex},
};

use chrono::Utc;
use tracing::{debug, error};

use crate::{
    connector::{
        binancefutures::{msg::rest::OrderResponse, BinanceFuturesError},
        util::gen_random_string,
    },
    prelude::OrderId,
    types::{Order, Status},
};

#[derive(Debug)]
struct OrderWrapper {
    asset_no: usize,
    order: Order,
    removed_by_ws: bool,
    removed_by_rest: bool,
}

pub type OrderManagerWrapper = Arc<Mutex<OrderManager>>;

/// Binance has separated channels for REST APIs and Websocket. Order responses are delivered
/// through these channels, with no guaranteed order of transmission. To prevent duplicate handling
/// of order responses, such as order deletion due to cancellation or fill, OrderManager manages the
/// order states before transmitting the responses to a live bot.
#[derive(Default, Debug)]
pub struct OrderManager {
    prefix: String,
    orders: HashMap<String, OrderWrapper>,
    order_id_map: HashMap<OrderId, String>,
}

impl OrderManager {
    pub fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
            orders: Default::default(),
            order_id_map: Default::default(),
        }
    }

    pub fn update_from_ws(
        &mut self,
        asset_no: usize,
        client_order_id: String,
        order: Order,
    ) -> Option<Order> {
        match self.orders.entry(client_order_id.clone()) {
            Entry::Occupied(mut entry) => {
                let wrapper = entry.get_mut();
                let already_removed = wrapper.removed_by_ws || wrapper.removed_by_rest;
                if order.exch_timestamp >= wrapper.order.exch_timestamp {
                    wrapper.order.update(&order);
                }

                if order.status != Status::New && order.status != Status::PartiallyFilled {
                    wrapper.removed_by_ws = true;
                    if !already_removed {
                        self.order_id_map.remove(&order.order_id);
                    }

                    if wrapper.removed_by_ws && wrapper.removed_by_rest {
                        entry.remove_entry();
                    }
                }

                if already_removed {
                    None
                } else {
                    Some(order)
                }
            }
            Entry::Vacant(entry) => {
                if !order.active() {
                    return None;
                }

                debug!(
                    %client_order_id,
                    ?order,
                    "BinanceFutures OrderManager received an unmanaged order from WS."
                );
                let wrapper = entry.insert(OrderWrapper {
                    asset_no,
                    order: order.clone(),
                    removed_by_ws: order.status != Status::New
                        && order.status != Status::PartiallyFilled,
                    removed_by_rest: false,
                });
                if wrapper.removed_by_ws || wrapper.removed_by_rest {
                    self.order_id_map.remove(&order.order_id);
                }
                Some(order)
            }
        }
    }

    pub fn update_submit_success(
        &mut self,
        asset_no: usize,
        order: Order,
        resp: OrderResponse,
    ) -> Option<Order> {
        let order = Order {
            qty: resp.orig_qty,
            leaves_qty: resp.orig_qty - resp.cum_qty,
            price_tick: (resp.price / order.tick_size).round() as i64,
            tick_size: order.tick_size,
            side: order.side,
            time_in_force: resp.time_in_force,
            exch_timestamp: resp.update_time * 1_000_000,
            status: Status::New,
            local_timestamp: 0,
            req: Status::None,
            exec_price_tick: 0,
            exec_qty: resp.executed_qty,
            order_id: order.order_id,
            order_type: resp.ty,
            // Invalid information
            q: Box::new(()),
            maker: false,
        };
        self.update_from_rest(asset_no, resp.client_order_id, order)
    }

    pub fn update_submit_fail(
        &mut self,
        asset_no: usize,
        mut order: Order,
        error: &BinanceFuturesError,
        client_order_id: String,
    ) -> Option<Order> {
        match error {
            BinanceFuturesError::OrderError { code: -5022, .. } => {
                // GTX rejection.
            }
            BinanceFuturesError::OrderError { code: -1008, .. } => {
                // Server is currently overloaded with other requests. Please try again in a few minutes.
                error!("Server is currently overloaded with other requests. Please try again in a few minutes.");
            }
            BinanceFuturesError::OrderError { code: -2019, .. } => {
                // Margin is insufficient.
                error!("Margin is insufficient.");
            }
            BinanceFuturesError::OrderError { code: -1015, .. } => {
                // Too many new orders; current limit is 300 orders per TEN_SECONDS."
                error!("Too many new orders; current limit is 300 orders per TEN_SECONDS.");
            }
            error => {
                error!(?error, "submit error");
            }
        }

        order.req = Status::None;
        order.status = Status::Expired;
        self.update_from_rest(asset_no, client_order_id, order)
    }

    pub fn update_cancel_success(
        &mut self,
        asset_no: usize,
        mut order: Order,
        resp: OrderResponse,
    ) -> Option<Order> {
        let order = Order {
            qty: resp.orig_qty,
            leaves_qty: resp.orig_qty - resp.cum_qty,
            price_tick: (resp.price / order.tick_size).round() as i64,
            tick_size: order.tick_size,
            side: resp.side,
            time_in_force: resp.time_in_force,
            exch_timestamp: resp.update_time * 1_000_000,
            status: Status::Canceled,
            local_timestamp: 0,
            req: Status::None,
            exec_price_tick: 0,
            exec_qty: resp.executed_qty,
            order_id: order.order_id,
            order_type: resp.ty,
            // Invalid information
            q: Box::new(()),
            maker: false,
        };
        self.update_from_rest(asset_no, resp.client_order_id, order)
    }

    pub fn update_cancel_fail(
        &mut self,
        asset_no: usize,
        mut order: Order,
        error: &BinanceFuturesError,
        client_order_id: String,
    ) -> Option<Order> {
        match error {
            BinanceFuturesError::OrderError { code: -2011, .. } => {
                // The given order may no longer exist; it could have already been filled or
                // canceled. But, it cannot determine the order status because it lacks the
                // necessary information.
                order.leaves_qty = 0.0;
                order.status = Status::None;
            }
            error => {
                error!(?error, "cancel error");
            }
        }
        order.req = Status::None;
        self.update_from_rest(asset_no, client_order_id, order)
    }

    fn update_from_rest(
        &mut self,
        asset_no: usize,
        client_order_id: String,
        order: Order,
    ) -> Option<Order> {
        match self.orders.entry(client_order_id.clone()) {
            Entry::Occupied(mut entry) => {
                let wrapper = entry.get_mut();
                let already_removed = wrapper.removed_by_ws || wrapper.removed_by_rest;
                if order.exch_timestamp >= wrapper.order.exch_timestamp {
                    wrapper.order.update(&order);
                }

                if order.status != Status::New && order.status != Status::PartiallyFilled {
                    wrapper.removed_by_rest = true;
                    if !already_removed {
                        self.order_id_map.remove(&order.order_id);
                    }

                    if wrapper.removed_by_ws && wrapper.removed_by_rest {
                        entry.remove_entry();
                    }
                }

                if already_removed {
                    None
                } else {
                    Some(order)
                }
            }
            Entry::Vacant(entry) => {
                if !order.active() {
                    return None;
                }

                debug!(
                    %client_order_id,
                    ?order,
                    "BinanceFutures OrderManager received an unmanaged order from REST."
                );
                let wrapper = entry.insert(OrderWrapper {
                    asset_no,
                    order: order.clone(),
                    removed_by_ws: false,
                    removed_by_rest: order.status != Status::New
                        && order.status != Status::PartiallyFilled,
                });
                if wrapper.removed_by_ws || wrapper.removed_by_rest {
                    self.order_id_map.remove(&order.order_id);
                }
                Some(order)
            }
        }
    }

    pub fn prepare_client_order_id(&mut self, asset_no: usize, order: Order) -> Option<String> {
        if self.order_id_map.contains_key(&order.order_id) {
            return None;
        }

        let rand_id = gen_random_string(16);

        let client_order_id = format!("{}{}{}", self.prefix, &rand_id, order.order_id);
        if self.orders.contains_key(&client_order_id) {
            return None;
        }

        self.order_id_map
            .insert(order.order_id, client_order_id.clone());
        self.orders.insert(
            client_order_id.clone(),
            OrderWrapper {
                asset_no,
                order,
                removed_by_ws: false,
                removed_by_rest: false,
            },
        );
        Some(client_order_id)
    }

    pub fn get_client_order_id(&self, order_id: OrderId) -> Option<String> {
        self.order_id_map.get(&order_id).cloned()
    }

    pub fn gc(&mut self) {
        let now = Utc::now().timestamp_nanos_opt().unwrap();
        let stale_ts = now - 300_000_000_000;
        let stale_ids: Vec<(_, _)> = self
            .orders
            .iter()
            .filter(|&(_, wrapper)| {
                wrapper.order.status != Status::New
                    && wrapper.order.status != Status::PartiallyFilled
                    && wrapper.order.status != Status::Unsupported
                    && wrapper.order.exch_timestamp < stale_ts
            })
            .map(|(client_order_id, wrapper)| (client_order_id.clone(), wrapper.order.order_id))
            .collect();
        for (client_order_id, order_id) in stale_ids.iter() {
            if self.order_id_map.contains_key(order_id) {
                // Something went wrong?
            }
            self.orders.remove(client_order_id);
        }
    }

    pub fn parse_client_order_id(client_order_id: &str, prefix: &str) -> Option<u64> {
        if !client_order_id.starts_with(prefix) {
            None
        } else {
            let s = &client_order_id[(prefix.len() + 16)..];
            if let Ok(order_id) = s.parse() {
                Some(order_id)
            } else {
                None
            }
        }
    }

    pub fn clear_orders(&mut self) -> Vec<(usize, Order)> {
        let mut values: Vec<(usize, Order)> = Vec::new();
        values.extend(self.orders.drain().map(|(_, mut order)| {
            order.order.status = Status::Canceled;
            (order.asset_no, order.order)
        }));
        values
    }
}
