use hftbacktest::types::{OrdType, Side, Status, TimeInForce};
use serde::{
    Deserialize, Deserializer,
    de::{Error, Unexpected},
};

#[allow(dead_code)]
pub mod rest;
#[allow(dead_code)]
pub mod stream;

pub fn from_str_to_side<'de, D>(deserializer: D) -> Result<Side, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &str = Deserialize::deserialize(deserializer)?;
    match s {
        "BUY" => Ok(Side::Buy),
        "SELL" => Ok(Side::Sell),
        s => Err(Error::invalid_value(Unexpected::Other(s), &"BUY or SELL")),
    }
}

pub fn from_str_to_status<'de, D>(deserializer: D) -> Result<Status, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &str = Deserialize::deserialize(deserializer)?;
    match s {
        "NEW" => Ok(Status::New),
        "PARTIALLY_FILLED" => Ok(Status::PartiallyFilled),
        "FILLED" => Ok(Status::Filled),
        "CANCELED" => Ok(Status::Canceled),
        "EXPIRED" => Ok(Status::Expired),
        s => Err(Error::invalid_value(
            Unexpected::Other(s),
            &"NEW,PARTIALLY_FILLED,FILLED,CANCELED,EXPIRED",
        )),
    }
}

pub fn from_str_to_type<'de, D>(deserializer: D) -> Result<OrdType, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &str = Deserialize::deserialize(deserializer)?;
    match s {
        "LIMIT" => Ok(OrdType::Limit),
        "MARKET" => Ok(OrdType::Market),
        s => Err(Error::invalid_value(Unexpected::Other(s), &"LIMIT,MARKET")),
    }
}

pub fn from_str_to_tif<'de, D>(deserializer: D) -> Result<TimeInForce, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &str = Deserialize::deserialize(deserializer)?;
    match s {
        "GTC" => Ok(TimeInForce::GTC),
        "IOC" => Ok(TimeInForce::IOC),
        "FOK" => Ok(TimeInForce::FOK),
        "GTX" => Ok(TimeInForce::GTX),
        s => Err(Error::invalid_value(
            Unexpected::Other(s),
            &"GTC,IOC,FOK,GTX",
        )),
    }
}
