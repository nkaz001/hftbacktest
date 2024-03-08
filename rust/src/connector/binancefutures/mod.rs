mod msg;
mod rest;
mod ws;

use std::{
    collections::HashMap,
    fmt::{Display, Formatter},
    sync::mpsc::Sender,
    time::Duration,
};

use reqwest::StatusCode;
use tracing::error;

use crate::{
    connector::{
        binancefutures::{
            rest::{BinanceFuturesClient, OrderRequestError},
            ws::connect,
        },
        Connector,
    },
    live::AssetInfo,
    ty::{EvError, Event, Order, OrderResponse, Position, Status},
};

fn parse_client_order_id(client_order_id: &str, prefix: &str) -> Option<i64> {
    if !client_order_id.starts_with(prefix) {
        None
    } else {
        let s = &client_order_id[prefix.len()..];
        if let Ok(order_id) = s.parse() {
            Some(order_id)
        } else {
            None
        }
    }
}

pub enum Endpoint {
    Public,
    Private,
    Testnet,
    LowLatency,
    Custom(String),
}

#[derive(Debug)]
pub enum BinanceFuturesError {
    AssetNotFound,
}

impl Display for BinanceFuturesError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for BinanceFuturesError {}

pub struct BinanceFutures {
    url: String,
    prefix: String,
    api_key: String,
    secret: String,
    assets: HashMap<String, AssetInfo>,
    inv_assets: HashMap<usize, AssetInfo>,
    client: BinanceFuturesClient,
}

impl BinanceFutures {
    pub fn new(stream_url: &str, api_url: &str, prefix: &str, api_key: &str, secret: &str) -> Self {
        Self {
            url: stream_url.to_string(),
            prefix: prefix.to_string(),
            api_key: api_key.to_string(),
            secret: secret.to_string(),
            assets: Default::default(),
            inv_assets: Default::default(),
            client: BinanceFuturesClient::new(
                api_url,
                prefix,
                api_key,
                secret,
            ),
        }
    }
}

impl Connector for BinanceFutures {
    fn add(&mut self, asset_no: usize, symbol: String, tick_size: f32, lot_size: f32) -> Result<(), anyhow::Error> {
        let asset_info = AssetInfo {
            asset_no,
            symbol: symbol.clone(),
            tick_size,
            lot_size,
        };
        self.assets.insert(symbol, asset_info.clone());
        self.inv_assets.insert(asset_no, asset_info);
        Ok(())
    }

    fn run(&mut self, ev_tx: Sender<Event>) -> Result<(), anyhow::Error> {
        let assets = self.assets.clone();
        let base_url = self.url.clone();
        let prefix = self.prefix.clone();
        let client = self.client.clone();
        let mut error_count = 0;

        let _ = tokio::spawn(async move {
            'connection: loop {
                if error_count > 0 {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }

                // Cancel all orders before connecting to the stream in order to start with the
                // clean state.
                for symbol in assets.keys() {
                    if let Err(e) = client.cancel_all_orders(symbol).await {
                        error!(error = ?e, symbol = symbol, "Couldn't cancel all open orders.");
                        if e.status().unwrap_or(StatusCode::default()) == StatusCode::UNAUTHORIZED {
                            ev_tx
                                .send(Event::Error(
                                    EvError::CriticalConnectionError as i64,
                                    Some({
                                        let mut var = HashMap::new();
                                        var.insert("reason", e.to_string());
                                        var.insert("status", format!("{:?}", e.status()));
                                        var
                                    }),
                                ))
                                .unwrap();
                        }
                        error_count += 1;
                        continue 'connection;
                    }
                }

                // Fetches the initial states such as positions and open orders.
                match client.get_position_information().await {
                    Ok(positions) => {
                        positions.into_iter().for_each(|position| {
                            assets.get(&position.symbol).map(|asset_info| {
                                ev_tx
                                    .send(Event::Position(Position {
                                        asset_no: asset_info.asset_no,
                                        symbol: position.symbol,
                                        qty: position.position_amount,
                                    }))
                                    .unwrap();
                            });
                        });
                    }
                    Err(e) => {
                        error!(error = ?e, "Couldn't get position information.");
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
                            .send(Event::Error(
                                1000,
                                Some({
                                    let mut var = HashMap::new();
                                    var.insert("reason", error.to_string());
                                    var.insert("status", format!("{:?}", error.status()));
                                    var
                                }),
                            ))
                            .unwrap();
                        continue 'connection;
                    }
                };

                // Prepares a URL that connects streams
                let streams: Vec<String> = assets
                    .keys()
                    .map(|symbol| {
                        format!(
                            "{}@depth@0ms/{}@trade",
                            symbol.to_lowercase(),
                            symbol.to_lowercase()
                        )
                    })
                    .collect();
                let url = format!("{}{}/{}", &base_url, listen_key, streams.join("/"));

                if let Err(error) =
                    connect(&url, ev_tx.clone(), assets.clone(), &prefix, client.clone()).await
                {
                    error!(?error, "A connection error occurred.");
                }
                error_count += 1;
                ev_tx
                    .send(Event::Error(EvError::ConnectionInterrupted as i64, None))
                    .unwrap();
            }
        });
        Ok(())
    }

    fn submit(
        &self,
        asset_no: usize,
        order: Order<()>,
        tx: Sender<Event>,
    ) -> Result<(), anyhow::Error> {
        let asset_info = self
            .inv_assets
            .get(&asset_no)
            .ok_or(BinanceFuturesError::AssetNotFound)?;
        let symbol = asset_info.symbol.clone();
        let client = self.client.clone();
        tokio::spawn(async move {
            match client.submit_order(&symbol, order).await {
                Ok(order) => {
                    tx.send(Event::Order(OrderResponse { asset_no, order }))
                        .unwrap();
                }
                Err(error) => {
                    error!(?error, "Error");
                    let mut order = match error {
                        OrderRequestError::InvalidRequest(order) => order,
                        OrderRequestError::ReqError(_, order) => order,
                        OrderRequestError::OrderError(order, _, _) => order,
                    };
                    order.req = Status::None;
                    order.status = Status::Expired;
                    tx.send(Event::Order(OrderResponse { asset_no, order }))
                        .unwrap();
                    // fixme
                    tx.send(Event::Error(0, None)).unwrap();
                }
            }
        });
        Ok(())
    }

    fn cancel(
        &self,
        asset_no: usize,
        order: Order<()>,
        tx: Sender<Event>,
    ) -> Result<(), anyhow::Error> {
        let asset_info = self
            .inv_assets
            .get(&asset_no)
            .ok_or(BinanceFuturesError::AssetNotFound)?;
        let symbol = asset_info.symbol.clone();
        let client = self.client.clone();
        tokio::spawn(async move {
            match client.cancel_order(&symbol, order).await {
                Ok(order) => {
                    tx.send(Event::Order(OrderResponse { asset_no, order }))
                        .unwrap();
                }
                Err(error) => {
                    error!(?error, "Error");
                    let mut order = match error {
                        OrderRequestError::InvalidRequest(order) => order,
                        OrderRequestError::ReqError(_, order) => order,
                        OrderRequestError::OrderError(order, _, _) => order,
                    };
                    order.req = Status::None;
                    tx.send(Event::Order(OrderResponse { asset_no, order }))
                        .unwrap();
                    // fixme
                    tx.send(Event::Error(0, None)).unwrap();
                }
            }
        });
        Ok(())
    }
}
