use std::{
    collections::{HashMap, HashSet},
    sync::{mpsc::Sender, Arc, Mutex},
    time::Duration,
};

use thiserror::Error;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tracing::error;

use crate::{
    connector::{
        bybit::{
            ordermanager::{OrderManager, OrderManagerWrapper},
            rest::BybitClient,
            ws::{connect_private, connect_public, connect_trade, OrderOp},
        },
        Connector,
    },
    live::Asset,
    types::{BuildError, ErrorKind, LiveError, LiveEvent, Order, Value},
};

mod msg;
mod ordermanager;
mod rest;
mod ws;

#[derive(Clone)]
pub enum Endpoint {
    Linear,
    Testnet,
    Custom(String),
}

impl From<String> for Endpoint {
    fn from(value: String) -> Self {
        Endpoint::Custom(value)
    }
}

impl From<&'static str> for Endpoint {
    fn from(value: &'static str) -> Self {
        Endpoint::Custom(value.to_string())
    }
}

#[derive(Error, Clone, Debug)]
pub enum BybitError {
    #[error("asset not found")]
    AssetNotFound,
    #[error("auth error {code}: {msg}")]
    AuthError { code: i64, msg: String },
    #[error("order error {code}: {msg}")]
    OrderError { code: i64, msg: String },
}

impl Into<Value> for BybitError {
    fn into(self) -> Value {
        match self {
            BybitError::AssetNotFound => Value::Empty,
            BybitError::AuthError { code, msg } => Value::Map({
                let mut map = HashMap::new();
                map.insert("code".to_string(), Value::Int(code));
                map.insert("msg".to_string(), Value::String(msg));
                map
            }),
            BybitError::OrderError { code, msg } => Value::Map({
                let mut map = HashMap::new();
                map.insert("code".to_string(), Value::Int(code));
                map.insert("msg".to_string(), Value::String(msg));
                map
            }),
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
    /// Sets an endpoint to connect.
    pub fn endpoint(self, endpoint: Endpoint) -> Self {
        if let Endpoint::Custom(_) = endpoint {
            panic!(
                "Use `public_url`, `private_url`, `trade_url`, and `rest_url` to set a custom endpoint instead"
            );
        }
        self.public_url(endpoint.clone())
            .private_url(endpoint.clone())
            .trade_url(endpoint.clone())
            .rest_url(endpoint)
    }

    /// Sets the public Websocket stream endpoint url.
    pub fn public_url<E: Into<Endpoint>>(self, endpoint: E) -> Self {
        match endpoint.into() {
            Endpoint::Linear => Self {
                public_url: "wss://stream.bybit.com/v5/public/linear".to_string(),
                ..self
            },
            Endpoint::Testnet => Self {
                public_url: "wss://stream-testnet.bybit.com/v5/public/linear".to_string(),
                ..self
            },
            Endpoint::Custom(public_url) => Self { public_url, ..self },
        }
    }

    /// Sets the private Websocket stream endpoint url.
    pub fn private_url<E: Into<Endpoint>>(self, endpoint: E) -> Self {
        match endpoint.into() {
            Endpoint::Linear => Self {
                private_url: "wss://stream.bybit.com/v5/private".to_string(),
                ..self
            },
            Endpoint::Testnet => Self {
                private_url: "wss://stream-testnet.bybit.com/v5/private".to_string(),
                ..self
            },
            Endpoint::Custom(private_url) => Self {
                private_url,
                ..self
            },
        }
    }

    /// Sets the trade Websocket stream endpoint url.
    pub fn trade_url<E: Into<Endpoint>>(self, endpoint: E) -> Self {
        match endpoint.into() {
            Endpoint::Linear => Self {
                trade_url: "wss://stream.bybit.com/v5/trade".to_string(),
                ..self
            },
            Endpoint::Testnet => Self {
                trade_url: "wss://stream-testnet.bybit.com/v5/trade".to_string(),
                ..self
            },
            Endpoint::Custom(trade_url) => Self { trade_url, ..self },
        }
    }

    /// Sets the REST API endpoint url.
    pub fn rest_url<E: Into<Endpoint>>(self, endpoint: E) -> Self {
        match endpoint.into() {
            Endpoint::Linear => Self {
                rest_url: "https://api.bybit.com".to_string(),
                ..self
            },
            Endpoint::Testnet => Self {
                rest_url: "https://api-testnet.bybit.com".to_string(),
                ..self
            },
            Endpoint::Custom(rest_url) => Self { rest_url, ..self },
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
    /// [FusedHashMapMarketDepth](crate::depth::FusedHashMapMarketDepth).
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
        Ok(Bybit {
            public_url: self.public_url,
            private_url: self.private_url,
            trade_url: self.trade_url,
            assets: Default::default(),
            inv_assets: Default::default(),
            topics: self.topics,
            api_key: self.api_key.clone(),
            secret: self.secret.clone(),
            order_tx: None,
            order_man: Arc::new(Mutex::new(OrderManager::new(&self.order_prefix))),
            category: self.category,
            client: BybitClient::new(&self.rest_url, &self.api_key, &self.secret),
        })
    }
}

pub struct Bybit {
    public_url: String,
    private_url: String,
    trade_url: String,
    assets: HashMap<String, Asset>,
    inv_assets: HashMap<usize, Asset>,
    topics: HashSet<String>,
    api_key: String,
    secret: String,
    order_tx: Option<UnboundedSender<OrderOp>>,
    order_man: OrderManagerWrapper,
    category: String,
    client: BybitClient,
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
}

impl Connector for Bybit {
    fn add(
        &mut self,
        asset_no: usize,
        symbol: String,
        tick_size: f64,
        lot_size: f64,
    ) -> Result<(), anyhow::Error> {
        let asset_info = Asset {
            asset_no,
            symbol: symbol.clone(),
            tick_size,
            lot_size,
        };
        self.assets.insert(symbol, asset_info.clone());
        self.inv_assets.insert(asset_no, asset_info);
        Ok(())
    }

    fn run(&mut self, ev_tx: Sender<LiveEvent>) -> Result<(), anyhow::Error> {
        // Connects to the public stream for the market data.
        let public_url = self.public_url.clone();
        let ev_tx_public = ev_tx.clone();
        let assets_public = self.assets.clone();
        let mut topics = vec!["orderbook.50".to_string(), "publicTrade".to_string()];
        for topic in self.topics.iter() {
            topics.push(topic.clone());
        }

        let _ = tokio::spawn(async move {
            let mut error_count = 0;
            loop {
                if error_count > 0 {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                if let Err(error) = connect_public(
                    &public_url,
                    ev_tx_public.clone(),
                    assets_public.clone(),
                    topics.clone(),
                )
                .await
                {
                    error!(?error, "A connection error occurred.");
                    ev_tx_public
                        .send(LiveEvent::Error(LiveError::with(
                            ErrorKind::ConnectionInterrupted,
                            error.into(),
                        )))
                        .unwrap();
                } else {
                    ev_tx_public
                        .send(LiveEvent::Error(LiveError::new(
                            ErrorKind::ConnectionInterrupted,
                        )))
                        .unwrap();
                }
                error_count += 1;
            }
        });

        // Connects to the private stream for the position and order data.
        let private_url = self.private_url.clone();
        let ev_tx_private = ev_tx.clone();
        let assets_private = self.assets.clone();
        let api_key_private = self.api_key.clone();
        let secret_private = self.secret.clone();
        let category_private = self.category.clone();
        let order_man_private = self.order_man.clone();
        let client_private = self.client.clone();
        let _ = tokio::spawn(async move {
            let mut error_count = 0;
            'connection: loop {
                if error_count > 0 {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }

                // Cancel all orders before connecting to the stream in order to start with the
                // clean state.
                for (symbol, _) in assets_private.iter() {
                    if let Err(error) = client_private
                        .cancel_all_orders(&category_private, symbol)
                        .await
                    {
                        error!(?error, %symbol, "Couldn't cancel all open orders.");
                        ev_tx_private
                            .send(LiveEvent::Error(LiveError::with(
                                ErrorKind::OrderError,
                                error.into(),
                            )))
                            .unwrap();
                        error_count += 1;
                        continue 'connection;
                    }
                }
                {
                    let mut order_manager_ = order_man_private.lock().unwrap();
                    let orders = order_manager_.clear_orders();
                    for (asset_no, order) in orders {
                        ev_tx_private
                            .send(LiveEvent::Order { asset_no, order })
                            .unwrap();
                    }
                }

                // Fetches the initial states such as positions and open orders.
                for (symbol, _) in assets_private.iter() {
                    match client_private
                        .get_position_information(&category_private, symbol)
                        .await
                    {
                        Ok(positions) => {
                            positions.into_iter().for_each(|position| {
                                assets_private.get(&position.symbol).map(|asset_info| {
                                    ev_tx_private
                                        .send(LiveEvent::Position {
                                            asset_no: asset_info.asset_no,
                                            qty: position.size,
                                        })
                                        .unwrap();
                                });
                            });
                        }
                        Err(error) => {
                            error!(?error, "Couldn't get position information.");
                            error_count += 1;
                            continue 'connection;
                        }
                    }
                }

                if let Err(error) = connect_private(
                    &private_url,
                    &api_key_private,
                    &secret_private,
                    ev_tx_private.clone(),
                    assets_private.clone(),
                    order_man_private.clone(),
                )
                .await
                {
                    error!(?error, "A connection error occurred.");
                    ev_tx_private
                        .send(LiveEvent::Error(LiveError::with(
                            ErrorKind::ConnectionInterrupted,
                            error.into(),
                        )))
                        .unwrap();
                } else {
                    ev_tx_private
                        .send(LiveEvent::Error(LiveError::new(
                            ErrorKind::ConnectionInterrupted,
                        )))
                        .unwrap();
                }
                error_count += 1;
            }
        });

        // Connects to the trade stream for order entry.
        let trade_url = self.trade_url.clone();
        let ev_tx_trade = ev_tx.clone();
        let api_key_trade = self.api_key.clone();
        let secret_trade = self.secret.clone();
        let order_man_trade = self.order_man.clone();
        let (order_tx, mut order_rx) = unbounded_channel();
        self.order_tx = Some(order_tx);
        let _ = tokio::spawn(async move {
            let mut error_count = 0;
            loop {
                if error_count > 0 {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                if let Err(error) = connect_trade(
                    &trade_url,
                    &api_key_trade,
                    &secret_trade,
                    ev_tx_trade.clone(),
                    &mut order_rx,
                    order_man_trade.clone(),
                )
                .await
                {
                    error!(?error, "A connection error occurred.");
                    ev_tx_trade
                        .send(LiveEvent::Error(LiveError::with(
                            ErrorKind::ConnectionInterrupted,
                            error.into(),
                        )))
                        .unwrap();
                } else {
                    ev_tx_trade
                        .send(LiveEvent::Error(LiveError::new(
                            ErrorKind::ConnectionInterrupted,
                        )))
                        .unwrap();
                }
                error_count += 1;
            }
        });

        Ok(())
    }

    fn submit(
        &self,
        asset_no: usize,
        order: Order,
        tx: Sender<LiveEvent>,
    ) -> Result<(), anyhow::Error> {
        let asset_info = self
            .inv_assets
            .get(&asset_no)
            .ok_or(BybitError::AssetNotFound)?;
        let mut order_man = self.order_man.lock().unwrap();
        let bybit_order =
            order_man.new_order(&asset_info.symbol, &self.category, asset_no, order)?;
        self.order_tx.as_ref().unwrap().send(OrderOp {
            op: "order.create".to_string(),
            bybit_order,
            tx,
        })?;
        Ok(())
    }

    fn cancel(
        &self,
        asset_no: usize,
        order: Order,
        tx: Sender<LiveEvent>,
    ) -> Result<(), anyhow::Error> {
        let asset_info = self
            .inv_assets
            .get(&asset_no)
            .ok_or(BybitError::AssetNotFound)?;
        let mut order_man = self.order_man.lock().unwrap();
        let bybit_order =
            order_man.cancel_order(&asset_info.symbol, &self.category, order.order_id)?;
        self.order_tx.as_ref().unwrap().send(OrderOp {
            op: "order.cancel".to_string(),
            bybit_order,
            tx,
        })?;
        Ok(())
    }
}
