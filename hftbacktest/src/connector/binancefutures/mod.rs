mod msg;
mod ordermanager;
mod rest;
mod ws;

use std::{
    collections::{HashMap, HashSet},
    sync::{mpsc::Sender, Arc, Mutex},
    time::Duration,
};

use thiserror::Error;
use tracing::{debug, error, warn};

use crate::{
    connector::{
        binancefutures::{
            ordermanager::{OrderManager, OrderManagerWrapper},
            rest::BinanceFuturesClient,
            ws::connect,
        },
        Connector,
    },
    live::Asset,
    types::{BuildError, ErrorKind, LiveError, LiveEvent, Order, Status, Value},
    utils::get_precision,
};

#[derive(Clone)]
pub enum Endpoint {
    Public,
    Private,
    Testnet,
    LowLatency,
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

#[derive(Error, Debug)]
pub enum BinanceFuturesError {
    #[error("asset not found")]
    AssetNotFound,
    #[error("invalid request")]
    InvalidRequest,
    #[error("http error: {0:?}")]
    ReqError(#[from] reqwest::Error),
    #[error("error({code}) at order_id({msg})")]
    OrderError { code: i64, msg: String },
}

impl Into<Value> for BinanceFuturesError {
    fn into(self) -> Value {
        match self {
            BinanceFuturesError::AssetNotFound => Value::String(self.to_string()),
            BinanceFuturesError::InvalidRequest => Value::String(self.to_string()),
            BinanceFuturesError::ReqError(err) => err.into(),
            BinanceFuturesError::OrderError { code, msg } => Value::Map({
                let mut map = HashMap::new();
                map.insert("code".to_string(), Value::Int(code));
                map.insert("msg".to_string(), Value::String(msg));
                map
            }),
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
    streams: HashSet<String>,
}

impl BinanceFuturesBuilder {
    /// Sets an endpoint to connect.
    pub fn endpoint(self, endpoint: Endpoint) -> Self {
        if let Endpoint::Custom(_) = endpoint {
            panic!("Use `stream_url` and `api_url` to set a custom endpoint instead");
        }
        self.stream_url(endpoint.clone()).api_url(endpoint)
    }

    /// Sets the Websocket streams endpoint url.
    pub fn stream_url<E: Into<Endpoint>>(self, endpoint: E) -> Self {
        match endpoint.into() {
            Endpoint::Public => {
                Self {
                    // wss://ws-fapi.binance.com/ws-fapi/v1
                    stream_url: "wss://fstream.binance.com".to_string(),
                    ..self
                }
            }
            Endpoint::Private => Self {
                stream_url: "wss://fstream-auth.binance.com".to_string(),
                ..self
            },
            Endpoint::Testnet => Self {
                stream_url: "wss://fstream.binancefuture.com".to_string(),
                ..self
            },
            Endpoint::LowLatency => Self {
                stream_url: "wss://fstream-mm.binance.com".to_string(),
                ..self
            },
            Endpoint::Custom(stream_url) => Self { stream_url, ..self },
        }
    }

    /// Sets the REST APIs endpoint url.
    pub fn api_url<E: Into<Endpoint>>(self, endpoint: E) -> Self {
        match endpoint.into() {
            Endpoint::Public => Self {
                api_url: "https://fapi.binance.com".to_string(),
                ..self
            },
            Endpoint::Private => Self {
                api_url: "https://fapi.binance.com".to_string(),
                ..self
            },
            Endpoint::Testnet => Self {
                api_url: "https://testnet.binancefuture.com".to_string(),
                ..self
            },
            Endpoint::LowLatency => Self {
                api_url: "https://fapi-mm.binance.com".to_string(),
                ..self
            },
            Endpoint::Custom(api_url) => Self { api_url, ..self },
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

    /// Adds an additional stream to receive through the WebSocket connection.
    pub fn add_stream(mut self, stream: &str) -> Self {
        self.streams.insert(stream.to_string());
        self
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

        let order_manager: OrderManagerWrapper =
            Arc::new(Mutex::new(OrderManager::new(&self.order_prefix)));
        Ok(BinanceFutures {
            url: self.stream_url.to_string(),
            prefix: self.order_prefix,
            assets: Default::default(),
            inv_assets: Default::default(),
            order_manager,
            client: BinanceFuturesClient::new(&self.api_url, &self.api_key, &self.secret),
            streams: self.streams,
        })
    }
}

/// A connector for Binance USD-m Futures.
pub struct BinanceFutures {
    url: String,
    prefix: String,
    assets: HashMap<String, Asset>,
    inv_assets: HashMap<usize, Asset>,
    order_manager: OrderManagerWrapper,
    client: BinanceFuturesClient,
    streams: HashSet<String>,
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
            streams: Default::default(),
        }
    }

    /// Constructs an instance of `BinanceFutures`.
    pub fn new(stream_url: &str, api_url: &str, prefix: &str, api_key: &str, secret: &str) -> Self {
        let order_manager: OrderManagerWrapper = Arc::new(Mutex::new(OrderManager::new(prefix)));
        Self {
            url: stream_url.to_string(),
            prefix: prefix.to_string(),
            assets: Default::default(),
            inv_assets: Default::default(),
            order_manager,
            client: BinanceFuturesClient::new(api_url, api_key, secret),
            streams: Default::default(),
        }
    }
}

impl Connector for BinanceFutures {
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
        let assets = self.assets.clone();
        let base_url = self.url.clone();
        let prefix = self.prefix.clone();
        let client = self.client.clone();
        let order_manager = self.order_manager.clone();
        let add_streams = self.streams.clone();
        let mut error_count = 0;

        let _ = tokio::spawn(async move {
            'connection: loop {
                if error_count > 0 {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }

                // Cancel all orders before connecting to the stream in order to start with the
                // clean state.
                for (symbol, _) in assets.iter() {
                    if let Err(error) = client.cancel_all_orders(symbol).await {
                        error!(?error, %symbol, "Couldn't cancel all open orders.");
                        ev_tx
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
                    let mut order_manager_ = order_manager.lock().unwrap();
                    let orders = order_manager_.clear_orders();
                    for (asset_no, order) in orders {
                        ev_tx.send(LiveEvent::Order { asset_no, order }).unwrap();
                    }
                }

                // Fetches the initial states such as positions and open orders.
                match client.get_position_information().await {
                    Ok(positions) => {
                        // todo: check if there is no position info when there is no holding
                        //       position. In that case, it needs to send zero-position to the bot.
                        positions.into_iter().for_each(|position| {
                            assets.get(&position.symbol).map(|asset_info| {
                                ev_tx
                                    .send(LiveEvent::Position {
                                        asset_no: asset_info.asset_no,
                                        qty: position.position_amount,
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

                let listen_key = match client.start_user_data_stream().await {
                    Ok(listen_key) => listen_key,
                    Err(error) => {
                        error!(?error, "Couldn't start user data stream.");
                        // 1000 indicates user data stream starting error.
                        ev_tx
                            .send(LiveEvent::Error(LiveError::with(
                                ErrorKind::Custom(1000),
                                error.into(),
                            )))
                            .unwrap();
                        continue 'connection;
                    }
                };

                // Prepares a URL that connects streams
                let mut streams: Vec<String> = assets
                    .keys()
                    .map(|symbol| {
                        format!(
                            "{}@depth@0ms/{}@trade",
                            symbol.to_lowercase(),
                            symbol.to_lowercase()
                        )
                    })
                    .collect();
                streams.append(&mut add_streams.iter().cloned().collect::<Vec<_>>());
                let url = format!(
                    "{}/stream?streams={}/{}",
                    &base_url,
                    listen_key,
                    streams.join("/")
                );

                if let Err(error) = connect(
                    &url,
                    ev_tx.clone(),
                    assets.clone(),
                    &prefix,
                    order_manager.clone(),
                    client.clone(),
                )
                .await
                {
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
                error_count += 1;
            }
        });
        Ok(())
    }

    fn submit(
        &self,
        asset_no: usize,
        mut order: Order,
        tx: Sender<LiveEvent>,
    ) -> Result<(), anyhow::Error> {
        let asset_info = self
            .inv_assets
            .get(&asset_no)
            .ok_or(BinanceFuturesError::AssetNotFound)?;
        let symbol = asset_info.symbol.clone();
        let client = self.client.clone();
        let orders = self.order_manager.clone();
        tokio::spawn(async move {
            let client_order_id = orders
                .lock()
                .unwrap()
                .prepare_client_order_id(asset_no, order.clone());

            match client_order_id {
                Some(client_order_id) => {
                    match client
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
                        .await
                    {
                        Ok(resp) => {
                            let order = orders
                                .lock()
                                .unwrap()
                                .update_submit_success(asset_no, order, resp);
                            if let Some(order) = order {
                                tx.send(LiveEvent::Order { asset_no, order }).unwrap();
                            }
                        }
                        Err(error) => {
                            let order = orders.lock().unwrap().update_submit_fail(
                                asset_no,
                                order,
                                &error,
                                client_order_id,
                            );
                            if let Some(order) = order {
                                tx.send(LiveEvent::Order { asset_no, order }).unwrap();
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
                    tx.send(LiveEvent::Order { asset_no, order }).unwrap();
                }
            }
        });
        Ok(())
    }

    fn cancel(
        &self,
        asset_no: usize,
        mut order: Order,
        tx: Sender<LiveEvent>,
    ) -> Result<(), anyhow::Error> {
        let asset_info = self
            .inv_assets
            .get(&asset_no)
            .ok_or(BinanceFuturesError::AssetNotFound)?;
        let symbol = asset_info.symbol.clone();
        let client = self.client.clone();
        let orders = self.order_manager.clone();
        tokio::spawn(async move {
            let client_order_id = orders.lock().unwrap().get_client_order_id(order.order_id);

            match client_order_id {
                Some(client_order_id) => {
                    match client.cancel_order(&client_order_id, &symbol).await {
                        Ok(resp) => {
                            let order = orders
                                .lock()
                                .unwrap()
                                .update_cancel_success(asset_no, order, resp);
                            if let Some(order) = order {
                                tx.send(LiveEvent::Order { asset_no, order }).unwrap();
                            }
                        }
                        Err(error) => {
                            let order = orders.lock().unwrap().update_cancel_fail(
                                asset_no,
                                order,
                                &error,
                                client_order_id,
                            );
                            if let Some(order) = order {
                                tx.send(LiveEvent::Order { asset_no, order }).unwrap();
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
                    // order.req = Status::None;
                    // order.status = Status::Expired;
                    // tx.send(Event::Order(OrderResponse { asset_no, order }))
                    //     .unwrap();
                }
            }
        });
        Ok(())
    }
}
