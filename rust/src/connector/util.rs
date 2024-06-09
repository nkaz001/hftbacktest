use std::{collections::HashMap, fmt, fmt::Write, hash::Hash};

use hmac::{Hmac, KeyInit, Mac};
use serde::{
    de,
    de::{Error, Visitor},
    Deserializer,
};
use sha2::Sha256;

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

struct F32Visitor;

impl<'de> Visitor<'de> for F32Visitor {
    type Value = f32;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string containing an f32 number")
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        s.parse::<f32>().map_err(Error::custom)
    }
}

struct OptionF32Visitor;

impl<'de> Visitor<'de> for OptionF32Visitor {
    type Value = Option<f32>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a string containing an f32 number")
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
        Ok(Some(deserializer.deserialize_str(F32Visitor)?))
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

pub fn from_str_to_f32<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_str(F32Visitor)
}

pub fn from_str_to_f64<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_str(F64Visitor)
}

pub fn from_str_to_f32_opt<'de, D>(deserializer: D) -> Result<Option<f32>, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_option(OptionF32Visitor)
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
