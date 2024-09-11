use hftbacktest::types::{OrdType, Side, Status, TimeInForce};
use serde::{
    de::{Error, Unexpected},
    Deserialize,
    Deserializer,
};

#[allow(dead_code)]
pub mod rest;
#[allow(dead_code)]
pub mod stream;

fn from_str_to_side<'de, D>(deserializer: D) -> Result<Side, D::Error>
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

fn from_str_to_status<'de, D>(deserializer: D) -> Result<Status, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &str = Deserialize::deserialize(deserializer)?;
    match s {
        "NEW" => Ok(Status::New),
        "PARTIALLY_FILLED" => Ok(Status::PartiallyFilled),
        "FILLED" => Ok(Status::Filled),
        "CANCELED" => Ok(Status::Canceled),
        // "REJECTED" => Ok(Status::Rejected),
        "EXPIRED" => Ok(Status::Expired),
        // "EXPIRED_IN_MATCH" => Ok(Status::ExpiredInMatch),
        s => Err(Error::invalid_value(
            Unexpected::Other(s),
            &"NEW,PARTIALLY_FILLED,FILLED,CANCELED,EXPIRED",
        )),
    }
}

fn from_str_to_type<'de, D>(deserializer: D) -> Result<OrdType, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &str = Deserialize::deserialize(deserializer)?;
    match s {
        "LIMIT" => Ok(OrdType::Limit),
        "MARKET" => Ok(OrdType::Market),
        // "STOP" => Ok(OrdType::StopLimit),
        // "TAKE_PROFIT" => Ok(OrdType::TakeProfitLimit),
        // "STOP_MARKET" => Ok(OrdType::StopMarket),
        // "TAKE_PROFIT_MARKET" => Ok(OrdType::TakeProfitMarket),
        // "TRAILING_STOP_MARKET" => Ok(OrdType::TrailingStopMarket),
        s => Err(Error::invalid_value(Unexpected::Other(s), &"LIMIT,MARKET")),
    }
}

fn from_str_to_tif<'de, D>(deserializer: D) -> Result<TimeInForce, D::Error>
where
    D: Deserializer<'de>,
{
    let s: &str = Deserialize::deserialize(deserializer)?;
    match s {
        "GTC" => Ok(TimeInForce::GTC),
        "IOC" => Ok(TimeInForce::IOC),
        "FOK" => Ok(TimeInForce::FOK),
        "GTX" => Ok(TimeInForce::GTX),
        // "GTD" => Ok(TimeInForce::GTD),
        s => Err(Error::invalid_value(
            Unexpected::Other(s),
            &"GTC,IOC,FOK,GTX",
        )),
    }
}
