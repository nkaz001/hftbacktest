use std::{
    collections::{hash_map::Entry, HashMap},
    sync::{Arc, Mutex},
};

use hftbacktest::{
    prelude::get_precision,
    types::{OrdType, Order, OrderId, Side, Status, TimeInForce},
};

use crate::{
    bybit::{
        msg::{Execution, FastExecution, Order as BybitOrder, PrivateOrder},
        BybitError,
    },
    utils::gen_random_string,
};

pub type SharedOrderManager = Arc<Mutex<OrderManager>>;

#[derive(Clone)]
pub struct OrderInfo {
    pub symbol: String,
    pub order_link_id: String,
    pub order: Order,
}

pub struct OrderManager {
    prefix: String,
    orders: HashMap<OrderId, OrderInfo>,
}

impl OrderManager {
    pub fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
            orders: Default::default(),
        }
    }

    fn parse_order_id(&self, order_link_id: &str) -> Result<OrderId, BybitError> {
        if !order_link_id.starts_with(&self.prefix) {
            return Err(BybitError::PrefixUnmatched);
        }
        order_link_id[(self.prefix.len() + 8)..]
            .parse()
            .map_err(BybitError::InvalidOrderId)
    }

    pub fn update_order(&mut self, data: &PrivateOrder) -> Result<OrderInfo, BybitError> {
        let order_id = self.parse_order_id(&data.order_link_id)?;
        let order_info = self
            .orders
            .get_mut(&order_id)
            .ok_or(BybitError::OrderNotFound)?;
        order_info.order.req = Status::None;
        order_info.order.status = data.order_status;
        order_info.order.exch_timestamp = data.updated_time * 1_000_000;
        let is_active = order_info.order.active();
        if !is_active {
            Ok(self.orders.remove(&order_id).unwrap())
        } else {
            Ok(order_info.clone())
        }
    }

    pub fn update_execution(&mut self, data: &Execution) -> Result<OrderInfo, BybitError> {
        let order_id = self.parse_order_id(&data.order_link_id)?;
        let order_info = self
            .orders
            .get_mut(&order_id)
            .ok_or(BybitError::OrderNotFound)?;
        order_info.order.exec_price_tick =
            (data.exec_price / order_info.order.price_tick as f64).round() as i64;
        order_info.order.exec_qty = data.exec_qty;
        order_info.order.exch_timestamp = data.exec_time * 1_000_000;
        Ok(order_info.clone())
    }

    pub fn update_fast_execution(&mut self, data: &FastExecution) -> Result<OrderInfo, BybitError> {
        // fixme: there is no valid order_link_id.
        let order_id = self.parse_order_id(&data.order_link_id)?;
        let order_info = self
            .orders
            .get_mut(&order_id)
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
        let rand_id = gen_random_string(8);
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
            order_link_id: format!("{}{}{}", self.prefix, rand_id, order.order_id),
        };
        match self.orders.entry(order.order_id) {
            Entry::Occupied(_) => {
                return Err(BybitError::OrderAlreadyExist);
            }
            Entry::Vacant(entry) => {
                entry.insert(OrderInfo {
                    symbol: symbol.to_string(),
                    order_link_id: bybit_order.order_link_id.clone(),
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
        let order_info = self
            .orders
            .get(&order_id)
            .ok_or(BybitError::OrderNotFound)?;
        let bybit_order = BybitOrder {
            symbol: symbol.to_string(),
            side: None,
            order_type: None,
            qty: None,
            price: None,
            category: category.to_string(),
            time_in_force: None,
            order_link_id: order_info.order_link_id.clone(),
        };
        Ok(bybit_order)
    }

    pub fn update_submit_fail(&mut self, order_link_id: &str) -> Result<OrderInfo, BybitError> {
        let order_id = self.parse_order_id(order_link_id)?;
        let mut order_info = self
            .orders
            .remove(&order_id)
            .ok_or(BybitError::OrderNotFound)?;
        order_info.order.req = Status::None;
        order_info.order.status = Status::Expired;
        Ok(order_info)
    }

    pub fn update_cancel_fail(&mut self, order_link_id: &str) -> Result<OrderInfo, BybitError> {
        let order_id = self.parse_order_id(order_link_id)?;
        let mut order_info = self
            .orders
            .get_mut(&order_id)
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
            removed_orders.push(self.orders.remove(&order_id).unwrap().order);
        }
        removed_orders
    }
}
