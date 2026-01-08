use std::sync::{Arc, Mutex};

use chrono::Utc;
use hashbrown::HashMap;
use hftbacktest::types::{Order, OrderId, Status};
use tracing::error;

use crate::{
    binancepapi::{
        BinancePapiError,
        msg::{rest::OrderResponse, stream::OrderTradeUpdate},
    },
    connector::GetOrders,
    utils::{RefSymbolOrderId, SymbolOrderId, generate_rand_string},
};

#[derive(Debug)]
struct OrderExt {
    symbol: String,
    order: Order,
    removed_by_ws: bool,
    removed_by_rest: bool,
}

pub type SharedOrderManager = Arc<Mutex<OrderManager>>;

pub type ClientOrderId = String;

/// Exchange order ID from Binance (different from bot's order_id which is price_tick)
pub type ExchangeOrderId = i64;

/// Order manager for PAPI connector
/// Handles order state management between REST and WebSocket channels
#[derive(Default, Debug)]
pub struct OrderManager {
    prefix: String,
    orders: HashMap<ClientOrderId, OrderExt>,
    order_id_map: HashMap<SymbolOrderId, ClientOrderId>,
    /// Maps (symbol, bot_order_id) -> exchange_order_id for fallback cancellation
    /// This allows us to cancel orders even if client_order_id mapping is lost
    exchange_order_id_map: HashMap<SymbolOrderId, ExchangeOrderId>,
}

impl OrderManager {
    pub fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
            orders: Default::default(),
            order_id_map: Default::default(),
            exchange_order_id_map: Default::default(),
        }
    }

    pub fn update_from_ws(
        &mut self,
        resp: &OrderTradeUpdate,
    ) -> Result<Option<Order>, BinancePapiError> {
        if !resp.order.client_order_id.starts_with(&self.prefix) {
            return Err(BinancePapiError::PrefixUnmatched);
        }
        let order_ext = self
            .orders
            .get_mut(&resp.order.client_order_id)
            .ok_or(BinancePapiError::OrderNotFound)?;

        let already_removed = order_ext.removed_by_ws || order_ext.removed_by_rest;
        if resp.transaction_time * 1_000_000 >= order_ext.order.exch_timestamp {
            order_ext.order.qty = resp.order.original_qty;
            order_ext.order.leaves_qty =
                resp.order.original_qty - resp.order.order_filled_accumulated_qty;
            order_ext.order.side = resp.order.side;
            order_ext.order.time_in_force = resp.order.time_in_force;
            order_ext.order.exch_timestamp = resp.transaction_time * 1_000_000;
            order_ext.order.status = resp.order.order_status;
            order_ext.order.exec_price_tick =
                (resp.order.last_filled_price / order_ext.order.tick_size).round() as i64;
            order_ext.order.exec_qty = resp.order.order_last_filled_qty;
            order_ext.order.order_type = resp.order.order_type;

            // Store exchange order ID for fallback cancellation
            let symbol_order_id = SymbolOrderId::new(
                order_ext.symbol.clone(),
                order_ext.order.order_id,
            );
            self.exchange_order_id_map
                .insert(symbol_order_id, resp.order.order_id);
        }

        let result = if already_removed {
            None
        } else {
            Some(order_ext.order.clone())
        };

        if order_ext.order.status != Status::New
            && order_ext.order.status != Status::PartiallyFilled
        {
            order_ext.removed_by_ws = true;
            if !already_removed {
                let symbol_order_id = RefSymbolOrderId::new(
                    &order_ext.symbol,
                    order_ext.order.order_id,
                );
                self.order_id_map.remove(&symbol_order_id);
                // Also remove from exchange_order_id_map when order is done
                self.exchange_order_id_map.remove(&symbol_order_id);
            }

            if order_ext.removed_by_ws && order_ext.removed_by_rest {
                self.orders.remove(&resp.order.client_order_id).unwrap();
            }
        }

        Ok(result)
    }

    pub fn update_submit_fail(
        &mut self,
        client_order_id: &ClientOrderId,
        error: &BinancePapiError,
    ) -> Option<Order> {
        match error {
            BinancePapiError::OrderError { code: -5022, .. } => {
                // GTX rejection.
            }
            BinancePapiError::OrderError { code: -1008, .. } => {
                error!(
                    "Server is currently overloaded with other requests. Please try again in a few minutes."
                );
            }
            BinancePapiError::OrderError { code: -2019, .. } => {
                error!("Margin is insufficient.");
            }
            BinancePapiError::OrderError { code: -1015, .. } => {
                error!("Too many new orders; current limit is 300 orders per TEN_SECONDS.");
            }
            error => {
                error!(?error, "submit error");
            }
        }
        self.update_from_rest_fail(client_order_id, Some(Status::Expired))
    }

    pub fn update_cancel_fail(
        &mut self,
        client_order_id: &ClientOrderId,
        error: &BinancePapiError,
    ) -> Option<Order> {
        match error {
            BinancePapiError::OrderError { code: -2011, .. } => {
                self.update_from_rest_fail(client_order_id, Some(Status::None))
            }
            error => {
                error!(?error, "cancel error");
                self.update_from_rest_fail(client_order_id, None)
            }
        }
    }

    pub fn update_from_rest_fail(
        &mut self,
        client_order_id: &ClientOrderId,
        status: Option<Status>,
    ) -> Option<Order> {
        let order_ext = self.orders.get_mut(client_order_id)?;

        let already_removed = order_ext.removed_by_ws || order_ext.removed_by_rest;
        if let Some(status) = status {
            order_ext.order.status = status;
        }
        order_ext.order.req = Status::None;

        let result = if already_removed {
            None
        } else {
            Some(order_ext.order.clone())
        };

        if order_ext.order.status != Status::New
            && order_ext.order.status != Status::PartiallyFilled
        {
            order_ext.removed_by_rest = true;
            if !already_removed {
                self.order_id_map.remove(&RefSymbolOrderId::new(
                    &order_ext.symbol,
                    order_ext.order.order_id,
                ));
            }

            if order_ext.removed_by_ws && order_ext.removed_by_rest {
                self.orders.remove(client_order_id).unwrap();
            }
        }

        result
    }

    pub fn update_from_rest(
        &mut self,
        client_order_id: &ClientOrderId,
        resp: &OrderResponse,
    ) -> Option<Order> {
        let order_ext = self.orders.get_mut(client_order_id)?;

        let already_removed = order_ext.removed_by_ws || order_ext.removed_by_rest;
        if resp.update_time * 1_000_000 >= order_ext.order.exch_timestamp {
            order_ext.order.qty = resp.orig_qty;
            order_ext.order.leaves_qty = resp.orig_qty - resp.cum_qty;
            order_ext.order.side = resp.side;
            order_ext.order.time_in_force = resp.time_in_force;
            order_ext.order.exch_timestamp = resp.update_time * 1_000_000;
            order_ext.order.status = resp.status;
            order_ext.order.exec_qty = resp.executed_qty;
            order_ext.order.order_type = resp.ty;
            order_ext.order.req = Status::None;

            // Store exchange order ID for fallback cancellation
            let symbol_order_id = SymbolOrderId::new(
                order_ext.symbol.clone(),
                order_ext.order.order_id,
            );
            self.exchange_order_id_map
                .insert(symbol_order_id, resp.order_id);
        }

        let result = if already_removed {
            None
        } else {
            Some(order_ext.order.clone())
        };

        if order_ext.order.status != Status::New
            && order_ext.order.status != Status::PartiallyFilled
        {
            order_ext.removed_by_rest = true;
            if !already_removed {
                let symbol_order_id = RefSymbolOrderId::new(
                    &order_ext.symbol,
                    order_ext.order.order_id,
                );
                self.order_id_map.remove(&symbol_order_id);
                // Also remove from exchange_order_id_map when order is done
                self.exchange_order_id_map.remove(&symbol_order_id);
            }

            if order_ext.removed_by_ws && order_ext.removed_by_rest {
                self.orders.remove(client_order_id).unwrap();
            }
        }

        result
    }

    pub fn prepare_client_order_id(&mut self, symbol: String, order: Order) -> Option<String> {
        let symbol_order_id = SymbolOrderId::new(symbol.clone(), order.order_id);
        if self.order_id_map.contains_key(&symbol_order_id) {
            return None;
        }

        let client_order_id = format!("{}{}", self.prefix, generate_rand_string(16));
        if self.orders.contains_key(&client_order_id) {
            return None;
        }

        self.order_id_map
            .insert(symbol_order_id, client_order_id.clone());
        self.orders.insert(
            client_order_id.clone(),
            OrderExt {
                symbol,
                order,
                removed_by_ws: false,
                removed_by_rest: false,
            },
        );
        Some(client_order_id)
    }

    pub fn get_client_order_id(&self, symbol: &str, order_id: OrderId) -> Option<String> {
        self.order_id_map
            .get(&RefSymbolOrderId::new(symbol, order_id))
            .cloned()
    }

    /// Get exchange order ID for fallback cancellation when client_order_id is not found
    pub fn get_exchange_order_id(&self, symbol: &str, order_id: OrderId) -> Option<ExchangeOrderId> {
        self.exchange_order_id_map
            .get(&RefSymbolOrderId::new(symbol, order_id))
            .copied()
    }

    /// Remove exchange order ID mapping (called after successful cancel)
    pub fn remove_exchange_order_id(&mut self, symbol: &str, order_id: OrderId) {
        self.exchange_order_id_map
            .remove(&RefSymbolOrderId::new(symbol, order_id));
    }

    /// Garbage collection for stale orders
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
            .map(|(client_order_id, wrapper)| {
                (
                    client_order_id.clone(),
                    SymbolOrderId::new(wrapper.symbol.clone(), wrapper.order.order_id),
                )
            })
            .collect();
        for (client_order_id, order_id) in stale_ids.iter() {
            if self.order_id_map.contains_key(order_id) {
                self.order_id_map.remove(order_id).unwrap();
            }
            self.orders.remove(client_order_id);
        }
    }

    pub fn cancel_all_from_rest(&mut self, symbol: &str) -> Vec<Order> {
        let mut removed_orders = Vec::new();
        let mut removed_order_ids = Vec::new();
        for (client_order_id, order_ext) in &mut self.orders {
            if order_ext.symbol != symbol {
                continue;
            }
            let already_removed = order_ext.removed_by_ws || order_ext.removed_by_rest;

            order_ext.removed_by_rest = true;
            order_ext.order.status = Status::Canceled;
            order_ext.order.exch_timestamp = Utc::now().timestamp_nanos_opt().unwrap();
            if !already_removed {
                self.order_id_map
                    .remove(&RefSymbolOrderId::new(symbol, order_ext.order.order_id));
                removed_orders.push(order_ext.order.clone());
            }

            if order_ext.removed_by_ws && order_ext.removed_by_rest {
                removed_order_ids.push(client_order_id.clone());
            }
        }

        for order_id in removed_order_ids {
            self.orders.remove(&order_id).unwrap();
        }
        removed_orders
    }
}

impl GetOrders for OrderManager {
    fn orders(&self, symbol: Option<String>) -> Vec<Order> {
        self.orders
            .iter()
            .filter(|(_, order)| {
                symbol.as_ref().map(|s| order.symbol == *s).unwrap_or(true) && order.order.active()
            })
            .map(|(_, order)| &order.order)
            .cloned()
            .collect()
    }
}
