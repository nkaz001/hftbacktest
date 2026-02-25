use chrono::Utc;
use hftbacktest::types::{OrdType, Side, TimeInForce};
use serde::Deserialize;

use super::{
    BinancePapiError,
    msg::rest::{
        AccountInfo, Balance, ListenKey, OrderResponse, OrderResponseResult, UmPositionRisk,
    },
};
use crate::binancefutures::msg::rest::ExchangeInfo;
use crate::utils::sign_hmac_sha256;

#[derive(Clone)]
pub struct BinancePapiClient {
    client: reqwest::Client,
    /// REST API base URL (https://papi.binance.com)
    api_url: String,
    api_key: String,
    secret: String,
}

impl BinancePapiClient {
    pub fn new(api_url: &str, api_key: &str, secret: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_url: api_url.to_string(),
            api_key: api_key.to_string(),
            secret: secret.to_string(),
        }
    }

    async fn get<T: for<'a> Deserialize<'a>>(
        &self,
        path: &str,
        mut query: String,
    ) -> Result<T, reqwest::Error> {
        let time = Utc::now().timestamp_millis() - 1000;
        if !query.is_empty() {
            query.push('&');
        }
        query.push_str("recvWindow=5000&timestamp=");
        query.push_str(&time.to_string());
        let signature = sign_hmac_sha256(&self.secret, &query);
        let resp = self
            .client
            .get(format!(
                "{}{}?{}&signature={}",
                self.api_url, path, query, signature
            ))
            .header("Accept", "application/json")
            .header("X-MBX-APIKEY", &self.api_key)
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
    ) -> Result<T, reqwest::Error> {
        let time = Utc::now().timestamp_millis() - 1000;
        let sign_body = format!("recvWindow=5000&timestamp={time}{body}");
        let signature = sign_hmac_sha256(&self.secret, &sign_body);
        let resp = self
            .client
            .put(format!(
                "{}{}?recvWindow=5000&timestamp={}&signature={}",
                self.api_url, path, time, signature
            ))
            .header("Accept", "application/json")
            .header("X-MBX-APIKEY", &self.api_key)
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
    ) -> Result<T, reqwest::Error> {
        let time = Utc::now().timestamp_millis() - 1000;
        let sign_body = format!("recvWindow=5000&timestamp={time}{body}");
        let signature = sign_hmac_sha256(&self.secret, &sign_body);
        let resp = self
            .client
            .post(format!(
                "{}{}?recvWindow=5000&timestamp={}&signature={}",
                self.api_url, path, time, signature
            ))
            .header("Accept", "application/json")
            .header("X-MBX-APIKEY", &self.api_key)
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
    ) -> Result<T, reqwest::Error> {
        let time = Utc::now().timestamp_millis() - 1000;
        let sign_body = format!("recvWindow=5000&timestamp={time}{body}");
        let signature = sign_hmac_sha256(&self.secret, &sign_body);
        let resp = self
            .client
            .delete(format!(
                "{}{}?recvWindow=5000&timestamp={}&signature={}",
                self.api_url, path, time, signature
            ))
            .header("Accept", "application/json")
            .header("X-MBX-APIKEY", &self.api_key)
            .body(body)
            .send()
            .await?
            .json()
            .await?;
        Ok(resp)
    }

    /// Get UM futures exchange info (public, no auth required).
    /// Uses fapi.binance.com since PAPI UM symbols use futures exchange info.
    /// GET https://fapi.binance.com/fapi/v1/exchangeInfo
    pub async fn get_exchange_info(&self) -> Result<ExchangeInfo, reqwest::Error> {
        let resp = self
            .client
            .get("https://fapi.binance.com/fapi/v1/exchangeInfo")
            .header("Accept", "application/json")
            .send()
            .await?
            .json()
            .await?;
        Ok(resp)
    }

    // ========== ListenKey Management ==========

    /// Start user data stream for Portfolio Margin
    /// POST /papi/v1/listenKey
    pub async fn start_user_data_stream(&self) -> Result<String, reqwest::Error> {
        let resp: ListenKey = self.post("/papi/v1/listenKey", String::new()).await?;
        Ok(resp.listen_key)
    }

    /// Keepalive user data stream
    /// PUT /papi/v1/listenKey
    pub async fn keepalive_user_data_stream(&self) -> Result<(), reqwest::Error> {
        let _: serde_json::Value = self.put("/papi/v1/listenKey", String::new()).await?;
        Ok(())
    }

    /// Close user data stream
    /// DELETE /papi/v1/listenKey
    pub async fn close_user_data_stream(&self) -> Result<(), reqwest::Error> {
        let _: serde_json::Value = self.delete("/papi/v1/listenKey", String::new()).await?;
        Ok(())
    }

    // ========== UM Futures Order Operations ==========

    /// Submit a new UM futures order
    /// POST /papi/v1/um/order
    #[allow(clippy::too_many_arguments)]
    pub async fn submit_um_order(
        &self,
        client_order_id: &str,
        symbol: &str,
        side: Side,
        price: f64,
        price_prec: usize,
        qty: f64,
        order_type: OrdType,
        time_in_force: TimeInForce,
        reduce_only: bool,
    ) -> Result<OrderResponse, BinancePapiError> {
        let mut body = String::with_capacity(200);
        body.push_str("newClientOrderId=");
        body.push_str(client_order_id);
        body.push_str("&symbol=");
        body.push_str(symbol);
        body.push_str("&side=");
        body.push_str(side.as_ref());
        body.push_str("&price=");
        body.push_str(&format!("{price:.price_prec$}"));
        body.push_str("&quantity=");
        body.push_str(&format!("{qty:.5}"));
        body.push_str("&type=");
        body.push_str(order_type.as_ref());
        body.push_str("&timeInForce=");
        body.push_str(time_in_force.as_ref());
        // Add reduceOnly for position-closing orders (bypasses min notional)
        if reduce_only {
            body.push_str("&reduceOnly=true");
        }

        let resp: OrderResponseResult = self.post("/papi/v1/um/order", body).await?;
        match resp {
            OrderResponseResult::Ok(resp) => Ok(resp),
            OrderResponseResult::Err(resp) => Err(BinancePapiError::OrderError {
                code: resp.code,
                msg: resp.msg,
            }),
        }
    }

    /// Modify an existing UM futures order
    /// PUT /papi/v1/um/order
    pub async fn modify_um_order(
        &self,
        client_order_id: &str,
        symbol: &str,
        side: Side,
        price: f64,
        price_prec: usize,
        qty: f64,
    ) -> Result<OrderResponse, BinancePapiError> {
        let mut body = String::with_capacity(100);
        body.push_str("symbol=");
        body.push_str(symbol);
        body.push_str("&origClientOrderId=");
        body.push_str(client_order_id);
        body.push_str("&side=");
        body.push_str(side.as_ref());
        body.push_str("&price=");
        body.push_str(&format!("{price:.price_prec$}"));
        body.push_str("&quantity=");
        body.push_str(&format!("{qty:.5}"));

        let resp: OrderResponseResult = self.put("/papi/v1/um/order", body).await?;
        match resp {
            OrderResponseResult::Ok(resp) => Ok(resp),
            OrderResponseResult::Err(resp) => Err(BinancePapiError::OrderError {
                code: resp.code,
                msg: resp.msg,
            }),
        }
    }

    /// Cancel a UM futures order
    /// DELETE /papi/v1/um/order
    pub async fn cancel_um_order(
        &self,
        client_order_id: &str,
        symbol: &str,
    ) -> Result<OrderResponse, BinancePapiError> {
        let mut body = String::with_capacity(100);
        body.push_str("symbol=");
        body.push_str(symbol);
        body.push_str("&origClientOrderId=");
        body.push_str(client_order_id);

        let resp: OrderResponseResult = self.delete("/papi/v1/um/order", body).await?;
        match resp {
            OrderResponseResult::Ok(resp) => Ok(resp),
            OrderResponseResult::Err(resp) => Err(BinancePapiError::OrderError {
                code: resp.code,
                msg: resp.msg,
            }),
        }
    }

    /// Cancel UM order by exchange order ID (fallback when client_order_id is not available)
    /// DELETE /papi/v1/um/order
    pub async fn cancel_um_order_by_exchange_id(
        &self,
        exchange_order_id: i64,
        symbol: &str,
    ) -> Result<OrderResponse, BinancePapiError> {
        let body = format!("symbol={}&orderId={}", symbol, exchange_order_id);

        let resp: OrderResponseResult = self.delete("/papi/v1/um/order", body).await?;
        match resp {
            OrderResponseResult::Ok(resp) => Ok(resp),
            OrderResponseResult::Err(resp) => Err(BinancePapiError::OrderError {
                code: resp.code,
                msg: resp.msg,
            }),
        }
    }

    /// Cancel all UM futures open orders for a symbol
    /// DELETE /papi/v1/um/allOpenOrders
    pub async fn cancel_all_um_orders(&self, symbol: &str) -> Result<(), reqwest::Error> {
        let _: serde_json::Value = self
            .delete("/papi/v1/um/allOpenOrders", format!("symbol={symbol}"))
            .await?;
        Ok(())
    }

    // ========== Account & Position Information ==========

    /// Get PAPI account information
    /// GET /papi/v1/account
    pub async fn get_account_info(&self) -> Result<AccountInfo, reqwest::Error> {
        self.get("/papi/v1/account", String::new()).await
    }

    /// Get PAPI balance
    /// GET /papi/v1/balance
    pub async fn get_balance(&self) -> Result<Vec<Balance>, reqwest::Error> {
        self.get("/papi/v1/balance", String::new()).await
    }

    /// Get UM position information
    /// GET /papi/v1/um/positionRisk
    pub async fn get_um_position_risk(&self) -> Result<Vec<UmPositionRisk>, reqwest::Error> {
        self.get("/papi/v1/um/positionRisk", String::new()).await
    }

    /// Get UM position information for a specific symbol
    /// GET /papi/v1/um/positionRisk?symbol=xxx
    pub async fn get_um_position_risk_for_symbol(
        &self,
        symbol: &str,
    ) -> Result<Vec<UmPositionRisk>, reqwest::Error> {
        self.get("/papi/v1/um/positionRisk", format!("symbol={symbol}"))
            .await
    }
}
