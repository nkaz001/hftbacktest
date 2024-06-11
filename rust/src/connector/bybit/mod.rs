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
            ordermanager::{OrderManager, WrappedOrderManager},
            rest::BybitClient,
            ws::{connect_private, connect_public, connect_trade, OrderOp},
        },
        Connector,
    },
    live::Asset,
    prelude::OrderResponse,
    types::{Error, ErrorKind, LiveEvent, Order, Position},
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

#[derive(Error, Debug)]
pub enum BybitError {
    #[error("asset not found")]
    AssetNotFound,
    #[error("auth error {0}: {1}")]
    AuthError(i64, String),
    #[error("order error {0}: {1}")]
    OrderError(i64, String),
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
    order_man: WrappedOrderManager,
    category: String,
    client: BybitClient,
}

impl Bybit {
    /// Currently, only `linear` category is supported.
    pub fn new(
        public_url: &str,
        private_url: &str,
        trade_url: &str,
        rest_url: &str,
        api_key: &str,
        secret: &str,
        prefix: &str,
        category: &str,
    ) -> Self {
        Self {
            public_url: public_url.to_string(),
            private_url: private_url.to_string(),
            trade_url: trade_url.to_string(),
            assets: Default::default(),
            inv_assets: Default::default(),
            topics: Default::default(),
            api_key: api_key.to_string(),
            secret: secret.to_string(),
            order_tx: None,
            order_man: Arc::new(Mutex::new(OrderManager::new(prefix))),
            category: category.to_string(),
            client: BybitClient::new(rest_url, api_key, secret),
        }
    }
}

impl Connector for Bybit {
    fn add(
        &mut self,
        asset_no: usize,
        symbol: String,
        tick_size: f32,
        lot_size: f32,
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
        // todo: for the finest and most frequent updates, fusing multiple depth topics is needed.
        let mut topics = vec![
            // "orderbook.1".to_string(),
            "orderbook.50".to_string(),
            // "orderbook.500".to_string(),
            "publicTrade".to_string(),
        ];
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
                        .send(LiveEvent::Error(Error::with(
                            ErrorKind::ConnectionInterrupted,
                            error,
                        )))
                        .unwrap();
                } else {
                    ev_tx_public
                        .send(LiveEvent::Error(Error::new(
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
                if let Err(error) = client_private.cancel_all_orders().await {
                    error!(?error, "Couldn't cancel all open orders.");
                    ev_tx_private
                        .send(LiveEvent::Error(Error::with(ErrorKind::OrderError, error)))
                        .unwrap();
                    error_count += 1;
                    continue 'connection;
                }
                {
                    let mut order_manager_ = order_man_private.lock().unwrap();
                    let orders = order_manager_.clear_orders();
                    for (asset_no, order) in orders {
                        ev_tx_private
                            .send(LiveEvent::Order(OrderResponse { asset_no, order }))
                            .unwrap();
                    }
                }

                // Fetches the initial states such as positions and open orders.
                match client_private.get_position_information().await {
                    Ok(positions) => {
                        positions.into_iter().for_each(|position| {
                            assets_private.get(&position.symbol).map(|asset_info| {
                                ev_tx_private
                                    .send(LiveEvent::Position(Position {
                                        asset_no: asset_info.asset_no,
                                        symbol: position.symbol,
                                        qty: position.size,
                                    }))
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
                        .send(LiveEvent::Error(Error::with(
                            ErrorKind::ConnectionInterrupted,
                            error,
                        )))
                        .unwrap();
                } else {
                    ev_tx_private
                        .send(LiveEvent::Error(Error::new(
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
                        .send(LiveEvent::Error(Error::with(
                            ErrorKind::ConnectionInterrupted,
                            error,
                        )))
                        .unwrap();
                } else {
                    ev_tx_trade
                        .send(LiveEvent::Error(Error::new(
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
        order: Order<()>,
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
        order: Order<()>,
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
