use chrono::Utc;
use serde::Deserialize;

use crate::{
    bybit::{
        BybitError,
        msg::{Position, RestResponse},
    },
    utils::sign_hmac_sha256,
};

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
}
