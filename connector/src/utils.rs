use std::{
    fmt,
    fmt::{Debug, Write},
    future::Future,
    marker::PhantomData,
    time::{Duration, Instant},
};

use hashbrown::Equivalent;
use hftbacktest::prelude::OrderId;
use hmac::{Hmac, KeyInit, Mac};
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use serde::{
    de,
    de::{Error, Visitor},
    Deserialize,
    Deserializer,
};
use sha2::Sha256;

use crate::bybit::BybitError;

struct I64Visitor;

impl<'de> Visitor<'de> for I64Visitor {
    type Value = i64;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string containing an i64 number")
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        s.parse::<i64>().map_err(Error::custom)
    }
}

struct OptionF64Visitor;

impl<'de> Visitor<'de> for OptionF64Visitor {
    type Value = Option<f64>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string containing an f64 number")
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: Error,
    {
        Ok(None)
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        match deserializer.deserialize_str(F64Visitor) {
            Ok(num) => Ok(Some(num)),
            Err(e) => {
                // fixme: dirty
                if format!("{e:?}").starts_with("Error(\"cannot parse float from empty string\"") {
                    Ok(None)
                } else {
                    Err(e)
                }
            }
        }
    }
}

struct F64Visitor;

impl<'de> Visitor<'de> for F64Visitor {
    type Value = f64;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string containing an f64 number")
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        s.parse::<f64>().map_err(Error::custom)
    }
}

pub fn from_str_to_i64<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_str(I64Visitor)
}

pub fn from_str_to_f64<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_str(F64Visitor)
}

pub fn from_str_to_f64_opt<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_option(OptionF64Visitor)
}

pub fn to_uppercase<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &str = Deserialize::deserialize(deserializer)?;
    Ok(s.to_uppercase())
}

pub fn to_lowercase<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &str = Deserialize::deserialize(deserializer)?;
    Ok(s.to_lowercase())
}

pub fn sign_hmac_sha256(secret: &str, s: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(s.as_bytes());
    let hash = mac.finalize().into_bytes();
    let mut tmp = String::with_capacity(hash.len() * 2);
    for c in hash {
        write!(&mut tmp, "{:02x}", c).unwrap();
    }
    tmp
}

pub type PxQty = (f64, f64);

pub fn parse_depth(
    bids: Vec<(String, String)>,
    asks: Vec<(String, String)>,
) -> Result<(Vec<PxQty>, Vec<PxQty>), BybitError> {
    let mut bids_ = Vec::with_capacity(bids.len());
    for (px, qty) in bids {
        bids_.push(parse_px_qty_tup(px, qty)?);
    }
    let mut asks_ = Vec::with_capacity(asks.len());
    for (px, qty) in asks {
        asks_.push(parse_px_qty_tup(px, qty)?);
    }
    Ok((bids_, asks_))
}

pub fn parse_px_qty_tup(px: String, qty: String) -> Result<PxQty, BybitError> {
    Ok((px.parse()?, qty.parse()?))
}

pub trait BackoffStrategy {
    fn backoff(&mut self) -> Duration;
}

pub struct ExponentialBackoff {
    last_attempt: Instant,
    attempts: i32,
    factor: u32,
    last_delay: Option<Duration>,
    reset_interval: Option<Duration>,
    min_delay: Duration,
    max_delay: Option<Duration>,
}

impl Default for ExponentialBackoff {
    fn default() -> Self {
        Self {
            last_attempt: Instant::now(),
            attempts: 0,
            factor: 2,
            last_delay: None,
            reset_interval: Some(Duration::from_secs(300)),
            min_delay: Duration::from_millis(100),
            max_delay: Some(Duration::from_secs(60)),
        }
    }
}

impl BackoffStrategy for ExponentialBackoff {
    fn backoff(&mut self) -> Duration {
        if let Some(reset_interval) = self.reset_interval {
            if self.last_attempt.elapsed() > reset_interval {
                self.attempts = 0;
            }
        }

        self.last_attempt = Instant::now();
        self.attempts += 1;

        match self.last_delay {
            None => {
                self.last_delay = Some(self.min_delay);
                self.min_delay
            }
            Some(last_delay) => {
                let mut delay = last_delay.saturating_mul(self.factor);

                if let Some(max_delay) = self.max_delay {
                    if delay > max_delay {
                        delay = max_delay;
                    }
                }
                self.last_delay = Some(delay);
                delay
            }
        }
    }
}

pub struct Retry<O, E, Backoff, ErrorHandler> {
    backoff: Backoff,
    error_handler: Option<ErrorHandler>,
    _o_marker: PhantomData<O>,
    _e_marker: PhantomData<E>,
}

impl<O, E, Backoff, ErrorHandler> Retry<O, E, Backoff, ErrorHandler>
where
    E: Debug,
    Backoff: BackoffStrategy,
    ErrorHandler: FnMut(E) -> Result<(), E>,
{
    pub fn new(backoff: Backoff) -> Self {
        Self {
            backoff,
            error_handler: None,
            _o_marker: Default::default(),
            _e_marker: Default::default(),
        }
    }

    pub fn error_handler(self, error_handler: ErrorHandler) -> Self {
        Self {
            error_handler: Some(error_handler),
            ..self
        }
    }

    pub async fn retry<F, Fut>(&mut self, func: F) -> Result<O, E>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<O, E>>,
    {
        loop {
            match func().await {
                Ok(o) => return Ok(o),
                Err(error) => {
                    if let Some(error_handler) = self.error_handler.as_mut() {
                        error_handler(error)?;
                    }
                    tokio::time::sleep(self.backoff.backoff()).await;
                }
            }
        }
    }
}

#[derive(Eq, Hash, PartialEq, Debug)]
pub struct SymbolOrderId {
    pub symbol: String,
    pub order_id: OrderId,
}

impl SymbolOrderId {
    pub fn new(symbol: String, order_id: OrderId) -> Self {
        Self { symbol, order_id }
    }
}

#[derive(Eq, Hash, PartialEq, Debug)]
pub struct RefSymbolOrderId<'a> {
    pub symbol: &'a str,
    pub order_id: OrderId,
}

impl<'a> RefSymbolOrderId<'a> {
    pub fn new(symbol: &'a str, order_id: OrderId) -> Self {
        Self { symbol, order_id }
    }
}

impl Equivalent<SymbolOrderId> for RefSymbolOrderId<'_> {
    fn equivalent(&self, key: &SymbolOrderId) -> bool {
        key.symbol == self.symbol && key.order_id == self.order_id
    }
}

pub fn generate_rand_string(length: usize) -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(length)
        .map(char::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use hashbrown::HashMap;

    use crate::utils::{RefSymbolOrderId, SymbolOrderId};

    #[test]
    fn equivalent_symbol_order_id() {
        let mut map = HashMap::new();
        map.insert(
            SymbolOrderId::new("key1".to_string(), 1),
            "value1".to_string(),
        );

        assert_eq!(
            map.get(&RefSymbolOrderId::new("key1", 1)).unwrap(),
            "value1"
        )
    }
}
