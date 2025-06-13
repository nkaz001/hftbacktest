use chrono::Utc;
use hftbacktest::types::{OrdType, Side, TimeInForce};
use serde::Deserialize;
use super::msg::{rest};
use crate::utils::sign_hmac_sha256;


#[derive(Clone)]
pub struct BinanceSpotClient {
    client: reqwest::Client,
    url: String,
    api_key: String,
    secret: String,
}

impl BinanceSpotClient {
    pub fn new(url: &str, api_key: &str, secret: &str) -> Self {
        let client = reqwest::Client::new();
        Self {
            client,
            url: url.to_string(),
            api_key: api_key.to_string(),
            secret: secret.to_string(),
        }
    }
    async fn get_noauth<T: for<'a> Deserialize<'a>>(
        &self,
        path: &str,
        query: String,
    ) -> Result<T, reqwest::Error> {
        let resp = self
            .client
            .get(format!("{}{}?{}", self.url, path, query))
            .header("Accept", "application/json")
            .send()
            .await?
            .json()
            .await?;
        Ok(resp)
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
                self.url, path, query, signature
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
        let sign_body = format!("recvWindow=5000&timestamp={}{}", time, body);
        let signature = sign_hmac_sha256(&self.secret, &sign_body);
        let resp = self
            .client
            .put(format!(
                "{}{}?recvWindow=5000&timestamp={}&signature={}",
                self.url, path, time, signature
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
        let sign_body = format!("recvWindow=5000&timestamp={}{}", time, body);
        let signature = sign_hmac_sha256(&self.secret, &sign_body);
        let resp = self
            .client
            .post(format!(
                "{}{}?recvWindow=5000&timestamp={}&signature={}",
                self.url, path, time, signature
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
        let sign_body = format!("recvWindow=5000&timestamp={}{}", time, body);
        let signature = sign_hmac_sha256(&self.secret, &sign_body);
        let resp = self
            .client
            .delete(format!(
                "{}{}?recvWindow=5000&timestamp={}&signature={}",
                self.url, path, time, signature
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

    pub async fn get_depth(&self, symbol: &str) -> Result<rest::Depth, reqwest::Error> {
        let resp: rest::Depth = self
            .get_noauth("/api/v1/depth", format!("symbol={}&limit=1000", symbol))
            .await?;
        Ok(resp)
    }
}