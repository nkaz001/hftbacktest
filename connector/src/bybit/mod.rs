use std::{
    collections::{HashMap, HashSet},
    num::{ParseFloatError, ParseIntError},
    sync::{Arc, Mutex},
};

use hftbacktest::types::{ErrorKind, LiveError, LiveEvent, Order, Value};
use serde::Deserialize;
use thiserror::Error;
use tokio::sync::{broadcast, broadcast::Sender, mpsc::UnboundedSender};
use tracing::error;

use crate::{
    bybit::{
        ordermanager::{OrderManager, SharedOrderManager},
        public_stream::PublicStream,
        rest::BybitClient,
        trade_stream::OrderOp,
    },
    connector::{Connector, ConnectorBuilder, GetOrders, PublishEvent},
    utils::{ExponentialBackoff, Retry},
};

#[allow(dead_code)]
mod msg;
mod ordermanager;
mod private_stream;
mod public_stream;
mod rest;
mod trade_stream;

#[derive(Error, Debug)]
pub enum BybitError {
    #[error("AssetNotFound")]
    AssetNotFound,
    #[error("AuthError: {code} - {msg}")]
    AuthError { code: i64, msg: String },
    #[error("OrderError: {code} - {msg}")]
    OrderError { code: i64, msg: String },
    #[error("InvalidPxQty: {0}")]
    InvalidPxQty(#[from] ParseFloatError),
    #[error("InvalidOrderId: {0}")]
    InvalidOrderId(ParseIntError),
    #[error("PrefixUnmatched")]
    PrefixUnmatched,
    #[error("OrderNotFound")]
    OrderNotFound,
    #[error("InvalidReqId")]
    InvalidReqId,
    #[error("InvalidArg: {0}")]
    InvalidArg(&'static str),
    #[error("OrderAlreadyExist")]
    OrderAlreadyExist,
    #[error("Serde: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Reqwest: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("Tungstenite: {0}")]
    Tungstenite(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("ConnectionAbort: {0}")]
    ConnectionAbort(String),
    #[error("ConnectionInterrupted")]
    ConnectionInterrupted,
    #[error("OpError: {0}")]
    OpError(String),
    #[error("Config: {0:?}")]
    Config(#[from] toml::de::Error),
}

impl BybitError {
    pub fn to_value(&self) -> Value {
        match self {
            BybitError::AssetNotFound => Value::Empty,
            BybitError::AuthError { code, msg } => Value::Map({
                let mut map = HashMap::new();
                map.insert("code".to_string(), Value::Int(*code));
                map.insert("msg".to_string(), Value::String(msg.clone()));
                map
            }),
            BybitError::OrderError { code, msg } => Value::Map({
                let mut map = HashMap::new();
                map.insert("code".to_string(), Value::Int(*code));
                map.insert("msg".to_string(), Value::String(msg.clone()));
                map
            }),
            BybitError::InvalidPxQty(_) => Value::String(self.to_string()),
            BybitError::InvalidOrderId(_) => Value::String(self.to_string()),
            BybitError::PrefixUnmatched => Value::String(self.to_string()),
            BybitError::OrderNotFound => Value::String(self.to_string()),
            BybitError::InvalidReqId => Value::String(self.to_string()),
            BybitError::InvalidArg(_) => Value::String(self.to_string()),
            BybitError::OrderAlreadyExist => Value::String(self.to_string()),
            BybitError::Serde(_) => Value::String(self.to_string()),
            BybitError::Tungstenite(_) => Value::String(self.to_string()),
            BybitError::ConnectionAbort(_) => Value::String(self.to_string()),
            BybitError::ConnectionInterrupted => Value::String(self.to_string()),
            BybitError::OpError(_) => Value::String(self.to_string()),
            BybitError::Reqwest(_) => Value::String(self.to_string()),
            BybitError::Config(_) => Value::String(self.to_string()),
        }
    }
}

#[derive(Deserialize)]
pub struct Config {
    public_url: String,
    private_url: String,
    trade_url: String,
    rest_url: String,
    api_key: String,
    secret: String,
    category: String,
    order_prefix: String,
}

type SharedSymbolSet = Arc<Mutex<HashSet<String>>>;

pub struct Bybit {
    config: Config,
    order_tx: Sender<OrderOp>,
    order_manager: SharedOrderManager,
    symbols: SharedSymbolSet,
    client: BybitClient,
    symbol_tx: Sender<String>,
}

impl Bybit {
    fn connect_public_stream(&self, ev_tx: UnboundedSender<PublishEvent>) {
        // Connects to the public stream for the market data.
        let public_url = self.config.public_url.clone();
        let symbol_tx = self.symbol_tx.clone();

        tokio::spawn(async move {
            let _ = Retry::new(ExponentialBackoff::default())
                .error_handler(|error: BybitError| {
                    error!(?error, "An error occurred in the public stream connection.");
                    ev_tx
                        .send(PublishEvent::LiveEvent(LiveEvent::Error(LiveError::with(
                            ErrorKind::ConnectionInterrupted,
                            error.to_value(),
                        ))))
                        .unwrap();
                    Ok(())
                })
                .retry(|| async {
                    let mut stream = PublicStream::new(ev_tx.clone(), symbol_tx.subscribe());
                    if let Err(error) = stream.connect(&public_url).await {
                        error!(?error, "A connection error occurred.");
                        ev_tx
                            .send(PublishEvent::LiveEvent(LiveEvent::Error(LiveError::with(
                                ErrorKind::ConnectionInterrupted,
                                error.to_value(),
                            ))))
                            .unwrap();
                    } else {
                        ev_tx
                            .send(PublishEvent::LiveEvent(LiveEvent::Error(LiveError::new(
                                ErrorKind::ConnectionInterrupted,
                            ))))
                            .unwrap();
                    }
                    Err::<(), BybitError>(BybitError::ConnectionInterrupted)
                })
                .await;
        });
    }

    fn connect_private_stream(&self, ev_tx: UnboundedSender<PublishEvent>) {
        // Connects to the private stream for the position and order data.
        let private_url = self.config.private_url.clone();
        let api_key = self.config.api_key.clone();
        let secret = self.config.secret.clone();
        let category = self.config.category.clone();
        let order_manager = self.order_manager.clone();
        let instruments = self.symbols.clone();
        let client = self.client.clone();

        tokio::spawn(async move {
            let _ = Retry::new(ExponentialBackoff::default())
                .error_handler(|error: BybitError| {
                    error!(
                        ?error,
                        "An error occurred in the private stream connection."
                    );
                    ev_tx
                        .send(PublishEvent::LiveEvent(LiveEvent::Error(LiveError::with(
                            ErrorKind::ConnectionInterrupted,
                            error.to_value(),
                        ))))
                        .unwrap();
                    Ok(())
                })
                .retry(|| async {
                    let stream = private_stream::PrivateStream::new(
                        api_key.clone(),
                        secret.clone(),
                        ev_tx.clone(),
                        order_manager.clone(),
                        instruments.clone(),
                        client.clone(),
                    );

                    // Cancel all orders before connecting to the stream in order to start with the
                    // clean state.
                    stream.cancel_all(&category).await?;

                    // Fetches the initial states such as positions and open orders.
                    stream.get_all_position(&category).await?;

                    stream.connect(&private_url).await?;
                    Ok(())
                })
                .await;
        });
    }

    fn connect_trade_stream(&self, ev_tx: UnboundedSender<PublishEvent>) {
        let trade_url = self.config.trade_url.clone();
        let api_key = self.config.api_key.clone();
        let secret = self.config.secret.clone();
        let order_manager = self.order_manager.clone();
        let order_tx = self.order_tx.clone();

        tokio::spawn(async move {
            let _ = Retry::new(ExponentialBackoff::default())
                .error_handler(|error: BybitError| {
                    error!(?error, "An error occurred in the trade stream connection.");
                    ev_tx
                        .send(PublishEvent::LiveEvent(LiveEvent::Error(LiveError::with(
                            ErrorKind::ConnectionInterrupted,
                            error.to_value(),
                        ))))
                        .unwrap();
                    Ok(())
                })
                .retry(|| async {
                    let mut stream = trade_stream::TradeStream::new(
                        api_key.clone(),
                        secret.clone(),
                        ev_tx.clone(),
                        order_manager.clone(),
                        order_tx.subscribe(),
                    );
                    stream.connect(&trade_url).await?;
                    Ok(())
                })
                .await;
        });
    }
}

impl ConnectorBuilder for Bybit {
    type Error = BybitError;

    fn build_from(config: &str) -> Result<Self, Self::Error> {
        let config: Config = toml::from_str(config)?;
        if config.order_prefix.contains("/") {
            panic!("order prefix cannot include '/'.");
        }
        if config.order_prefix.len() > 8 {
            panic!("order prefix length should be not greater than 8.");
        }
        let (order_tx, _) = broadcast::channel(500);
        let (symbol_tx, _) = broadcast::channel(500);
        let order_manager = Arc::new(Mutex::new(OrderManager::new(&config.order_prefix)));
        let client = BybitClient::new(&config.rest_url, &config.api_key, &config.secret);
        Ok(Bybit {
            config,
            order_tx,
            order_manager,
            client,
            symbols: Default::default(),
            symbol_tx,
        })
    }
}

impl Connector for Bybit {
    fn register(&mut self, symbol: String) {
        let mut symbols = self.symbols.lock().unwrap();
        if !symbols.contains(&symbol) {
            symbols.insert(symbol.clone());
            self.symbol_tx.send(symbol).unwrap();
        }
    }

    fn order_manager(&self) -> Arc<Mutex<dyn GetOrders + Send + 'static>> {
        self.order_manager.clone()
    }

    fn run(&mut self, ev_tx: UnboundedSender<PublishEvent>) {
        self.connect_public_stream(ev_tx.clone());
        self.connect_private_stream(ev_tx.clone());
        self.connect_trade_stream(ev_tx);
    }

    fn submit(&self, asset: String, order: Order, _ev_tx: UnboundedSender<PublishEvent>) {
        let bybit_order = self
            .order_manager
            .lock()
            .unwrap()
            .new_order(&asset, &self.config.category, order)
            .unwrap();
        self.order_tx
            .send(OrderOp {
                op: "order.create",
                bybit_order,
            })
            .unwrap();
    }

    fn cancel(&self, asset: String, order: Order, _ev_tx: UnboundedSender<PublishEvent>) {
        let bybit_order = self
            .order_manager
            .lock()
            .unwrap()
            .cancel_order(&asset, &self.config.category, order.order_id)
            .unwrap();
        self.order_tx
            .send(OrderOp {
                op: "order.cancel",
                bybit_order,
            })
            .unwrap();
    }
}
