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
    types::{BuildError, ErrorKind, LiveError, LiveEvent, Order, Status, Value},
};
use thiserror::Error;
use tokio::sync::{broadcast, broadcast::Sender, mpsc::UnboundedSender};
use tokio_tungstenite::tungstenite;
use tracing::{debug, error, warn};

use crate::{
    binancefutures::{
        ordermanager::{OrderManager, SharedOrderManager},
        rest::BinanceFuturesClient,
    },
    connector::{Connector, Instrument},
    utils::retry,
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
        }
    }
}

/// Binance Futures USD-M connector [`BinanceFutures`] builder.
pub struct BinanceFuturesBuilder {
    stream_url: String,
    api_url: String,
    order_prefix: String,
    api_key: String,
    secret: String,
}

impl BinanceFuturesBuilder {
    /// Sets the Websocket streams endpoint url.
    pub fn stream_url<E>(self, endpoint: E) -> Self
    where
        E: Into<String>,
    {
        Self {
            stream_url: endpoint.into(),
            ..self
        }
    }

    /// Sets the REST APIs endpoint url.
    pub fn api_url<E>(self, endpoint: E) -> Self
    where
        E: Into<String>,
    {
        Self {
            api_url: endpoint.into(),
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

    /// Builds [`BinanceFutures`] connector.
    pub fn build(self) -> Result<BinanceFutures, BuildError> {
        if self.stream_url.is_empty() {
            return Err(BuildError::BuilderIncomplete("stream_url"));
        }
        if self.api_url.is_empty() {
            return Err(BuildError::BuilderIncomplete("api_url"));
        }
        if self.api_key.is_empty() {
            return Err(BuildError::BuilderIncomplete("api_key"));
        }
        if self.secret.is_empty() {
            return Err(BuildError::BuilderIncomplete("secret"));
        }

        let order_manager: SharedOrderManager =
            Arc::new(Mutex::new(OrderManager::new(&self.order_prefix)));

        let (symbol_tx, _) = broadcast::channel(500);

        Ok(BinanceFutures {
            url: self.stream_url.to_string(),
            prefix: self.order_prefix,
            instruments: Default::default(),
            order_manager,
            client: BinanceFuturesClient::new(&self.api_url, &self.api_key, &self.secret),
            symbol_tx,
        })
    }
}

type SharedInstrumentMap = Arc<Mutex<HashMap<String, Instrument>>>;

/// A connector for Binance USD-m Futures.
pub struct BinanceFutures {
    url: String,
    prefix: String,
    instruments: SharedInstrumentMap,
    order_manager: SharedOrderManager,
    client: BinanceFuturesClient,
    symbol_tx: Sender<String>,
}

impl BinanceFutures {
    /// Gets [`BinanceFuturesBuilder`] to build [`BinanceFutures`] connector.
    pub fn builder() -> BinanceFuturesBuilder {
        BinanceFuturesBuilder {
            stream_url: "".to_string(),
            api_url: "".to_string(),
            order_prefix: "".to_string(),
            api_key: "".to_string(),
            secret: "".to_string(),
        }
    }

    pub fn connect_market_data_stream(&mut self, ev_tx: UnboundedSender<LiveEvent>) {
        let base_url = self.url.clone();
        let client = self.client.clone();
        let symbol_tx = self.symbol_tx.clone();

        tokio::spawn(async move {
            let _ = retry(|| async {
                let mut stream = market_data_stream::MarketDataStream::new(
                    client.clone(),
                    ev_tx.clone(),
                    symbol_tx.subscribe(),
                );
                if let Err(error) = stream.connect(&base_url).await {
                    error!(?error, "A connection error occurred.");
                    ev_tx
                        .send(LiveEvent::Error(LiveError::with(
                            ErrorKind::ConnectionInterrupted,
                            error.into(),
                        )))
                        .unwrap();
                } else {
                    ev_tx
                        .send(LiveEvent::Error(LiveError::new(
                            ErrorKind::ConnectionInterrupted,
                        )))
                        .unwrap();
                }
                Err::<(), BinanceFuturesError>(BinanceFuturesError::ConnectionInterrupted)
            })
            .await;
        });
    }

    pub fn connect_user_data_stream(&self, ev_tx: UnboundedSender<LiveEvent>) {
        let base_url = self.url.clone();
        let prefix = self.prefix.clone();
        let client = self.client.clone();
        let order_manager = self.order_manager.clone();
        let instruments = self.instruments.clone();

        tokio::spawn(async move {
            let _ = retry(|| async {
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

                // Prepares a URL that connects streams
                let url = format!("{}/stream?streams={}", &base_url, listen_key,);

                if let Err(error) = stream.connect(&url).await {
                    error!(?error, "A connection error occurred.");
                    ev_tx
                        .send(LiveEvent::Error(LiveError::with(
                            ErrorKind::ConnectionInterrupted,
                            error.into(),
                        )))
                        .unwrap();
                } else {
                    ev_tx
                        .send(LiveEvent::Error(LiveError::new(
                            ErrorKind::ConnectionInterrupted,
                        )))
                        .unwrap();
                }
                Err::<(), BinanceFuturesError>(BinanceFuturesError::InvalidRequest)
            })
            .await;
        });
    }
}

impl Connector for BinanceFutures {
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
        self.connect_market_data_stream(ev_tx.clone());
        self.connect_user_data_stream(ev_tx.clone());
    }

    fn submit(&self, symbol: String, mut order: Order, tx: UnboundedSender<LiveEvent>) {
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
                                tx.send(LiveEvent::Order { symbol, order }).unwrap();
                            }
                        }
                        Err(error) => {
                            if let Some(order) = order_manager.lock().unwrap().update_submit_fail(
                                symbol.clone(),
                                order,
                                &error,
                                client_order_id,
                            ) {
                                tx.send(LiveEvent::Order { symbol, order }).unwrap();
                            }

                            tx.send(LiveEvent::Error(LiveError::with(
                                ErrorKind::OrderError,
                                error.into(),
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
                    tx.send(LiveEvent::Order { symbol, order }).unwrap();
                }
            }
        });
    }

    fn cancel(&self, symbol: String, mut order: Order, tx: UnboundedSender<LiveEvent>) {
        let client = self.client.clone();
        let order_manager = self.order_manager.clone();

        tokio::spawn(async move {
            let client_order_id = order_manager
                .lock()
                .unwrap()
                .get_client_order_id(order.order_id);

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
                                tx.send(LiveEvent::Order { symbol, order }).unwrap();
                            }
                        }
                        Err(error) => {
                            if let Some(order) = order_manager.lock().unwrap().update_cancel_fail(
                                symbol.clone(),
                                order,
                                &error,
                                client_order_id,
                            ) {
                                tx.send(LiveEvent::Order { symbol, order }).unwrap();
                            }

                            tx.send(LiveEvent::Error(LiveError::with(
                                ErrorKind::OrderError,
                                error.into(),
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
