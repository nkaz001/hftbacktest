use std::{
    collections::{hash_map::Entry, HashMap},
    num::{ParseFloatError, ParseIntError},
    sync::{Arc, Mutex},
};

use thiserror::Error;

use crate::{
    connector::{
        bybit::msg::{Execution, FastExecution, Order as BybitOrder, PrivateOrder},
        util::gen_random_string,
    },
    prelude::{get_precision, OrdType, OrderId, Side, TimeInForce},
    types::{Order, Status, Value},
};

pub type OrderManagerWrapper = Arc<Mutex<OrderManager>>;

#[derive(Error, Debug)]
pub(super) enum HandleError {
    #[error("px qty parse error: {0}")]
    InvalidPxQty(#[from] ParseFloatError),
    #[error("order id parse error: {0}")]
    InvalidOrderId(ParseIntError),
    #[error("prefix unmatched")]
    PrefixUnmatched,
    #[error("order not found")]
    OrderNotFound,
    #[error("req id is invalid")]
    InvalidReqId,
    #[error("asset not found")]
    AssetNotFound,
    #[error("invalid argument")]
    InvalidArg(&'static str),
    #[error("order already exist")]
    OrderAlreadyExist,
    #[error("serde: {0}")]
    SerdeError(#[from] serde_json::Error),
    #[error("tokio: {0}")]
    TokioError(#[from] tokio_tungstenite::tungstenite::Error),
}

impl Into<Value> for HandleError {
    fn into(self) -> Value {
        // todo!: improve this to deliver detailed error information.
        Value::String(self.to_string())
    }
}

pub struct OrderManager {
    prefix: String,
    orders: HashMap<OrderId, (usize, String, Order)>,
}

impl OrderManager {
    pub fn new(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
            orders: Default::default(),
        }
    }

    fn parse_order_id(&self, order_link_id: &str) -> Result<OrderId, HandleError> {
        if !order_link_id.starts_with(&self.prefix) {
            return Err(HandleError::PrefixUnmatched);
        }
        order_link_id[(self.prefix.len() + 8)..]
            .parse()
            .map_err(|e| HandleError::InvalidOrderId(e))
    }

    pub fn update_order(&mut self, data: &PrivateOrder) -> Result<(usize, Order), HandleError> {
        let order_id = self.parse_order_id(&data.order_link_id)?;
        let (asset_no, _order_link_id, order) = self
            .orders
            .get_mut(&order_id)
            .ok_or(HandleError::OrderNotFound)?;
        order.req = Status::None;
        order.status = data.order_status;
        order.exch_timestamp = data.updated_time * 1_000_000;
        let is_active = order.active();
        if !is_active {
            let (asset_no, _order_link_id, order) = self.orders.remove(&order_id).unwrap();
            Ok((asset_no, order))
        } else {
            Ok((*asset_no, order.clone()))
        }
    }

    pub fn update_execution(&mut self, data: &Execution) -> Result<(usize, Order), HandleError> {
        let order_id = self.parse_order_id(&data.order_link_id)?;
        let (asset_no, _order_link_id, order) = self
            .orders
            .get_mut(&order_id)
            .ok_or(HandleError::OrderNotFound)?;
        order.exec_price_tick = (data.exec_price / order.price_tick as f64).round() as i64;
        order.exec_qty = data.exec_qty;
        order.exch_timestamp = data.exec_time * 1_000_000;
        Ok((*asset_no, order.clone()))
    }

    pub fn update_fast_execution(
        &mut self,
        data: &FastExecution,
    ) -> Result<(usize, Order), HandleError> {
        // fixme: there is no valid order_link_id.
        let order_id = self.parse_order_id(&data.order_link_id)?;
        let (asset_no, _order_link_id, order) = self
            .orders
            .get_mut(&order_id)
            .ok_or(HandleError::OrderNotFound)?;
        order.exec_price_tick = (data.exec_price / order.price_tick as f64).round() as i64;
        order.exec_qty = data.exec_qty;
        order.exch_timestamp = data.exec_time * 1_000_000;
        Ok((*asset_no, order.clone()))
    }

    pub fn new_order(
        &mut self,
        symbol: &str,
        category: &str,
        asset_no: usize,
        order: Order,
    ) -> Result<BybitOrder, HandleError> {
        let price_prec = get_precision(order.tick_size);
        let rand_id = gen_random_string(8);
        let bybit_order = BybitOrder {
            symbol: symbol.to_string(),
            side: Some({
                match order.side {
                    Side::Buy => "Buy".to_string(),
                    Side::Sell => "Sell".to_string(),
                    Side::None | Side::Unsupported => return Err(HandleError::InvalidArg("side")),
                }
            }),
            order_type: Some({
                match order.order_type {
                    OrdType::Limit => "Limit".to_string(),
                    OrdType::Market => "Market".to_string(),
                    OrdType::Unsupported => return Err(HandleError::InvalidArg("order_type")),
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
                        return Err(HandleError::InvalidArg("time_in_force"));
                    }
                }
            }),
            order_link_id: format!("{}{}{}", self.prefix, rand_id, order.order_id),
        };
        match self.orders.entry(order.order_id) {
            Entry::Occupied(_) => {
                return Err(HandleError::OrderAlreadyExist);
            }
            Entry::Vacant(entry) => {
                entry.insert((asset_no, bybit_order.order_link_id.clone(), order));
            }
        }
        Ok(bybit_order)
    }

    pub fn cancel_order(
        &mut self,
        symbol: &str,
        category: &str,
        order_id: OrderId,
    ) -> Result<BybitOrder, HandleError> {
        let (_, order_link_id, order) = self
            .orders
            .get(&order_id)
            .ok_or(HandleError::OrderNotFound)?;
        let bybit_order = BybitOrder {
            symbol: symbol.to_string(),
            side: None,
            order_type: None,
            qty: None,
            price: None,
            category: category.to_string(),
            time_in_force: None,
            order_link_id: order_link_id.clone(),
        };
        Ok(bybit_order)
    }

    pub fn update_submit_fail(
        &mut self,
        order_link_id: &str,
    ) -> Result<(usize, Order), HandleError> {
        let order_id = self.parse_order_id(order_link_id)?;
        let (asset_no, _order_link_id, mut order) = self
            .orders
            .remove(&order_id)
            .ok_or(HandleError::OrderNotFound)?;
        order.req = Status::None;
        order.status = Status::Expired;
        Ok((asset_no, order))
    }

    pub fn update_cancel_fail(
        &mut self,
        order_link_id: &str,
    ) -> Result<(usize, Order), HandleError> {
        let order_id = self.parse_order_id(order_link_id)?;
        let (asset_no, _order_link_id, mut order) = self
            .orders
            .get_mut(&order_id)
            .cloned()
            .ok_or(HandleError::OrderNotFound)?;
        order.req = Status::None;
        Ok((asset_no, order))
    }

    pub fn clear_orders(&mut self) -> Vec<(usize, Order)> {
        let mut values: Vec<(usize, Order)> = Vec::new();
        values.extend(self.orders.drain().map(|(_, (asset_no, _, mut order))| {
            order.status = Status::Canceled;
            (asset_no, order)
        }));
        values
    }
}
