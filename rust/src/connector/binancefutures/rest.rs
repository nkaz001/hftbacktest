use std::{
    collections::HashMap,
    fmt::{Debug, Write},
};

use chrono::Utc;
use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use sha2::Sha256;
use thiserror::Error;

/// https://binance-docs.github.io/apidocs/futures/en/
use super::{msg::PositionInformationV2, parse_client_order_id};
use crate::{
    connector::binancefutures::msg::{ListenKey, OrderResponse, OrderResponseResult},
    live::AssetInfo,
    ty::{Order, Status},
};

#[derive(Error, Debug)]
pub enum RequestError<T> {
    InvalidRequest(T),
    ReqError(reqwest::Error, T),
    OrderError(Order<()>, i64, String),
}

pub type OrderRequestError = RequestError<Order<()>>;

/// tick_size should not be a computed value.
fn get_precision(tick_size: f32) -> usize {
    let s = tick_size.to_string();
    let mut prec = 0;
    for (i, c) in s.chars().enumerate() {
        if c == '.' {
            prec = s.len() - i - 1;
            break;
        }
    }
    prec
}

#[derive(Clone)]
pub struct BinanceFuturesClient {
    client: reqwest::Client,
    url: String,
    prefix: String,
    api_key: String,
    secret: String,
}

impl BinanceFuturesClient {
    pub fn new(url: &str, prefix: &str, api_key: &str, secret: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            url: url.to_string(),
            prefix: prefix.to_string(),
            api_key: api_key.to_string(),
            secret: secret.to_string(),
        }
    }

    fn sign(secret: &str, s: &str) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(s.as_bytes());
        let hash = mac.finalize().into_bytes();
        let mut tmp = String::with_capacity(hash.len() * 2);
        for c in hash {
            write!(&mut tmp, "{:02x}", c).unwrap();
        }
        tmp
    }

    async fn get<T: for<'a> Deserialize<'a>>(
        &self,
        path: &str,
        mut query: String,
        api_key: &str,
        secret: &str,
    ) -> Result<T, reqwest::Error> {
        let time = Utc::now().timestamp_millis() - 1000;
        if !query.is_empty() {
            query.push_str("&");
        }
        query.push_str("recvWindow=5000&timestamp=");
        query.push_str(&time.to_string());
        let signature = Self::sign(secret, &query);
        let resp = self
            .client
            .get(&format!(
                "{}{}?{}&signature={}",
                self.url, path, query, signature
            ))
            .header("Accept", "application/json")
            .header("X-MBX-APIKEY", api_key)
            .send()
            .await?
            .json()
            .await?;
        Ok(resp)
    }

    async fn put<T: for<'a> Deserialize<'a>>(
        &self,
        path: &str,
        body: String,
        api_key: &str,
        secret: &str,
    ) -> Result<T, reqwest::Error> {
        let time = Utc::now().timestamp_millis() - 1000;
        let sign_body = format!("recvWindow=5000&timestamp={}{}", time, body);
        let signature = Self::sign(secret, &sign_body);
        let resp = self
            .client
            .put(&format!(
                "{}{}?recvWindow=5000&timestamp={}&signature={}",
                self.url, path, time, signature
            ))
            .header("Accept", "application/json")
            .header("X-MBX-APIKEY", api_key)
            .body(body)
            .send()
            .await?
            .json()
            .await?;
        Ok(resp)
    }

    async fn post<T: for<'a> Deserialize<'a>>(
        &self,
        path: &str,
        body: String,
        api_key: &str,
        secret: &str,
    ) -> Result<T, reqwest::Error> {
        let time = Utc::now().timestamp_millis() - 1000;
        let sign_body = format!("recvWindow=5000&timestamp={}{}", time, body);
        let signature = Self::sign(secret, &sign_body);
        let resp = self
            .client
            .post(&format!(
                "{}{}?recvWindow=5000&timestamp={}&signature={}",
                self.url, path, time, signature
            ))
            .header("Accept", "application/json")
            .header("X-MBX-APIKEY", api_key)
            .body(body)
            .send()
            .await?
            .json()
            .await?;
        Ok(resp)
    }

    async fn delete<T: for<'a> Deserialize<'a>>(
        &self,
        path: &str,
        body: String,
        api_key: &str,
        secret: &str,
    ) -> Result<T, reqwest::Error> {
        let time = Utc::now().timestamp_millis() - 1000;
        let sign_body = format!("recvWindow=5000&timestamp={}{}", time, body);
        let signature = Self::sign(secret, &sign_body);
        let resp = self
            .client
            .delete(&format!(
                "{}{}?recvWindow=5000&timestamp={}&signature={}",
                self.url, path, time, signature
            ))
            .header("Accept", "application/json")
            .header("X-MBX-APIKEY", api_key)
            .body(body)
            .send()
            .await?
            .json()
            .await?;
        Ok(resp)
    }

    pub async fn start_user_data_stream(&self) -> Result<String, reqwest::Error> {
        let resp: Result<ListenKey, _> = self
            .post(
                "/fapi/v1/listenKey",
                String::new(),
                &self.api_key,
                &self.secret,
            )
            .await;
        resp.map(|v| v.listen_key)
    }

    pub async fn keepalive_user_data_stream(&self) -> Result<(), reqwest::Error> {
        let _: serde_json::Value = self
            .put(
                "/fapi/v1/listenKey",
                String::new(),
                &self.api_key,
                &self.secret,
            )
            .await?;
        Ok(())
    }

    pub async fn submit_order(
        &self,
        symbol: &str,
        order: Order<()>,
    ) -> Result<Order<()>, OrderRequestError> {
        let prec = get_precision(order.tick_size);
        let mut body = String::with_capacity(200);
        body.push_str("newClientOrderId=");
        body.push_str(&self.prefix);
        body.push_str(&order.order_id.to_string());
        body.push_str("&symbol=");
        body.push_str(&symbol);
        body.push_str("&side=");
        body.push_str(&order.side.to_string());
        body.push_str("&price=");
        body.push_str(&format!(
            "{:.prec$}",
            order.price_tick as f32 * order.tick_size,
            prec = prec
        ));
        body.push_str("&quantity=");
        body.push_str(&format!("{:.5}", order.qty));
        body.push_str("&type=");
        body.push_str(&order.order_type.to_string());
        body.push_str("&timeInForce=");
        body.push_str(&order.time_in_force.to_string());

        let resp: OrderResponseResult = self
            .post("/fapi/v1/order", body, &self.api_key, &self.secret)
            .await
            .map_err(|e| RequestError::ReqError(e, order.clone()))?;
        match resp {
            OrderResponseResult::Ok(resp) => {
                Ok(Order {
                    qty: resp.orig_qty,
                    leaves_qty: resp.orig_qty - resp.cum_qty,
                    price_tick: (resp.price / order.tick_size).round() as i32,
                    tick_size: order.tick_size,
                    side: order.side,
                    time_in_force: resp.time_in_force,
                    exch_timestamp: resp.update_time * 1000,
                    status: Status::New,
                    local_timestamp: 0,
                    req: Status::None,
                    exec_price_tick: 0,
                    exec_qty: resp.executed_qty,
                    order_id: order.order_id,
                    order_type: resp.type_,
                    // Invalid information
                    q: (),
                    // Invalid information
                    maker: false,
                })
            }
            OrderResponseResult::Err(resp) => {
                Err(RequestError::OrderError(order, resp.code, resp.msg))
            }
        }
    }

    pub async fn submit_orders(
        &self,
        symbol: &str,
        orders: Vec<Order<()>>,
    ) -> Result<Vec<Result<Order<()>, OrderRequestError>>, RequestError<Vec<Order<()>>>> {
        if orders.len() > 5 {
            return Err(RequestError::InvalidRequest(orders));
        }
        let mut body = String::with_capacity(2000 * orders.len());
        body.push_str("{\"batchOrders\":[");
        for (i, order) in orders.iter().enumerate() {
            if i > 0 {
                body.push_str(",");
            }
            body.push_str("{\"newClientOrderId\":\"");
            body.push_str(&self.prefix);
            body.push_str(&order.order_id.to_string());
            body.push_str("\",\"symbol\":\"");
            body.push_str(&symbol);
            body.push_str("\",\"side\":\"");
            body.push_str(&order.side.to_string());
            body.push_str("\",\"price\":\"");
            body.push_str(&format!("{:.5}", order.price_tick as f32 * order.tick_size));
            body.push_str("\",\"quantity\":\"");
            body.push_str(&format!("{:.5}", order.qty));
            body.push_str("\",\"type\":\"");
            body.push_str(&order.order_type.to_string());
            body.push_str("\",\"timeInForce\":\"");
            body.push_str(&order.time_in_force.to_string());
            body.push_str("\"}");
        }
        body.push_str("]}");

        let resp_: Vec<OrderResponseResult> = self
            .post("/fapi/v1/batchOrders", body, &self.api_key, &self.secret)
            .await
            .map_err(|e| RequestError::ReqError(e, orders.clone()))?;
        Ok(resp_
            .into_iter()
            .zip(orders.into_iter())
            .map(|(resp, order)| {
                match resp {
                    OrderResponseResult::Ok(resp) => {
                        // todo: check if the order id is matched.
                        Ok(Order {
                            qty: resp.orig_qty,
                            leaves_qty: resp.orig_qty - resp.cum_qty,
                            price_tick: (resp.price / order.tick_size).round() as i32,
                            tick_size: order.tick_size,
                            side: order.side,
                            time_in_force: resp.time_in_force,
                            exch_timestamp: resp.update_time * 1000,
                            status: Status::New,
                            local_timestamp: 0,
                            req: Status::None,
                            exec_price_tick: 0,
                            exec_qty: resp.executed_qty,
                            order_id: order.order_id,
                            order_type: resp.type_,
                            // Invalid information
                            q: (),
                            // Invalid information
                            maker: false,
                        })
                    }
                    OrderResponseResult::Err(resp) => {
                        Err(RequestError::OrderError(order, resp.code, resp.msg))
                    }
                }
            })
            .collect())
    }

    pub async fn modify_order(
        &self,
        symbol: &str,
        order: Order<()>,
    ) -> Result<Order<()>, OrderRequestError> {
        let mut body = String::with_capacity(100);
        body.push_str("symbol=");
        body.push_str(&symbol);
        body.push_str("&origClientOrderId=");
        body.push_str(&self.prefix);
        body.push_str(&order.order_id.to_string());
        body.push_str("&side=");
        body.push_str(&order.side.to_string());
        body.push_str("&price=");
        body.push_str(&format!("{:.5}", order.price_tick as f32 * order.tick_size));
        body.push_str("&quantity=");
        body.push_str(&format!("{:.5}", order.qty));

        let resp: OrderResponseResult = self
            .put("/fapi/v1/order", body, &self.api_key, &self.secret)
            .await
            .map_err(|e| RequestError::ReqError(e, order.clone()))?;
        match resp {
            OrderResponseResult::Ok(resp) => {
                Ok(Order {
                    qty: resp.orig_qty,
                    leaves_qty: resp.orig_qty - resp.cum_qty,
                    price_tick: (resp.price / order.tick_size).round() as i32,
                    tick_size: order.tick_size,
                    side: resp.side,
                    time_in_force: resp.time_in_force,
                    exch_timestamp: resp.update_time * 1000,
                    status: resp.status,
                    local_timestamp: 0,
                    req: Status::None,
                    exec_price_tick: 0,
                    exec_qty: resp.executed_qty,
                    order_id: order.order_id,
                    order_type: resp.type_,
                    // Invalid information
                    q: (),
                    // Invalid information
                    maker: false,
                })
            }
            OrderResponseResult::Err(resp) => {
                Err(RequestError::OrderError(order, resp.code, resp.msg))
            }
        }
    }

    pub async fn cancel_order(
        &self,
        symbol: &str,
        order: Order<()>,
    ) -> Result<Order<()>, OrderRequestError> {
        let mut body = String::with_capacity(100);
        body.push_str("symbol=");
        body.push_str(&symbol);
        body.push_str("&origClientOrderId=");
        body.push_str(&self.prefix);
        body.push_str(&order.order_id.to_string());

        let resp: OrderResponseResult = self
            .delete("/fapi/v1/order", body, &self.api_key, &self.secret)
            .await
            .map_err(|e| RequestError::ReqError(e, order.clone()))?;
        match resp {
            OrderResponseResult::Ok(resp) => {
                Ok(Order {
                    qty: resp.orig_qty,
                    leaves_qty: resp.orig_qty - resp.cum_qty,
                    price_tick: (resp.price / order.tick_size).round() as i32,
                    tick_size: order.tick_size,
                    side: resp.side,
                    time_in_force: resp.time_in_force,
                    exch_timestamp: resp.update_time * 1000,
                    status: Status::Canceled,
                    local_timestamp: 0,
                    req: Status::None,
                    exec_price_tick: 0,
                    exec_qty: resp.executed_qty,
                    order_id: order.order_id,
                    order_type: resp.type_,
                    // Invalid information
                    q: (),
                    // Invalid information
                    maker: false,
                })
            }
            OrderResponseResult::Err(resp) => {
                Err(RequestError::OrderError(order, resp.code, resp.msg))
            }
        }
    }

    pub async fn cancel_orders(
        &self,
        symbol: &str,
        orders: Vec<Order<()>>,
    ) -> Result<Vec<Result<Order<()>, OrderRequestError>>, RequestError<Vec<Order<()>>>> {
        if orders.len() > 10 {
            return Err(RequestError::InvalidRequest(orders));
        }
        let mut body = String::with_capacity(100);
        body.push_str("{\"symbol\":\"");
        body.push_str(&symbol);
        body.push_str("\",\"origClientOrderIdList\":[");
        for (i, order) in orders.iter().enumerate() {
            if i > 0 {
                body.push_str(",");
            }
            body.push_str("\"");
            body.push_str(&self.prefix);
            body.push_str(&order.order_id.to_string());
            body.push_str("\"");
        }
        body.push_str("]}");
        let resp_: Vec<OrderResponseResult> = self
            .post("/fapi/v1/batchOrders", body, &self.api_key, &self.secret)
            .await
            .map_err(|e| RequestError::ReqError(e, orders.clone()))?;
        Ok(resp_
            .into_iter()
            .zip(orders.into_iter())
            .map(|(resp, order)| {
                match resp {
                    OrderResponseResult::Ok(resp) => {
                        // todo: check if the order id is matched.
                        Ok(Order {
                            qty: resp.orig_qty,
                            leaves_qty: resp.orig_qty - resp.cum_qty,
                            price_tick: (resp.price / order.tick_size).round() as i32,
                            tick_size: order.tick_size,
                            side: resp.side,
                            time_in_force: resp.time_in_force,
                            exch_timestamp: resp.update_time * 1000,
                            status: Status::Canceled,
                            local_timestamp: 0,
                            req: Status::None,
                            exec_price_tick: 0,
                            exec_qty: resp.executed_qty,
                            order_id: order.order_id,
                            order_type: resp.type_,
                            // Invalid information
                            q: (),
                            // Invalid information
                            maker: false,
                        })
                    }
                    OrderResponseResult::Err(resp) => {
                        Err(RequestError::OrderError(order, resp.code, resp.msg))
                    }
                }
            })
            .collect())
    }

    pub async fn cancel_all_orders(&self, symbol: &str) -> Result<(), reqwest::Error> {
        let _: serde_json::Value = self
            .delete(
                "/fapi/v1/allOpenOrders",
                format!("symbol={}", symbol),
                &self.api_key,
                &self.secret,
            )
            .await?;
        Ok(())
    }

    pub async fn get_position_information(
        &self,
    ) -> Result<Vec<PositionInformationV2>, reqwest::Error> {
        let resp: Vec<PositionInformationV2> = self
            .get(
                "/fapi/v2/positionRisk",
                String::new(),
                &self.api_key,
                &self.secret,
            )
            .await?;
        Ok(resp)
    }

    pub async fn get_current_all_open_orders(
        &self,
        assets: &HashMap<String, AssetInfo>,
    ) -> Result<Vec<Order<()>>, reqwest::Error> {
        let resp: Vec<OrderResponse> = self
            .get(
                "/fapi/v1/openOrders",
                String::new(),
                &self.api_key,
                &self.secret,
            )
            .await?;
        Ok(resp
            .iter()
            .map(|data| {
                assets.get(&data.symbol).and_then(|asset_info| {
                    parse_client_order_id(&data.client_order_id, &self.prefix).map(|order_id| {
                        Order {
                            qty: data.orig_qty,
                            leaves_qty: data.orig_qty - data.cum_qty,
                            price_tick: (data.price / asset_info.tick_size).round() as i32,
                            tick_size: asset_info.tick_size,
                            side: data.side,
                            time_in_force: data.time_in_force,
                            exch_timestamp: data.update_time * 1000,
                            status: data.status,
                            local_timestamp: 0,
                            req: Status::None,
                            exec_price_tick: 0,
                            exec_qty: data.executed_qty,
                            order_id,
                            order_type: data.type_,
                            // Invalid information
                            q: (),
                            // Invalid information
                            maker: false,
                        }
                    })
                })
            })
            .filter(|order| order.is_some())
            .map(|order| order.unwrap())
            .collect())
    }
}
