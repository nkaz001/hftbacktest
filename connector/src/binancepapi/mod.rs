mod market_data_stream;
pub mod msg;
mod ordermanager;
pub mod rest;
mod user_data_stream;

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
use tracing::{debug, error, info, warn};

use crate::{
    binancepapi::{
        ordermanager::{OrderManager, SharedOrderManager},
        rest::BinancePapiClient,
    },
    connector::{Connector, ConnectorBuilder, GetOrders, PublishEvent},
    utils::{ExponentialBackoff, Retry},
};

#[derive(Error, Debug)]
pub enum BinancePapiError {
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

impl From<BinancePapiError> for Value {
    fn from(value: BinancePapiError) -> Value {
        match value {
            BinancePapiError::InstrumentNotFound => Value::String(value.to_string()),
            BinancePapiError::InvalidRequest => Value::String(value.to_string()),
            BinancePapiError::ReqError(error) => {
                let mut map = HashMap::new();
                if let Some(code) = error.status() {
                    map.insert("status_code".to_string(), Value::String(code.to_string()));
                }
                map.insert("msg".to_string(), Value::String(error.to_string()));
                Value::Map(map)
            }
            BinancePapiError::OrderError { code, msg } => Value::Map({
                let mut map = HashMap::new();
                map.insert("code".to_string(), Value::Int(code));
                map.insert("msg".to_string(), Value::String(msg));
                map
            }),
            BinancePapiError::Tunstenite(error) => Value::String(format!("{error}")),
            BinancePapiError::ListenKeyExpired => Value::String(value.to_string()),
            BinancePapiError::ConnectionInterrupted => Value::String(value.to_string()),
            BinancePapiError::ConnectionAbort(_) => Value::String(value.to_string()),
            BinancePapiError::Config(_) => Value::String(value.to_string()),
            BinancePapiError::PrefixUnmatched => Value::String(value.to_string()),
            BinancePapiError::OrderNotFound => Value::String(value.to_string()),
        }
    }
}

#[derive(Deserialize)]
pub struct Config {
    /// REST API URL for PAPI (https://papi.binance.com)
    api_url: String,
    /// WebSocket stream URL for market data (wss://fstream.binance.com/ws)
    stream_url: String,
    /// WebSocket stream URL for user data (wss://fstream.binance.com/pm)
    /// If not specified, defaults to stream_url with /pm path
    #[serde(default)]
    user_stream_url: String,
    #[serde(default)]
    order_prefix: String,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    secret: String,
}

type SharedSymbolSet = Arc<Mutex<HashSet<String>>>;

/// A connector for Binance Portfolio Margin (PAPI).
///
/// PAPI provides unified margin for UM Futures, CM Futures, and Margin trading.
/// This connector supports:
/// - UM Futures order operations (submit, modify, cancel)
/// - Portfolio Margin account information
/// - User data stream for order and position updates
/// - Market data stream for depth and trade updates
pub struct BinancePapi {
    config: Config,
    symbols: SharedSymbolSet,
    order_manager: SharedOrderManager,
    client: BinancePapiClient,
    symbol_tx: Sender<String>,
}

impl BinancePapi {
    pub fn connect_market_data_stream(&mut self, ev_tx: UnboundedSender<PublishEvent>) {
        let base_url = self.config.stream_url.clone();
        let client = self.client.clone();
        let symbol_tx = self.symbol_tx.clone();

        tokio::spawn(async move {
            let _ = Retry::new(ExponentialBackoff::default())
                .error_handler(|error: BinancePapiError| {
                    error!(
                        ?error,
                        "An error occurred in the market data stream connection."
                    );
                    ev_tx
                        .send(PublishEvent::LiveEvent(LiveEvent::Error(LiveError::with(
                            ErrorKind::ConnectionInterrupted,
                            error.into(),
                        ))))
                        .unwrap();
                    Ok(())
                })
                .retry(|| async {
                    let mut stream = market_data_stream::MarketDataStream::new(
                        client.clone(),
                        ev_tx.clone(),
                        symbol_tx.subscribe(),
                    );
                    debug!("Connecting to the market data stream...");
                    stream.connect(&base_url).await?;
                    debug!("The market data stream connection is permanently closed.");
                    Ok(())
                })
                .await;
        });
    }

    pub fn connect_user_data_stream(&self, ev_tx: UnboundedSender<PublishEvent>) {
        // User data stream URL: wss://fstream.binance.com/pm/ws/{listenKey}
        let user_stream_base = if self.config.user_stream_url.is_empty() {
            // Default to stream_url with /pm path
            self.config
                .stream_url
                .replace("/ws", "/pm/ws")
                .trim_end_matches('/')
                .to_string()
        } else {
            self.config.user_stream_url.trim_end_matches('/').to_string()
        };

        let client = self.client.clone();
        let order_manager = self.order_manager.clone();
        let instruments = self.symbols.clone();
        let symbol_tx = self.symbol_tx.clone();

        tokio::spawn(async move {
            let _ = Retry::new(ExponentialBackoff::default())
                .error_handler(|error: BinancePapiError| {
                    error!(
                        ?error,
                        "An error occurred in the user data stream connection."
                    );
                    ev_tx
                        .send(PublishEvent::LiveEvent(LiveEvent::Error(LiveError::with(
                            ErrorKind::ConnectionInterrupted,
                            error.into(),
                        ))))
                        .unwrap();
                    Ok(())
                })
                .retry(|| async {
                    let mut stream = user_data_stream::UserDataStream::new(
                        client.clone(),
                        ev_tx.clone(),
                        order_manager.clone(),
                        instruments.clone(),
                        symbol_tx.subscribe(),
                    );

                    debug!("Requesting the listen key for the PAPI user data stream...");
                    let listen_key = stream.get_listen_key().await?;

                    // PAPI user data stream URL format: wss://fstream.binance.com/pm/ws/{listenKey}
                    let url = format!("{}/{}", user_stream_base, listen_key);
                    debug!("Connecting to the PAPI user data stream: {}", url);
                    stream.connect(&url).await?;
                    debug!("The user data stream connection is permanently closed.");
                    Ok(())
                })
                .await;
        });
    }
}

impl ConnectorBuilder for BinancePapi {
    type Error = BinancePapiError;

    fn build_from(config: &str) -> Result<Self, Self::Error> {
        let config: Config = toml::from_str(config)?;

        let order_manager = Arc::new(Mutex::new(OrderManager::new(&config.order_prefix)));
        let client = BinancePapiClient::new(&config.api_url, &config.api_key, &config.secret);
        let (symbol_tx, _) = broadcast::channel(500);

        Ok(BinancePapi {
            config,
            symbols: Default::default(),
            order_manager,
            client,
            symbol_tx,
        })
    }
}

impl Connector for BinancePapi {
    fn register(&mut self, symbol: String) {
        // Binance symbols must be lowercase to subscribe to the WebSocket stream.
        if symbol.to_lowercase() != symbol {
            error!("Binance PAPI symbol must be lowercase.");
        }
        let symbol = symbol.to_lowercase();
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
        self.connect_market_data_stream(ev_tx.clone());
        // Connects to the user stream only if the API key and secret are provided.
        if !self.config.api_key.is_empty() && !self.config.secret.is_empty() {
            self.connect_user_data_stream(ev_tx.clone());
        }
    }

    fn submit(&self, symbol: String, mut order: Order, tx: UnboundedSender<PublishEvent>) {
        let client = self.client.clone();
        let order_manager = self.order_manager.clone();

        tokio::spawn(async move {
            let client_order_id = order_manager
                .lock()
                .unwrap()
                .prepare_client_order_id(symbol.clone(), order.clone());

            match client_order_id {
                Some(client_order_id) => {
                    let result = client
                        .submit_um_order(
                            &client_order_id,
                            &symbol,
                            order.side,
                            order.price_tick as f64 * order.tick_size,
                            get_precision(order.tick_size),
                            order.qty,
                            order.order_type,
                            order.time_in_force,
                            order.reduce_only,
                        )
                        .await;
                    match result {
                        Ok(resp) => {
                            if let Some(order) = order_manager
                                .lock()
                                .unwrap()
                                .update_from_rest(&client_order_id, &resp)
                            {
                                tx.send(PublishEvent::LiveEvent(LiveEvent::Order {
                                    symbol,
                                    order,
                                }))
                                .unwrap();
                            }
                        }
                        Err(error) => {
                            if let Some(order) = order_manager
                                .lock()
                                .unwrap()
                                .update_submit_fail(&client_order_id, &error)
                            {
                                tx.send(PublishEvent::LiveEvent(LiveEvent::Order {
                                    symbol,
                                    order,
                                }))
                                .unwrap();
                            }

                            tx.send(PublishEvent::LiveEvent(LiveEvent::Error(LiveError::with(
                                ErrorKind::OrderError,
                                error.into(),
                            ))))
                            .unwrap();
                        }
                    }
                }
                None => {
                    warn!(
                        ?order,
                        "Coincidentally, creates a duplicated client order id. \
                        This order request will be expired."
                    );
                    order.req = Status::None;
                    order.status = Status::Expired;
                    tx.send(PublishEvent::LiveEvent(LiveEvent::Order { symbol, order }))
                        .unwrap();
                }
            }
        });
    }

    fn cancel(&self, symbol: String, order: Order, tx: UnboundedSender<PublishEvent>) {
        let client = self.client.clone();
        let order_manager = self.order_manager.clone();
        let bot_order_id = order.order_id;

        tokio::spawn(async move {
            let (client_order_id, exchange_order_id) = {
                let mgr = order_manager.lock().unwrap();
                (
                    mgr.get_client_order_id(&symbol, bot_order_id),
                    mgr.get_exchange_order_id(&symbol, bot_order_id),
                )
            };

            match client_order_id {
                Some(client_order_id) => {
                    let result = client.cancel_um_order(&client_order_id, &symbol).await;
                    match result {
                        Ok(resp) => {
                            if let Some(order) = order_manager
                                .lock()
                                .unwrap()
                                .update_from_rest(&client_order_id, &resp)
                            {
                                tx.send(PublishEvent::LiveEvent(LiveEvent::Order {
                                    symbol,
                                    order,
                                }))
                                .unwrap();
                            }
                        }
                        Err(error) => {
                            if let Some(order) = order_manager
                                .lock()
                                .unwrap()
                                .update_cancel_fail(&client_order_id, &error)
                            {
                                tx.send(PublishEvent::LiveEvent(LiveEvent::Order {
                                    symbol,
                                    order,
                                }))
                                .unwrap();
                            }

                            tx.send(PublishEvent::LiveEvent(LiveEvent::Error(LiveError::with(
                                ErrorKind::OrderError,
                                error.into(),
                            ))))
                            .unwrap();
                        }
                    }
                }
                None => {
                    // Fallback: try to cancel using exchange order ID
                    if let Some(exchange_order_id) = exchange_order_id {
                        info!(
                            order_id = bot_order_id,
                            exchange_order_id,
                            "client_order_id not found, using exchange_order_id fallback"
                        );
                        match client
                            .cancel_um_order_by_exchange_id(exchange_order_id, &symbol)
                            .await
                        {
                            Ok(_) => {
                                info!(
                                    order_id = bot_order_id,
                                    exchange_order_id,
                                    "Successfully cancelled order using exchange_order_id"
                                );
                                // Clean up the exchange_order_id mapping
                                order_manager
                                    .lock()
                                    .unwrap()
                                    .remove_exchange_order_id(&symbol, bot_order_id);
                            }
                            Err(error) => {
                                warn!(
                                    order_id = bot_order_id,
                                    exchange_order_id,
                                    ?error,
                                    "Failed to cancel order using exchange_order_id"
                                );
                            }
                        }
                    } else {
                        warn!(
                            order_id = bot_order_id,
                            "Neither client_order_id nor exchange_order_id found; \
                            order may have already been canceled or filled."
                        );
                    }
                }
            }
        });
    }

    fn shutdown(&self, symbols: Vec<String>) -> crate::connector::BoxFuture<'_, ()> {
        Box::pin(async move {
            info!("Shutting down BinancePapi connector, canceling all open orders...");
            for symbol in symbols {
                info!(%symbol, "Canceling all UM orders for symbol");
                match self.client.cancel_all_um_orders(&symbol).await {
                    Ok(()) => {
                        info!(%symbol, "Successfully canceled all UM orders");
                    }
                    Err(error) => {
                        error!(%symbol, ?error, "Failed to cancel all UM orders");
                    }
                }
            }
            info!("BinancePapi connector shutdown complete");
        })
    }
}
