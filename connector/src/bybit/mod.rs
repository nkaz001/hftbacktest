use std::{
    collections::{HashMap, HashSet},
    num::{ParseFloatError, ParseIntError},
    sync::{Arc, Mutex},
};

use hftbacktest::types::{BuildError, ErrorKind, LiveError, LiveEvent, Order, Value};
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
    connector::{Connector, Instrument},
    utils::retry,
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
        }
    }
}

/// Bybit connector [`Bybit`] builder.
/// Currently only `linear` category (linear futures) is supported.
pub struct BybitBuilder {
    public_url: String,
    private_url: String,
    trade_url: String,
    rest_url: String,
    topics: HashSet<String>,
    api_key: String,
    secret: String,
    category: String,
    order_prefix: String,
}

impl BybitBuilder {
    /// Sets the public Websocket stream endpoint url.
    pub fn public_url<E: Into<String>>(self, endpoint: E) -> Self {
        Self {
            public_url: endpoint.into(),
            ..self
        }
    }

    /// Sets the private Websocket stream endpoint url.
    pub fn private_url<E: Into<String>>(self, endpoint: E) -> Self {
        Self {
            private_url: endpoint.into(),
            ..self
        }
    }

    /// Sets the trade Websocket stream endpoint url.
    pub fn trade_url<E: Into<String>>(self, endpoint: E) -> Self {
        Self {
            trade_url: endpoint.into(),
            ..self
        }
    }

    /// Sets the REST API endpoint url.
    pub fn rest_url<E: Into<String>>(self, endpoint: E) -> Self {
        Self {
            rest_url: endpoint.into(),
            ..self
        }
    }

    /// Sets the API key
    pub fn category(self, category: &str) -> Self {
        Self {
            category: category.to_string(),
            ..self
        }
    }

    /// Sets the API key
    pub fn api_key(self, api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            ..self
        }
    }

    /// Sets the secret key
    pub fn secret(self, secret: &str) -> Self {
        Self {
            secret: secret.to_string(),
            ..self
        }
    }

    /// Sets the order prefix, which is used to differentiate the orders submitted through this
    /// connector.
    pub fn order_prefix(self, order_prefix: &str) -> Self {
        Self {
            order_prefix: order_prefix.to_string(),
            ..self
        }
    }

    /// Adds an additional topic to receive through the public WebSocket stream.
    pub fn add_topic(mut self, topic: &str) -> Self {
        self.topics.insert(topic.to_string());
        self
    }

    /// Subscribes to the orderbook.1 and orderbook.500 topics to obtain a wider range of depth and
    /// the most frequent updates. `MarketDepth` that can handle data fusion should be used, such as
    /// [FusedHashMapMarketDepth](hftbacktest::depth::fuse::FusedHashMapMarketDepth).
    /// Please see: `<https://bybit-exchange.github.io/docs/v5/websocket/public/orderbook>`
    pub fn subscribe_multiple_depth(mut self) -> Self {
        self.topics.insert("orderbook.1".to_string());
        self.topics.insert("orderbook.500".to_string());
        self
    }

    /// Builds [`Bybit`] connector.
    pub fn build(self) -> Result<Bybit, BuildError> {
        if self.public_url.is_empty() {
            return Err(BuildError::BuilderIncomplete("public_url"));
        }
        if self.private_url.is_empty() {
            return Err(BuildError::BuilderIncomplete("private_url"));
        }
        if self.trade_url.is_empty() {
            return Err(BuildError::BuilderIncomplete("trade_url"));
        }
        if self.rest_url.is_empty() {
            return Err(BuildError::BuilderIncomplete("rest_url"));
        }
        if self.api_key.is_empty() {
            return Err(BuildError::BuilderIncomplete("api_key"));
        }
        if self.secret.is_empty() {
            return Err(BuildError::BuilderIncomplete("secret"));
        }
        if self.category.is_empty() {
            return Err(BuildError::BuilderIncomplete("category"));
        }

        if self.order_prefix.contains("/") {
            panic!("order prefix cannot include '/'.");
        }
        if self.order_prefix.len() > 8 {
            panic!("order prefix length should be not greater than 8.");
        }
        let (order_tx, _) = broadcast::channel(500);
        let (symbol_tx, _) = broadcast::channel(500);
        Ok(Bybit {
            public_url: self.public_url,
            private_url: self.private_url,
            trade_url: self.trade_url,
            api_key: self.api_key.clone(),
            secret: self.secret.clone(),
            order_tx,
            order_manager: Arc::new(Mutex::new(OrderManager::new(&self.order_prefix))),
            category: self.category,
            client: BybitClient::new(&self.rest_url, &self.api_key, &self.secret),
            instruments: Default::default(),
            symbol_tx,
        })
    }
}

type SharedInstrumentMap = Arc<Mutex<HashMap<String, Instrument>>>;

pub struct Bybit {
    public_url: String,
    private_url: String,
    trade_url: String,
    api_key: String,
    secret: String,
    order_tx: Sender<OrderOp>,
    order_manager: SharedOrderManager,
    category: String,
    instruments: SharedInstrumentMap,
    client: BybitClient,
    symbol_tx: Sender<String>,
}

impl Bybit {
    pub fn builder() -> BybitBuilder {
        BybitBuilder {
            public_url: "".to_string(),
            private_url: "".to_string(),
            trade_url: "".to_string(),
            rest_url: "".to_string(),
            topics: Default::default(),
            api_key: "".to_string(),
            secret: "".to_string(),
            category: "".to_string(),
            order_prefix: "".to_string(),
        }
    }

    fn connect_public_stream(&self, ev_tx: UnboundedSender<LiveEvent>) {
        // Connects to the public stream for the market data.
        let public_url = self.public_url.clone();
        let symbol_tx = self.symbol_tx.clone();
        // let mut topics = vec!["orderbook.50".to_string(), "publicTrade".to_string()];
        // for topic in self.topics.iter() {
        //     topics.push(topic.clone());
        // }

        tokio::spawn(async move {
            let _ = retry(|| async {
                let mut stream = PublicStream::new(ev_tx.clone(), symbol_tx.subscribe());
                if let Err(error) = stream.connect(&public_url).await {
                    error!(?error, "A connection error occurred.");
                    ev_tx
                        .send(LiveEvent::Error(LiveError::with(
                            ErrorKind::ConnectionInterrupted,
                            error.to_value(),
                        )))
                        .unwrap();
                } else {
                    ev_tx
                        .send(LiveEvent::Error(LiveError::new(
                            ErrorKind::ConnectionInterrupted,
                        )))
                        .unwrap();
                }
                Err::<(), BybitError>(BybitError::ConnectionInterrupted)
            })
            .await;
        });
    }

    fn connect_private_stream(&self, ev_tx: UnboundedSender<LiveEvent>) {
        // Connects to the private stream for the position and order data.
        let private_url = self.private_url.clone();
        let api_key = self.api_key.clone();
        let secret = self.secret.clone();
        let category = self.category.clone();
        let order_manager = self.order_manager.clone();
        let instruments = self.instruments.clone();
        let client = self.client.clone();
        tokio::spawn(async move {
            let _ = retry(|| async {
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

                if let Err(error) = stream.connect(&private_url).await {
                    error!(?error, "A connection error occurred.");
                    ev_tx
                        .send(LiveEvent::Error(LiveError::with(
                            ErrorKind::ConnectionInterrupted,
                            error.to_value(),
                        )))
                        .unwrap();
                } else {
                    ev_tx
                        .send(LiveEvent::Error(LiveError::new(
                            ErrorKind::ConnectionInterrupted,
                        )))
                        .unwrap();
                }
                Err::<(), BybitError>(BybitError::ConnectionInterrupted)
            })
            .await;
        });
    }

    fn connect_trade_stream(&self, ev_tx: UnboundedSender<LiveEvent>) {
        let trade_url = self.trade_url.clone();
        let api_key = self.api_key.clone();
        let secret = self.secret.clone();
        let order_manager = self.order_manager.clone();
        let order_tx = self.order_tx.clone();
        tokio::spawn(async move {
            let _ = retry(|| async {
                let mut stream = trade_stream::TradeStream::new(
                    api_key.clone(),
                    secret.clone(),
                    ev_tx.clone(),
                    order_manager.clone(),
                    order_tx.subscribe(),
                );
                if let Err(error) = stream.connect(&trade_url).await {
                    error!(?error, "A connection error occurred.");
                    ev_tx
                        .send(LiveEvent::Error(LiveError::with(
                            ErrorKind::ConnectionInterrupted,
                            error.to_value(),
                        )))
                        .unwrap();
                } else {
                    ev_tx
                        .send(LiveEvent::Error(LiveError::new(
                            ErrorKind::ConnectionInterrupted,
                        )))
                        .unwrap();
                }
                Err::<(), BybitError>(BybitError::ConnectionInterrupted)
            })
            .await;
        });
    }
}

impl Connector for Bybit {
    fn add(&mut self, symbol: String, tick_size: f64, _ev_tx: UnboundedSender<LiveEvent>) {
        let instrument = Instrument {
            symbol: symbol.clone(),
            tick_size,
        };
        let mut instruments = self.instruments.lock().unwrap();
        instruments.insert(symbol.clone(), instrument.clone());
        self.symbol_tx.send(symbol).unwrap();
    }

    fn run(&mut self, ev_tx: UnboundedSender<LiveEvent>) {
        self.connect_public_stream(ev_tx.clone());
        self.connect_private_stream(ev_tx.clone());
        self.connect_trade_stream(ev_tx);
    }

    fn submit(&self, asset: String, order: Order, _ev_tx: UnboundedSender<LiveEvent>) {
        let mut order_manager = self.order_manager.lock().unwrap();
        let bybit_order = order_manager
            .new_order(&asset, &self.category, order)
            .unwrap();
        self.order_tx
            .send(OrderOp {
                op: "order.create",
                bybit_order,
            })
            .unwrap();
    }

    fn cancel(&self, asset: String, order: Order, _ev_tx: UnboundedSender<LiveEvent>) {
        let mut order_manager = self.order_manager.lock().unwrap();
        let bybit_order = order_manager
            .cancel_order(&asset, &self.category, order.order_id)
            .unwrap();
        self.order_tx
            .send(OrderOp {
                op: "order.cancel",
                bybit_order,
            })
            .unwrap();
    }
}
