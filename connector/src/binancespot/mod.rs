mod market_data_stream;
mod user_data_stream;
mod msg;
mod rest;
mod ordermanager;  


use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use hftbacktest::{
    prelude::get_precision,
    types::{ErrorKind, LiveError, LiveEvent, Order, Status, Value},
};
use serde::Deserialize;
use thiserror::Error;
use tokio::sync::{broadcast, broadcast::Sender, mpsc::UnboundedSender};
use tokio_tungstenite::tungstenite;
use tracing::{debug, error, warn};

use crate::{
    connector::{Connector, ConnectorBuilder, GetOrders, PublishEvent},
    utils::{ExponentialBackoff, Retry},
};

#[derive(Error, Debug)]
pub enum BinanceSpotError {
    #[error("InstrumentNotFound")]
    InstrumentNotFound,
    #[error("InvalidRequest")]
    InvalidRequest,
    #[error("ListenKeyExpired")]
    ListenKeyExpired,
    #[error("ConnectionInterrupted")]
    ConnectionInterrupted,
    #[error("ConnectionAbort: {0}")]
    ConnectionAbort(String),
    #[error("ReqError: {0:?}")]
    ReqError(#[from] reqwest::Error),
    #[error("OrderError: {code} - {msg})")]
    OrderError { code: i64, msg: String },
    #[error("PrefixUnmatched")]
    PrefixUnmatched,
    #[error("OrderNotFound")]
    OrderNotFound,
    #[error("Tunstenite: {0:?}")]
    Tunstenite(#[from] tungstenite::Error),
    #[error("Config: {0:?}")]
    Config(#[from] toml::de::Error),
}

impl From<BinanceSpotError> for Value {
    fn from(value: BinanceSpotError) -> Value {
        match value {
            BinanceSpotError::InstrumentNotFound => Value::String(value.to_string()),
            BinanceSpotError::InvalidRequest => Value::String(value.to_string()),
            BinanceSpotError::ReqError(error) => {
                let mut map = HashMap::new();
                if let Some(code) = error.status() {
                    map.insert("status_code".to_string(), Value::String(code.to_string()));
                }
                map.insert("msg".to_string(), Value::String(error.to_string()));
                Value::Map(map)
            }
            BinanceSpotError::OrderError { code, msg } => Value::Map({
                let mut map = HashMap::new();
                map.insert("code".to_string(), Value::Int(code));
                map.insert("msg".to_string(), Value::String(msg));
                map
            }),
            BinanceSpotError::Tunstenite(error) => Value::String(format!("{error}")),
            BinanceSpotError::ListenKeyExpired => Value::String(value.to_string()),
            BinanceSpotError::ConnectionInterrupted => Value::String(value.to_string()),
            BinanceSpotError::ConnectionAbort(_) => Value::String(value.to_string()),
            BinanceSpotError::Config(_) => Value::String(value.to_string()),
            BinanceSpotError::PrefixUnmatched => Value::String(value.to_string()),
            BinanceSpotError::OrderNotFound => Value::String(value.to_string()),
        }
    }
}