use std::sync::{Arc, Mutex};

use hashbrown::{hash_map::Entry, HashMap};
use hftbacktest::{
    prelude::get_precision,
    types::{OrdType, Order, OrderId, Side, Status, TimeInForce},
};

use crate::{
    bybit::{
        msg::{Execution, FastExecution, Order as BybitOrder, PrivateOrder},
        BybitError,
    },
    connector::GetOrders,
    utils::{generate_rand_string, RefSymbolOrderId, SymbolOrderId},
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
        match self.orders.entry(order_link_id) {
            Entry::Occupied(_) => {
                return Err(BybitError::OrderAlreadyExist);
            }
            Entry::Vacant(entry) => {
                entry.insert(OrderExt {
                    symbol: symbol.to_string(),
                    order,
                });
            }
        }
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

    pub fn clear_orders(&mut self, symbol: &str) -> Vec<Order> {
        let removed_order_ids: Vec<_> = self
            .orders
            .iter()
            .filter(|(_, order)| order.symbol == symbol)
            .map(|(id, _)| id)
            .cloned()
            .collect();

        let mut removed_orders = Vec::new();
        for order_id in removed_order_ids {
            let removed_order = self.orders.remove(&order_id).unwrap();
            self.order_id_map.remove(&RefSymbolOrderId::new(
                &removed_order.symbol,
                removed_order.order.order_id,
            ));
            removed_orders.push(removed_order.order);
        }
        removed_orders
    }
}

impl GetOrders for OrderManager {
    fn get_orders(&self, symbol: Option<String>) -> Vec<Order> {
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
