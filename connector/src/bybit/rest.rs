use chrono::Utc;
use serde::Deserialize;

use crate::{
    bybit::{
        BybitError,
        msg::{
            ClosedPnlEntry, ClosedPnlResponse, Order as BybitOrder, OrderResponseData, Position,
            RestResponse, TransactionLogEntry, TransactionLogResponse,
        },
    },
    utils::sign_hmac_sha256,
};

/// REST API response for order operations
#[derive(Deserialize, Debug)]
pub struct OrderRestResponse {
    #[serde(rename = "retCode")]
    pub ret_code: i64,
    #[serde(rename = "retMsg")]
    pub ret_msg: String,
    pub result: OrderResponseData,
    #[serde(rename = "retExtInfo")]
    pub ret_ext_info: serde_json::Value,
    pub time: i64,
}

#[derive(Clone)]
pub struct BybitClient {
    client: reqwest::Client,
    url: String,
    api_key: String,
    secret: String,
}

impl BybitClient {
    pub fn new(url: &str, api_key: &str, secret: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            url: url.to_string(),
            api_key: api_key.to_string(),
            secret: secret.to_string(),
        }
    }

    async fn get<T: for<'a> Deserialize<'a>>(
        &self,
        path: &str,
        query: &str,
        api_key: &str,
        secret: &str,
    ) -> Result<T, reqwest::Error> {
        let time = Utc::now().timestamp_millis() - 1000;
        let sign_body = format!("{time}{api_key}5000{query}");
        let signature = sign_hmac_sha256(secret, &sign_body);
        let resp = self
            .client
            .get(format!("{}{}?{}", self.url, path, query))
            .header("Accept", "application/json")
            .header("X-BAPI-SIGN", signature)
            .header("X-BAPI-API-KEY", api_key)
            .header("X-BAPI-TIMESTAMP", time)
            .header("X-BAPI-RECV-WINDOW", "5000")
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
        let sign_body = format!("{time}{api_key}5000{body}");
        let signature = sign_hmac_sha256(secret, &sign_body);
        let resp = self
            .client
            .post(format!("{}{}", self.url, path))
            .header("Accept", "application/json")
            .header("X-BAPI-SIGN", signature)
            .header("X-BAPI-API-KEY", api_key)
            .header("X-BAPI-TIMESTAMP", time)
            .header("X-BAPI-RECV-WINDOW", "5000")
            .body(body)
            .send()
            .await?
            .json()
            .await?;
        Ok(resp)
    }

    pub async fn cancel_all_orders(&self, category: &str, symbol: &str) -> Result<(), BybitError> {
        let resp: RestResponse = self
            .post(
                "/v5/order/cancel-all",
                format!("{{\"category\":\"{category}\",\"symbol\":\"{symbol}\"}}"),
                &self.api_key,
                &self.secret,
            )
            .await?;
        if resp.result.success != "1" {
            Err(BybitError::OpError(resp.ret_msg))
        } else {
            Ok(())
        }
    }

    pub async fn get_position_information(
        &self,
        category: &str,
        symbol: &str,
    ) -> Result<Vec<Position>, BybitError> {
        let resp: RestResponse = self
            .get(
                "/v5/position/list",
                &format!("category={category}&symbol={symbol}"),
                &self.api_key,
                &self.secret,
            )
            .await?;
        if resp.ret_code != 0 {
            Err(BybitError::OpError(resp.ret_msg))
        } else {
            let position: Vec<Position> = serde_json::from_value(resp.result.list.unwrap())?;
            Ok(position)
        }
    }

    /// Submit a new order via REST API
    pub async fn submit_order(&self, order: &BybitOrder) -> Result<OrderResponseData, BybitError> {
        let body = serde_json::to_string(order)?;
        let resp: OrderRestResponse = self
            .post("/v5/order/create", body, &self.api_key, &self.secret)
            .await?;
        if resp.ret_code != 0 {
            Err(BybitError::OrderError {
                code: resp.ret_code,
                msg: resp.ret_msg,
            })
        } else {
            Ok(resp.result)
        }
    }

    /// Cancel an order via REST API
    pub async fn cancel_order(
        &self,
        category: &str,
        symbol: &str,
        order_link_id: &str,
    ) -> Result<OrderResponseData, BybitError> {
        let body = format!(
            r#"{{"category":"{}","symbol":"{}","orderLinkId":"{}"}}"#,
            category, symbol, order_link_id
        );
        let resp: OrderRestResponse = self
            .post("/v5/order/cancel", body, &self.api_key, &self.secret)
            .await?;
        if resp.ret_code != 0 {
            Err(BybitError::OrderError {
                code: resp.ret_code,
                msg: resp.ret_msg,
            })
        } else {
            Ok(resp.result)
        }
    }

    /// Get transaction log (fees, PnL, funding, etc.)
    ///
    /// # Arguments
    /// * `account_type` - Account type (UNIFIED for unified account)
    /// * `category` - Category (linear, inverse, option)
    /// * `symbol` - Optional symbol filter
    /// * `start_time` - Optional start time in milliseconds
    /// * `end_time` - Optional end time in milliseconds
    /// * `limit` - Optional limit (default 20, max 50)
    pub async fn get_transaction_log(
        &self,
        account_type: &str,
        category: Option<&str>,
        symbol: Option<&str>,
        start_time: Option<i64>,
        end_time: Option<i64>,
        limit: Option<u32>,
    ) -> Result<Vec<TransactionLogEntry>, BybitError> {
        let mut query = format!("accountType={}", account_type);
        if let Some(c) = category {
            query.push_str("&category=");
            query.push_str(c);
        }
        if let Some(s) = symbol {
            query.push_str("&symbol=");
            query.push_str(s);
        }
        if let Some(t) = start_time {
            query.push_str("&startTime=");
            query.push_str(&t.to_string());
        }
        if let Some(t) = end_time {
            query.push_str("&endTime=");
            query.push_str(&t.to_string());
        }
        if let Some(l) = limit {
            query.push_str("&limit=");
            query.push_str(&l.to_string());
        }
        let resp: TransactionLogResponse = self
            .get(
                "/v5/account/transaction-log",
                &query,
                &self.api_key,
                &self.secret,
            )
            .await?;
        if resp.ret_code != 0 {
            Err(BybitError::OpError(resp.ret_msg))
        } else {
            Ok(resp.result.list)
        }
    }

    /// Get closed PnL history
    ///
    /// # Arguments
    /// * `category` - Category (linear, inverse)
    /// * `symbol` - Optional symbol filter
    /// * `start_time` - Optional start time in milliseconds
    /// * `end_time` - Optional end time in milliseconds
    /// * `limit` - Optional limit (default 20, max 100)
    pub async fn get_closed_pnl(
        &self,
        category: &str,
        symbol: Option<&str>,
        start_time: Option<i64>,
        end_time: Option<i64>,
        limit: Option<u32>,
    ) -> Result<Vec<ClosedPnlEntry>, BybitError> {
        let mut query = format!("category={}", category);
        if let Some(s) = symbol {
            query.push_str("&symbol=");
            query.push_str(s);
        }
        if let Some(t) = start_time {
            query.push_str("&startTime=");
            query.push_str(&t.to_string());
        }
        if let Some(t) = end_time {
            query.push_str("&endTime=");
            query.push_str(&t.to_string());
        }
        if let Some(l) = limit {
            query.push_str("&limit=");
            query.push_str(&l.to_string());
        }
        let resp: ClosedPnlResponse = self
            .get(
                "/v5/position/closed-pnl",
                &query,
                &self.api_key,
                &self.secret,
            )
            .await?;
        if resp.ret_code != 0 {
            Err(BybitError::OpError(resp.ret_msg))
        } else {
            Ok(resp.result.list)
        }
    }

    /// Get all positions
    pub async fn get_all_positions(
        &self,
        category: &str,
        settle_coin: Option<&str>,
    ) -> Result<Vec<Position>, BybitError> {
        let mut query = format!("category={}", category);
        if let Some(s) = settle_coin {
            query.push_str("&settleCoin=");
            query.push_str(s);
        }
        let resp: RestResponse = self
            .get("/v5/position/list", &query, &self.api_key, &self.secret)
            .await?;
        if resp.ret_code != 0 {
            Err(BybitError::OpError(resp.ret_msg))
        } else {
            let positions: Vec<Position> = serde_json::from_value(resp.result.list.unwrap())?;
            Ok(positions)
        }
    }
}
