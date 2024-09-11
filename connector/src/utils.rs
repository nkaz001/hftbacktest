use std::{
    fmt,
    fmt::{Debug, Write},
    future::Future,
    time::Duration,
};

use hmac::{Hmac, KeyInit, Mac};
use rand::{distributions::Alphanumeric, Rng};
use serde::{
    de,
    de::{Error, Visitor},
    Deserializer,
};
use sha2::Sha256;
use tracing::error;

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

pub fn gen_random_string(len: usize) -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}

pub async fn retry<F, Fut, O, E>(func: F) -> Result<O, E>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<O, E>>,
    E: Debug,
{
    let mut backoff = 500;
    loop {
        match func().await {
            Ok(o) => return Ok(o),
            Err(error) => {
                error!(?error, "Retrying...");
                tokio::time::sleep(Duration::from_millis(backoff)).await;
                backoff *= 2;
                backoff = backoff.max(60_000);
            }
        }
    }
}
