mod msg;
mod ordermanager;
mod rest;
mod ws;

use std::{
    collections::HashMap,
    sync::{mpsc::Sender, Arc, Mutex},
    time::Duration,
};

use thiserror::Error;
use tracing::{debug, error, warn};

use crate::{
    connector::{
        binancefutures::{
            ordermanager::{OrderManager, WrappedOrderManager},
            rest::BinanceFuturesClient,
            ws::connect,
        },
        Connector,
    },
    get_precision,
    live::AssetInfo,
    ty::{Error, ErrorType, LiveEvent, Order, OrderResponse, Position, Status},
};

pub enum Endpoint {
    Public,
    Private,
    Testnet,
    LowLatency,
    Custom(String),
}

#[derive(Error, Debug)]
pub enum BinanceFuturesError {
    #[error("asset not found")]
    AssetNotFound,
}

pub struct BinanceFutures {
    url: String,
    prefix: String,
    assets: HashMap<String, AssetInfo>,
    inv_assets: HashMap<usize, AssetInfo>,
    order_manager: WrappedOrderManager,
    client: BinanceFuturesClient,
}

impl BinanceFutures {
    pub fn new(stream_url: &str, api_url: &str, prefix: &str, api_key: &str, secret: &str) -> Self {
        let order_manager: WrappedOrderManager = Arc::new(Mutex::new(OrderManager::new(prefix)));
        Self {
            url: stream_url.to_string(),
            prefix: prefix.to_string(),
            assets: Default::default(),
            inv_assets: Default::default(),
            order_manager,
            client: BinanceFuturesClient::new(api_url, api_key, secret),
        }
    }
}

impl Connector for BinanceFutures {
    fn add(
        &mut self,
        asset_no: usize,
        symbol: String,
        tick_size: f32,
        lot_size: f32,
    ) -> Result<(), anyhow::Error> {
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

    fn run(&mut self, ev_tx: Sender<LiveEvent>) -> Result<(), anyhow::Error> {
        let assets = self.assets.clone();
        let base_url = self.url.clone();
        let prefix = self.prefix.clone();
        let client = self.client.clone();
        let orders = self.order_manager.clone();
        let mut error_count = 0;

        let _ = tokio::spawn(async move {
            'connection: loop {
                if error_count > 0 {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }

                // Cancel all orders before connecting to the stream in order to start with the
                // clean state.
                for symbol in assets.keys() {
                    if let Err(error) = client.cancel_all_orders(symbol).await {
                        error!(?error, %symbol, "Couldn't cancel all open orders.");
                        ev_tx
                            .send(LiveEvent::Error(Error::with(ErrorType::OrderError, error)))
                            .unwrap();
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
                                    .send(LiveEvent::Position(Position {
                                        asset_no: asset_info.asset_no,
                                        symbol: position.symbol,
                                        qty: position.position_amount,
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

                let listen_key = match client.start_user_data_stream().await {
                    Ok(listen_key) => listen_key,
                    Err(error) => {
                        error!(?error, "Couldn't start user data stream.");
                        // 1000 indicates user data stream starting error.
                        ev_tx
                            .send(LiveEvent::Error(Error::with(
                                ErrorType::Custom(1000),
                                error,
                            )))
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

                if let Err(error) = connect(
                    &url,
                    ev_tx.clone(),
                    assets.clone(),
                    &prefix,
                    orders.clone(),
                    client.clone(),
                )
                .await
                {
                    error!(?error, "A connection error occurred.");
                    ev_tx
                        .send(LiveEvent::Error(Error::with(
                            ErrorType::ConnectionInterrupted,
                            error,
                        )))
                        .unwrap();
                } else {
                    ev_tx
                        .send(LiveEvent::Error(Error::new(
                            ErrorType::ConnectionInterrupted,
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
        mut order: Order<()>,
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
                .prepare_client_order_id(order.clone());

            match client_order_id {
                Some(client_order_id) => {
                    match client
                        .submit_order(
                            &client_order_id,
                            &symbol,
                            order.side,
                            order.price_tick as f32 * order.tick_size,
                            get_precision(order.tick_size),
                            order.qty,
                            order.order_type,
                            order.time_in_force,
                        )
                        .await
                    {
                        Ok(resp) => {
                            let order = orders.lock().unwrap().update_submit_success(order, resp);
                            if let Some(order) = order {
                                tx.send(LiveEvent::Order(OrderResponse { asset_no, order }))
                                    .unwrap();
                            }
                        }
                        Err(error) => {
                            let order = orders.lock().unwrap().update_submit_fail(
                                order,
                                &error,
                                client_order_id,
                            );
                            if let Some(order) = order {
                                tx.send(LiveEvent::Order(OrderResponse { asset_no, order }))
                                    .unwrap();
                            }

                            tx.send(LiveEvent::Error(Error::with(ErrorType::OrderError, error)))
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
                    tx.send(LiveEvent::Order(OrderResponse { asset_no, order }))
                        .unwrap();
                }
            }
        });
        Ok(())
    }

    fn cancel(
        &self,
        asset_no: usize,
        mut order: Order<()>,
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
                            let order = orders.lock().unwrap().update_cancel_success(order, resp);
                            if let Some(order) = order {
                                tx.send(LiveEvent::Order(OrderResponse { asset_no, order }))
                                    .unwrap();
                            }
                        }
                        Err(error) => {
                            let order = orders.lock().unwrap().update_cancel_fail(
                                order,
                                &error,
                                client_order_id,
                            );
                            if let Some(order) = order {
                                tx.send(LiveEvent::Order(OrderResponse { asset_no, order }))
                                    .unwrap();
                            }

                            tx.send(LiveEvent::Error(Error::with(ErrorType::OrderError, error)))
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
