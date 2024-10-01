mod market_data_stream;
mod msg;
mod ordermanager;
mod rest;
mod user_data_stream;

use std::{
    collections::HashMap,
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
    binancefutures::{
        ordermanager::{OrderManager, SharedOrderManager},
        rest::BinanceFuturesClient,
    },
    connector::{Connector, ConnectorBuilder, Instrument, PublishMessage},
    utils::{ExponentialBackoff, Retry},
};

#[derive(Error, Debug)]
pub enum BinanceFuturesError {
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
    #[error("Tunstenite: {0:?}")]
    Tunstenite(#[from] tungstenite::Error),
    #[error("Config: {0:?}")]
    Config(#[from] toml::de::Error),
}

impl From<BinanceFuturesError> for Value {
    fn from(value: BinanceFuturesError) -> Value {
        match value {
            BinanceFuturesError::InstrumentNotFound => Value::String(value.to_string()),
            BinanceFuturesError::InvalidRequest => Value::String(value.to_string()),
            BinanceFuturesError::ReqError(error) => error.into(),
            BinanceFuturesError::OrderError { code, msg } => Value::Map({
                let mut map = HashMap::new();
                map.insert("code".to_string(), Value::Int(code));
                map.insert("msg".to_string(), Value::String(msg));
                map
            }),
            BinanceFuturesError::Tunstenite(error) => Value::String(format!("{error}")),
            BinanceFuturesError::ListenKeyExpired => Value::String(value.to_string()),
            BinanceFuturesError::ConnectionInterrupted => Value::String(value.to_string()),
            BinanceFuturesError::ConnectionAbort(_) => Value::String(value.to_string()),
            BinanceFuturesError::Config(_) => Value::String(value.to_string()),
        }
    }
}

#[derive(Deserialize)]
pub struct Config {
    stream_url: String,
    api_url: String,
    #[serde(default)]
    order_prefix: String,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    secret: String,
}

type SharedInstrumentMap = Arc<Mutex<HashMap<String, Instrument>>>;

/// A connector for Binance USD-m Futures.
pub struct BinanceFutures {
    config: Config,
    instruments: SharedInstrumentMap,
    order_manager: SharedOrderManager,
    client: BinanceFuturesClient,
    symbol_tx: Sender<String>,
}

impl BinanceFutures {
    pub fn connect_market_data_stream(&mut self, ev_tx: UnboundedSender<PublishMessage>) {
        let base_url = self.config.stream_url.clone();
        let client = self.client.clone();
        let symbol_tx = self.symbol_tx.clone();

        tokio::spawn(async move {
            let _ = Retry::new(ExponentialBackoff::default())
                .error_handler(|error: BinanceFuturesError| {
                    error!(
                        ?error,
                        "An error occurred in the market data stream connection."
                    );
                    ev_tx
                        .send(PublishMessage::LiveEvent(LiveEvent::Error(
                            LiveError::with(ErrorKind::ConnectionInterrupted, error.into()),
                        )))
                        .unwrap();
                    Ok(())
                })
                .retry(|| async {
                    let mut stream = market_data_stream::MarketDataStream::new(
                        client.clone(),
                        ev_tx.clone(),
                        symbol_tx.subscribe(),
                    );
                    stream.connect(&base_url).await?;
                    Ok(())
                })
                .await;
        });
    }

    pub fn connect_user_data_stream(&self, ev_tx: UnboundedSender<PublishMessage>) {
        let base_url = self.config.stream_url.clone();
        let prefix = self.config.order_prefix.clone();
        let client = self.client.clone();
        let order_manager = self.order_manager.clone();
        let instruments = self.instruments.clone();

        tokio::spawn(async move {
            let _ = Retry::new(ExponentialBackoff::default())
                .error_handler(|error: BinanceFuturesError| {
                    error!(
                        ?error,
                        "An error occurred in the user data stream connection."
                    );
                    ev_tx
                        .send(PublishMessage::LiveEvent(LiveEvent::Error(
                            LiveError::with(ErrorKind::ConnectionInterrupted, error.into()),
                        )))
                        .unwrap();
                    Ok(())
                })
                .retry(|| async {
                    let mut stream = user_data_stream::UserDataStream::new(
                        client.clone(),
                        ev_tx.clone(),
                        order_manager.clone(),
                        instruments.clone(),
                        prefix.clone(),
                    );

                    // Cancel all orders before connecting to the stream in order to start with the
                    // clean state.
                    stream.cancel_all().await?;

                    // Fetches the initial states such as positions and open orders.
                    stream.get_position_information().await?;

                    let listen_key = stream.get_listen_key().await?;

                    stream.connect(&format!("{base_url}/{listen_key}")).await?;
                    Ok(())
                })
                .await;
        });
    }
}

impl ConnectorBuilder for BinanceFutures {
    type Error = BinanceFuturesError;

    fn build_from(config: &str) -> Result<Self, Self::Error> {
        let config: Config = toml::from_str(config)?;

        let order_manager = Arc::new(Mutex::new(OrderManager::new(&config.order_prefix)));
        let client = BinanceFuturesClient::new(&config.api_url, &config.api_key, &config.secret);
        let (symbol_tx, _) = broadcast::channel(500);

        Ok(BinanceFutures {
            config,
            instruments: Default::default(),
            order_manager,
            client,
            symbol_tx,
        })
    }
}

impl Connector for BinanceFutures {
    fn add(
        &mut self,
        symbol: String,
        tick_size: f64,
        id: u64,
        ev_tx: UnboundedSender<PublishMessage>,
    ) {
        let mut instruments = self.instruments.lock().unwrap();
        if instruments.contains_key(&symbol) {
            let order_manager = self.order_manager.lock().unwrap();
            let orders = order_manager.get_orders(&symbol);

            ev_tx
                .send(PublishMessage::LiveEventsWithId {
                    id,
                    events: orders
                        .into_iter()
                        .map(|order| LiveEvent::Order {
                            symbol: symbol.clone(),
                            order,
                        })
                        .collect(),
                })
                .unwrap();
        } else {
            instruments.insert(
                symbol.clone(),
                Instrument {
                    symbol: symbol.clone(),
                    tick_size,
                },
            );
            self.symbol_tx.send(symbol).unwrap();
        }
    }

    fn run(&mut self, ev_tx: UnboundedSender<PublishMessage>) {
        self.connect_market_data_stream(ev_tx.clone());
        // Connects to the user stream only if the API key and secret are provided.
        if !self.config.api_key.is_empty() && !self.config.secret.is_empty() {
            self.connect_user_data_stream(ev_tx.clone());
        }
    }

    fn submit(&self, symbol: String, mut order: Order, tx: UnboundedSender<PublishMessage>) {
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
                        .submit_order(
                            &client_order_id,
                            &symbol,
                            order.side,
                            order.price_tick as f64 * order.tick_size,
                            get_precision(order.tick_size),
                            order.qty,
                            order.order_type,
                            order.time_in_force,
                        )
                        .await;
                    match result {
                        Ok(resp) => {
                            if let Some(order) = order_manager
                                .lock()
                                .unwrap()
                                .update_submit_success(symbol.clone(), order, resp)
                            {
                                tx.send(PublishMessage::LiveEvent(LiveEvent::Order {
                                    symbol,
                                    order,
                                }))
                                .unwrap();
                            }
                        }
                        Err(error) => {
                            if let Some(order) = order_manager.lock().unwrap().update_submit_fail(
                                symbol.clone(),
                                order,
                                &error,
                                client_order_id,
                            ) {
                                tx.send(PublishMessage::LiveEvent(LiveEvent::Order {
                                    symbol,
                                    order,
                                }))
                                .unwrap();
                            }

                            tx.send(PublishMessage::LiveEvent(LiveEvent::Error(
                                LiveError::with(ErrorKind::OrderError, error.into()),
                            )))
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
                    tx.send(PublishMessage::LiveEvent(LiveEvent::Order {
                        symbol,
                        order,
                    }))
                    .unwrap();
                }
            }
        });
    }

    fn cancel(&self, symbol: String, order: Order, tx: UnboundedSender<PublishMessage>) {
        let client = self.client.clone();
        let order_manager = self.order_manager.clone();

        tokio::spawn(async move {
            let client_order_id = order_manager
                .lock()
                .unwrap()
                .get_client_order_id(&symbol, order.order_id);

            match client_order_id {
                Some(client_order_id) => {
                    let result = client.cancel_order(&client_order_id, &symbol).await;
                    match result {
                        Ok(resp) => {
                            if let Some(order) = order_manager
                                .lock()
                                .unwrap()
                                .update_cancel_success(symbol.clone(), order, resp)
                            {
                                tx.send(PublishMessage::LiveEvent(LiveEvent::Order {
                                    symbol,
                                    order,
                                }))
                                .unwrap();
                            }
                        }
                        Err(error) => {
                            if let Some(order) = order_manager.lock().unwrap().update_cancel_fail(
                                symbol.clone(),
                                order,
                                &error,
                                client_order_id,
                            ) {
                                tx.send(PublishMessage::LiveEvent(LiveEvent::Order {
                                    symbol,
                                    order,
                                }))
                                .unwrap();
                            }

                            tx.send(PublishMessage::LiveEvent(LiveEvent::Error(
                                LiveError::with(ErrorKind::OrderError, error.into()),
                            )))
                            .unwrap();
                        }
                    }
                }
                None => {
                    debug!(
                        order_id = order.order_id,
                        "client_order_id corresponding to order_id is not found; \
                        this may be due to the order already being canceled or filled."
                    );
                }
            }
        });
    }
}
