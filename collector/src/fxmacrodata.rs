//! FXMacroData collector integration for macro, FX, commodity, and rates data.

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use reqwest::{Client, RequestBuilder, Url};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;
use tracing::info;

const DEFAULT_BASE_URL: &str = "https://api.fxmacrodata.com/v1/";

/// Client for FXMacroData REST and GraphQL endpoints.
#[derive(Debug, Clone)]
pub struct FxMacroDataClient {
    client: Client,
    api_key: String,
    base_url: Url,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
struct GraphQlRequest<'a> {
    query: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    variables: Option<Value>,
}

impl FxMacroDataClient {
    /// Builds a client from the `FXMACRODATA_API_KEY` or `FXMD_API_KEY`
    /// environment variable.
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("FXMACRODATA_API_KEY")
            .or_else(|_| std::env::var("FXMD_API_KEY"))
            .context("FXMACRODATA_API_KEY or FXMD_API_KEY must be set")?;
        Self::with_base_url(api_key, DEFAULT_BASE_URL)
    }

    /// Builds a client using the default FXMacroData API URL.
    #[allow(dead_code)]
    pub fn new(api_key: impl Into<String>) -> Self {
        Self::with_base_url(api_key, DEFAULT_BASE_URL).expect("default FXMacroData URL is valid")
    }

    /// Builds a client using a caller-supplied API base URL.
    pub fn with_base_url(api_key: impl Into<String>, base_url: &str) -> Result<Self> {
        // Redirects are not followed, so the API key is never sent to another host.
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .context("failed to build FXMacroData HTTP client")?;
        Ok(Self {
            client,
            api_key: api_key.into(),
            base_url: Url::parse(base_url).context("invalid FXMacroData base URL")?,
        })
    }

    /// Sends a generic GET request to a relative FXMacroData API path.
    #[allow(dead_code)]
    pub async fn request(&self, path: &str, params: &[(&str, String)]) -> Result<Value> {
        self.get_json(path, params).await
    }

    /// Retrieves the FXMacroData data catalogue for a currency.
    pub async fn data_catalogue(&self, currency: &str, params: &[(&str, String)]) -> Result<Value> {
        self.get_json(&format!("data_catalogue/{currency}"), params)
            .await
    }

    /// Retrieves announcement history for a currency and indicator.
    pub async fn announcements(
        &self,
        currency: &str,
        indicator: &str,
        params: &[(&str, String)],
    ) -> Result<Value> {
        self.get_json(&format!("announcements/{currency}/{indicator}"), params)
            .await
    }

    /// Retrieves the latest announcements for a currency.
    pub async fn latest_announcements(
        &self,
        currency: &str,
        params: &[(&str, String)],
    ) -> Result<Value> {
        self.get_json(&format!("announcements/{currency}/latest"), params)
            .await
    }

    /// Retrieves recently changed announcement records.
    pub async fn announcement_changes(&self, params: &[(&str, String)]) -> Result<Value> {
        self.get_json("announcements/changes", params).await
    }

    /// Retrieves the release calendar for a currency.
    pub async fn calendar(&self, currency: &str, params: &[(&str, String)]) -> Result<Value> {
        self.get_json(&format!("calendar/{currency}"), params).await
    }

    /// Retrieves prediction data for a currency and indicator.
    pub async fn predictions(
        &self,
        currency: &str,
        indicator: &str,
        params: &[(&str, String)],
    ) -> Result<Value> {
        self.get_json(&format!("predictions/{currency}/{indicator}"), params)
            .await
    }

    /// Retrieves FX spot history for a base/quote pair.
    pub async fn forex(&self, base: &str, quote: &str, params: &[(&str, String)]) -> Result<Value> {
        self.get_json(&format!("forex/{base}/{quote}"), params)
            .await
    }

    /// Retrieves COT positioning data for a currency.
    pub async fn cot(&self, currency: &str, params: &[(&str, String)]) -> Result<Value> {
        self.get_json(&format!("cot/{currency}"), params).await
    }

    /// Retrieves a commodity series by indicator slug.
    pub async fn commodity(&self, indicator: &str, params: &[(&str, String)]) -> Result<Value> {
        self.get_json(&format!("commodities/{indicator}"), params)
            .await
    }

    /// Retrieves the latest commodity observations.
    pub async fn commodities_latest(&self, params: &[(&str, String)]) -> Result<Value> {
        self.get_json("commodities/latest", params).await
    }

    /// Retrieves curve data for a currency.
    pub async fn curves(&self, currency: &str, params: &[(&str, String)]) -> Result<Value> {
        self.get_json(&format!("curves/{currency}"), params).await
    }

    /// Retrieves curve proxy data for a currency.
    pub async fn curve_proxies(&self, currency: &str, params: &[(&str, String)]) -> Result<Value> {
        self.get_json(&format!("curve_proxies/{currency}"), params)
            .await
    }

    /// Retrieves forward-curve data for a currency.
    pub async fn forward_curves(&self, currency: &str, params: &[(&str, String)]) -> Result<Value> {
        self.get_json(&format!("forward_curves/{currency}"), params)
            .await
    }

    /// Retrieves rate differentials for a currency pair.
    pub async fn rate_differentials(
        &self,
        base: &str,
        quote: &str,
        params: &[(&str, String)],
    ) -> Result<Value> {
        self.get_json(&format!("rate_differentials/{base}/{quote}"), params)
            .await
    }

    /// Retrieves forward-rate differentials for a currency pair.
    pub async fn forward_differentials(
        &self,
        base: &str,
        quote: &str,
        params: &[(&str, String)],
    ) -> Result<Value> {
        self.get_json(&format!("forward_differentials/{base}/{quote}"), params)
            .await
    }

    /// Retrieves current FX market-session data.
    pub async fn market_sessions(&self, params: &[(&str, String)]) -> Result<Value> {
        self.get_json("market_sessions", params).await
    }

    /// Retrieves current risk-sentiment data.
    pub async fn risk_sentiment(&self, params: &[(&str, String)]) -> Result<Value> {
        self.get_json("risk_sentiment", params).await
    }

    /// Retrieves central-bank news for a currency.
    pub async fn news(&self, currency: &str, params: &[(&str, String)]) -> Result<Value> {
        self.get_json(&format!("news/{currency}"), params).await
    }

    /// Retrieves central-bank press releases for a currency.
    pub async fn press_releases(&self, currency: &str, params: &[(&str, String)]) -> Result<Value> {
        self.get_json(&format!("press-releases/{currency}"), params)
            .await
    }

    /// Sends a GraphQL query to FXMacroData.
    #[allow(dead_code)]
    pub async fn graphql(&self, query: &str, variables: Option<Value>) -> Result<Value> {
        let url = self.build_url("graphql", &[])?;
        let request = self
            .client
            .post(url.clone())
            .json(&GraphQlRequest { query, variables });
        let response = self
            .with_api_key(request)
            .send()
            .await
            .context("FXMacroData GraphQL request failed")?;

        Self::parse_response(response, url.as_str()).await
    }

    async fn get_json(&self, path: &str, params: &[(&str, String)]) -> Result<Value> {
        let url = self.build_url(path, params)?;
        let request = self.client.get(url.clone());
        let response = self
            .with_api_key(request)
            .send()
            .await
            .context("FXMacroData request failed")?;

        Self::parse_response(response, url.as_str()).await
    }

    fn build_url(&self, path: &str, params: &[(&str, String)]) -> Result<Url> {
        let mut url = self
            .base_url
            .join(path.trim_start_matches('/'))
            .context("failed to build FXMacroData URL")?;
        if !params.is_empty() {
            let mut query = url.query_pairs_mut();
            for (key, value) in params {
                query.append_pair(key, value);
            }
        }
        Ok(url)
    }

    /// Adds the `X-API-Key` header when an API key is configured.
    fn with_api_key(&self, request: RequestBuilder) -> RequestBuilder {
        if self.api_key.is_empty() {
            request
        } else {
            request.header("X-API-Key", &self.api_key)
        }
    }

    async fn parse_response(response: reqwest::Response, url: &str) -> Result<Value> {
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("FXMacroData HTTP {status} for {url}: {body}"));
        }

        response
            .json::<Value>()
            .await
            .context("failed to parse FXMacroData response")
    }
}

/// Collects FXMacroData payloads for pairs, currencies, indicators, and
/// commodities.
pub async fn run_collection(
    symbols: Vec<String>,
    writer_tx: UnboundedSender<(DateTime<Utc>, String, String)>,
) -> Result<()> {
    let client = FxMacroDataClient::from_env()?;

    emit(
        &writer_tx,
        "fxmacrodata_market_sessions",
        client.market_sessions(&[]).await?,
    );
    emit(
        &writer_tx,
        "fxmacrodata_risk_sentiment",
        client.risk_sentiment(&[]).await?,
    );
    emit(
        &writer_tx,
        "fxmacrodata_commodities_latest",
        client.commodities_latest(&[]).await?,
    );
    emit(
        &writer_tx,
        "fxmacrodata_announcement_changes",
        client.announcement_changes(&[]).await?,
    );

    for symbol in symbols {
        if let Some((base, quote)) = parse_pair(&symbol) {
            emit(
                &writer_tx,
                &format!("fxmacrodata_{base}{quote}_forex"),
                client
                    .forex(&base, &quote, &[("limit", "500".to_string())])
                    .await?,
            );
            emit(
                &writer_tx,
                &format!("fxmacrodata_{base}{quote}_rate_differentials"),
                client.rate_differentials(&base, &quote, &[]).await?,
            );
            emit(
                &writer_tx,
                &format!("fxmacrodata_{base}{quote}_forward_differentials"),
                client.forward_differentials(&base, &quote, &[]).await?,
            );
            continue;
        }

        if let Some((currency, indicator)) = parse_indicator_spec(&symbol) {
            emit(
                &writer_tx,
                &format!("fxmacrodata_{currency}_{indicator}_announcements"),
                client.announcements(&currency, &indicator, &[]).await?,
            );
            emit(
                &writer_tx,
                &format!("fxmacrodata_{currency}_{indicator}_predictions"),
                client.predictions(&currency, &indicator, &[]).await?,
            );
            continue;
        }

        if let Some(indicator) = parse_commodity_spec(&symbol) {
            emit(
                &writer_tx,
                &format!("fxmacrodata_commodity_{indicator}"),
                client.commodity(&indicator, &[]).await?,
            );
            continue;
        }

        let currency = normalize_currency(&symbol)?;
        info!(%currency, "collecting FXMacroData currency context");
        emit(
            &writer_tx,
            &format!("fxmacrodata_{currency}_catalogue"),
            client.data_catalogue(&currency, &[]).await?,
        );
        emit(
            &writer_tx,
            &format!("fxmacrodata_{currency}_latest_announcements"),
            client.latest_announcements(&currency, &[]).await?,
        );
        emit(
            &writer_tx,
            &format!("fxmacrodata_{currency}_calendar"),
            client.calendar(&currency, &[]).await?,
        );
        emit(
            &writer_tx,
            &format!("fxmacrodata_{currency}_cot"),
            client.cot(&currency, &[]).await?,
        );
        emit(
            &writer_tx,
            &format!("fxmacrodata_{currency}_curves"),
            client.curves(&currency, &[]).await?,
        );
        emit(
            &writer_tx,
            &format!("fxmacrodata_{currency}_curve_proxies"),
            client.curve_proxies(&currency, &[]).await?,
        );
        emit(
            &writer_tx,
            &format!("fxmacrodata_{currency}_forward_curves"),
            client.forward_curves(&currency, &[]).await?,
        );
        emit(
            &writer_tx,
            &format!("fxmacrodata_{currency}_news"),
            client.news(&currency, &[]).await?,
        );
        emit(
            &writer_tx,
            &format!("fxmacrodata_{currency}_press_releases"),
            client.press_releases(&currency, &[]).await?,
        );
    }

    Ok(())
}

fn emit(writer_tx: &UnboundedSender<(DateTime<Utc>, String, String)>, label: &str, data: Value) {
    let _ = writer_tx.send((Utc::now(), label.to_string(), data.to_string()));
}

fn parse_pair(symbol: &str) -> Option<(String, String)> {
    let normalized = symbol
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .collect::<String>()
        .to_lowercase();

    if normalized.len() == 6 {
        Some((normalized[..3].to_string(), normalized[3..].to_string()))
    } else {
        None
    }
}

fn parse_indicator_spec(symbol: &str) -> Option<(String, String)> {
    let (currency, indicator) = symbol.split_once(':')?;
    if currency.eq_ignore_ascii_case("commodity") {
        return None;
    }

    let currency = normalize_currency(currency).ok()?;
    let indicator = indicator.trim().to_lowercase();
    if indicator.is_empty() {
        None
    } else {
        Some((currency, indicator))
    }
}

fn parse_commodity_spec(symbol: &str) -> Option<String> {
    let (prefix, indicator) = symbol.split_once(':')?;
    if !prefix.eq_ignore_ascii_case("commodity") {
        return None;
    }

    let indicator = indicator.trim().to_lowercase();
    if indicator.is_empty() {
        None
    } else {
        Some(indicator)
    }
}

fn normalize_currency(currency: &str) -> Result<String> {
    let normalized = currency.trim().to_lowercase();
    if normalized.len() == 3 && normalized.chars().all(|c| c.is_ascii_alphabetic()) {
        Ok(normalized)
    } else {
        Err(anyhow!(
            "FXMacroData symbols must be currencies like usd, pairs like eurusd, or specs like usd:non_farm_payrolls"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_urls_under_api_v1() {
        let client = FxMacroDataClient::with_base_url("test-key", "https://example.com/api/v1/")
            .expect("valid base URL");
        let url = client
            .build_url("forex/eur/usd", &[("limit", "1".to_string())])
            .expect("URL should build");

        assert_eq!(
            url.as_str(),
            "https://example.com/api/v1/forex/eur/usd?limit=1"
        );
    }

    #[test]
    fn sends_api_key_header() {
        let client = FxMacroDataClient::with_base_url("test-key", "https://example.com/api/v1/")
            .expect("valid base URL");
        let url = client
            .build_url("forex/eur/usd", &[])
            .expect("URL should build");
        let request = client
            .with_api_key(client.client.get(url))
            .build()
            .expect("request should build");

        assert_eq!(
            request.url().as_str(),
            "https://example.com/api/v1/forex/eur/usd"
        );
        assert_eq!(request.headers()["X-API-Key"], "test-key");
    }

    #[test]
    fn omits_api_key_header_without_key() {
        let client = FxMacroDataClient::with_base_url("", "https://example.com/api/v1/")
            .expect("valid base URL");
        let url = client
            .build_url("calendar/usd", &[])
            .expect("URL should build");
        let request = client
            .with_api_key(client.client.get(url))
            .build()
            .expect("request should build");

        assert!(request.headers().get("X-API-Key").is_none());
    }

    #[test]
    fn parses_currency_pairs() {
        assert_eq!(
            parse_pair("EUR/USD"),
            Some(("eur".to_string(), "usd".to_string()))
        );
        assert_eq!(
            parse_pair("gbpjpy"),
            Some(("gbp".to_string(), "jpy".to_string()))
        );
        assert_eq!(parse_pair("usd"), None);
    }

    #[test]
    fn parses_indicator_specs() {
        assert_eq!(
            parse_indicator_spec("USD:non_farm_payrolls"),
            Some(("usd".to_string(), "non_farm_payrolls".to_string()))
        );
        assert_eq!(parse_indicator_spec("eurusd"), None);
        assert_eq!(parse_indicator_spec("commodity:brent"), None);
    }

    #[test]
    fn parses_commodity_specs() {
        assert_eq!(
            parse_commodity_spec("commodity:brent"),
            Some("brent".to_string())
        );
        assert_eq!(parse_commodity_spec("USD:non_farm_payrolls"), None);
    }
}
