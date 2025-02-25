use std::sync::{Arc, Mutex};

use hashbrown::HashMap;
use hftbacktest::{
    prelude::get_precision,
    types::{OrdType, Order, OrderId, Side, Status, TimeInForce},
};

use crate::{
    bybit::{
        BybitError,
        msg::{Execution, FastExecution, Order as BybitOrder, PrivateOrder},
    },
    connector::GetOrders,
    utils::{RefSymbolOrderId, SymbolOrderId, generate_rand_string},
};

pub type SharedOrderManager = Arc<Mutex<OrderManager>>;

pub type OrderLinkId = String;

#[derive(Clone)]
pub struct OrderExt {
    pub symbol: String,
    pub order: Order,
}

pub struct OrderManager {
    prefix: String,
    orders: HashMap<OrderLinkId, OrderExt>,
    order_id_map: HashMap<SymbolOrderId, OrderLinkId>,
}

impl OrderManager {
    pub fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
            orders: Default::default(),
            order_id_map: Default::default(),
        }
    }

    pub fn update_order(&mut self, data: &PrivateOrder) -> Result<OrderExt, BybitError> {
        if !data.order_link_id.starts_with(&self.prefix) {
            return Err(BybitError::PrefixUnmatched);
        }
        let order = self
            .orders
            .get_mut(&data.order_link_id)
            .ok_or(BybitError::OrderNotFound)?;
        order.order.req = Status::None;
        order.order.status = data.order_status;
        order.order.exch_timestamp = data.updated_time * 1_000_000;
        let is_active = order.order.active();
        if !is_active {
            self.order_id_map
                .remove(&RefSymbolOrderId::new(&order.symbol, order.order.order_id));
            Ok(self.orders.remove(&data.order_link_id).unwrap())
        } else {
            Ok(order.clone())
        }
    }

    pub fn update_execution(&mut self, data: &Execution) -> Result<OrderExt, BybitError> {
        if !data.order_link_id.starts_with(&self.prefix) {
            return Err(BybitError::PrefixUnmatched);
        }
        let order_info = self
            .orders
            .get_mut(&data.order_link_id)
            .ok_or(BybitError::OrderNotFound)?;
        order_info.order.exec_price_tick =
            (data.exec_price / order_info.order.price_tick as f64).round() as i64;
        order_info.order.exec_qty = data.exec_qty;
        order_info.order.exch_timestamp = data.exec_time * 1_000_000;
        Ok(order_info.clone())
    }

    pub fn update_fast_execution(&mut self, data: &FastExecution) -> Result<OrderExt, BybitError> {
        // fixme: there is no valid order_link_id.
        if !data.order_link_id.starts_with(&self.prefix) {
            return Err(BybitError::PrefixUnmatched);
        }
        let order_info = self
            .orders
            .get_mut(&data.order_link_id)
            .ok_or(BybitError::OrderNotFound)?;
        order_info.order.exec_price_tick =
            (data.exec_price / order_info.order.price_tick as f64).round() as i64;
        order_info.order.exec_qty = data.exec_qty;
        order_info.order.exch_timestamp = data.exec_time * 1_000_000;
        Ok(order_info.clone())
    }

    pub fn new_order(
        &mut self,
        symbol: &str,
        category: &str,
        order: Order,
    ) -> Result<BybitOrder, BybitError> {
        let price_prec = get_precision(order.tick_size);
        let order_link_id = format!("{}{}", self.prefix, generate_rand_string(16));
        let bybit_order = BybitOrder {
            symbol: symbol.to_string(),
            side: Some({
                match order.side {
                    Side::Buy => "Buy".to_string(),
                    Side::Sell => "Sell".to_string(),
                    Side::None | Side::Unsupported => return Err(BybitError::InvalidArg("side")),
                }
            }),
            order_type: Some({
                match order.order_type {
                    OrdType::Limit => "Limit".to_string(),
                    OrdType::Market => "Market".to_string(),
                    OrdType::Unsupported => return Err(BybitError::InvalidArg("order_type")),
                }
            }),
            qty: Some(format!("{:.5}", order.qty)),
            price: Some(format!(
                "{:.prec$}",
                order.price_tick as f64 * order.tick_size,
                prec = price_prec
            )),
            category: category.to_string(),
            time_in_force: Some({
                match order.time_in_force {
                    TimeInForce::GTC => "GTC".to_string(),
                    TimeInForce::GTX => "PostOnly".to_string(),
                    TimeInForce::FOK => "FOK".to_string(),
                    TimeInForce::IOC => "IOC".to_string(),
                    TimeInForce::Unsupported => {
                        return Err(BybitError::InvalidArg("time_in_force"));
                    }
                }
            }),
            order_link_id: order_link_id.clone(),
        };

        let symbol_order_id = SymbolOrderId::new(symbol.to_string(), order.order_id);
        if self.order_id_map.contains_key(&symbol_order_id) {
            return Err(BybitError::OrderAlreadyExist);
        }

        if self.orders.contains_key(&order_link_id) {
            return Err(BybitError::OrderAlreadyExist);
        }

        self.order_id_map
            .insert(symbol_order_id, order_link_id.clone());
        self.orders.insert(
            order_link_id,
            OrderExt {
                symbol: symbol.to_string(),
                order,
            },
        );
        Ok(bybit_order)
    }

    pub fn cancel_order(
        &mut self,
        symbol: &str,
        category: &str,
        order_id: OrderId,
    ) -> Result<BybitOrder, BybitError> {
        let order_link_id = self
            .order_id_map
            .get(&RefSymbolOrderId::new(symbol, order_id))
            .ok_or(BybitError::OrderNotFound)?;
        let order = BybitOrder {
            symbol: symbol.to_string(),
            side: None,
            order_type: None,
            qty: None,
            price: None,
            category: category.to_string(),
            time_in_force: None,
            order_link_id: order_link_id.clone(),
        };
        Ok(order)
    }

    pub fn update_submit_fail(&mut self, order_link_id: &str) -> Result<OrderExt, BybitError> {
        let mut order = self
            .orders
            .remove(order_link_id)
            .ok_or(BybitError::OrderNotFound)?;
        order.order.req = Status::None;
        order.order.status = Status::Expired;
        self.order_id_map
            .remove(&RefSymbolOrderId::new(&order.symbol, order.order.order_id));
        Ok(order)
    }

    pub fn update_cancel_fail(&mut self, order_link_id: &str) -> Result<OrderExt, BybitError> {
        let mut order_info = self
            .orders
            .get_mut(order_link_id)
            .cloned()
            .ok_or(BybitError::OrderNotFound)?;
        order_info.order.req = Status::None;
        Ok(order_info)
    }

    pub fn cancel_all(&mut self, symbol: &str) -> Vec<Order> {
        let mut removed_order_ids = Vec::new();
        for (order_link_id, order_ext) in &mut self.orders {
            if order_ext.symbol != symbol {
                continue;
            }

            order_ext.order.status = Status::Canceled;

            self.order_id_map
                .remove(&RefSymbolOrderId::new(symbol, order_ext.order.order_id));
            removed_order_ids.push(order_link_id.clone());
        }

        removed_order_ids
            .iter()
            .map(|id| self.orders.remove(id).unwrap().order)
            .collect()
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
