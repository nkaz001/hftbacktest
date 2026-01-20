//! Bybit connector using REST API for order execution
//!
//! This connector uses:
//! - WebSocket for market data (public stream)
//! - WebSocket for order updates and positions (private stream)
//! - REST API for order submission and cancellation
//!
//! This is compatible with Bybit Testnet and Demo Trading environments
//! where WebSocket Trade stream is not available.

use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

use hftbacktest::types::{ErrorKind, LiveError, LiveEvent, Order};
use serde::Deserialize;
use tokio::sync::{broadcast, broadcast::Sender, mpsc::UnboundedSender};
use tracing::{error, info, warn};

use crate::{
    bybit::{
        BybitError,
        ordermanager::{OrderManager, SharedOrderManager},
        private_stream::PrivateStream,
        public_stream::PublicStream,
        rest::BybitClient,
    },
    connector::{Connector, ConnectorBuilder, GetOrders, PublishEvent},
    utils::{ExponentialBackoff, Retry},
};

#[derive(Deserialize)]
pub struct Config {
    pub public_url: String,
    pub private_url: String,
    #[serde(alias = "api_url")]
    pub rest_url: String,
    pub api_key: String,
    pub secret: String,
    pub category: String,
    pub order_prefix: String,
    /// IPC service name for recording (e.g., "hft_recording")
    #[serde(default)]
    pub recording_service: Option<String>,
    /// Connector name for recording events (defaults to "bybit_rest")
    #[serde(default = "default_connector_name")]
    pub connector_name: String,
}

fn default_connector_name() -> String {
    "bybit_rest".to_string()
}

type SharedSymbolSet = Arc<Mutex<HashSet<String>>>;

/// Bybit connector using REST API for order execution
pub struct BybitRest {
    config: Config,
    order_manager: SharedOrderManager,
    symbols: SharedSymbolSet,
    client: BybitClient,
    symbol_tx: Sender<String>,
}

impl BybitRest {
    fn connect_public_stream(&self, ev_tx: UnboundedSender<PublishEvent>) {
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
        let private_url = self.config.private_url.clone();
        let api_key = self.config.api_key.clone();
        let secret = self.config.secret.clone();
        let category = self.config.category.clone();
        let order_manager = self.order_manager.clone();
        let instruments = self.symbols.clone();
        let client = self.client.clone();
        let symbol_tx = self.symbol_tx.clone();
        let recording_service = self.config.recording_service.clone();
        let connector_name = self.config.connector_name.clone();

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
                    let mut stream = PrivateStream::new(
                        api_key.clone(),
                        secret.clone(),
                        ev_tx.clone(),
                        order_manager.clone(),
                        instruments.clone(),
                        category.clone(),
                        client.clone(),
                        symbol_tx.subscribe(),
                        recording_service.clone(),
                        connector_name.clone(),
                    );

                    stream.connect(&private_url).await?;

                    Ok(())
                })
                .await;
        });
    }
}

impl ConnectorBuilder for BybitRest {
    type Error = BybitError;

    fn build_from(config: &str) -> Result<Self, Self::Error> {
        let config: Config = toml::from_str(config)?;
        if config.order_prefix.contains("/") {
            panic!("order prefix cannot include '/'.");
        }
        if config.order_prefix.len() > 8 {
            panic!("order prefix length should be not greater than 8.");
        }
        let (symbol_tx, _) = broadcast::channel(500);
        let order_manager = Arc::new(Mutex::new(OrderManager::new(&config.order_prefix)));
        let client = BybitClient::new(&config.rest_url, &config.api_key, &config.secret);
        Ok(BybitRest {
            config,
            order_manager,
            client,
            symbols: Default::default(),
            symbol_tx,
        })
    }
}

impl Connector for BybitRest {
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
        self.connect_private_stream(ev_tx);
    }

    fn submit(&self, symbol: String, order: Order, tx: UnboundedSender<PublishEvent>) {
        let client = self.client.clone();
        let order_manager = self.order_manager.clone();
        let category = self.config.category.clone();

        info!(
            %symbol,
            order_id = order.order_id,
            side = ?order.side,
            price_tick = order.price_tick,
            qty = order.qty,
            "Submit order request received from bot"
        );

        tokio::spawn(async move {
            let bybit_order = {
                let mut mgr = order_manager.lock().unwrap();
                match mgr.new_order(&symbol, &category, order.clone()) {
                    Ok(o) => o,
                    Err(error) => {
                        error!(?error, "Failed to create order");
                        tx.send(PublishEvent::LiveEvent(LiveEvent::Error(LiveError::with(
                            ErrorKind::OrderError,
                            error.to_value(),
                        ))))
                        .unwrap();
                        return;
                    }
                }
            };

            let order_link_id = bybit_order.order_link_id.clone();
            info!(
                %symbol,
                %order_link_id,
                order_id = order.order_id,
                side = ?order.side,
                price = ?bybit_order.price,
                qty = ?bybit_order.qty,
                "Submitting order to exchange via REST API"
            );

            match client.submit_order(&bybit_order).await {
                Ok(resp) => {
                    info!(
                        %symbol,
                        %order_link_id,
                        exchange_order_id = %resp.order_id,
                        "Order submitted successfully"
                    );
                    // Order status will be updated via private stream
                }
                Err(error) => {
                    error!(
                        %symbol,
                        %order_link_id,
                        ?error,
                        "Order submission failed"
                    );
                    if let Ok(order_ext) = order_manager
                        .lock()
                        .unwrap()
                        .update_submit_fail(&order_link_id)
                    {
                        tx.send(PublishEvent::LiveEvent(LiveEvent::Order {
                            symbol,
                            order: order_ext.order,
                        }))
                        .unwrap();
                    }

                    tx.send(PublishEvent::LiveEvent(LiveEvent::Error(LiveError::with(
                        ErrorKind::OrderError,
                        error.to_value(),
                    ))))
                    .unwrap();
                }
            }
        });
    }

    fn cancel(&self, symbol: String, order: Order, tx: UnboundedSender<PublishEvent>) {
        let client = self.client.clone();
        let order_manager = self.order_manager.clone();
        let category = self.config.category.clone();
        let bot_order_id = order.order_id;

        tokio::spawn(async move {
            let bybit_order = {
                let mut mgr = order_manager.lock().unwrap();
                match mgr.cancel_order(&symbol, &category, bot_order_id) {
                    Ok(o) => o,
                    Err(error) => {
                        // Order not found - may have already been canceled or filled
                        warn!(
                            order_id = bot_order_id,
                            ?error,
                            "Order not found for cancellation"
                        );
                        return;
                    }
                }
            };

            let order_link_id = bybit_order.order_link_id.clone();
            info!(
                %symbol,
                %order_link_id,
                order_id = bot_order_id,
                "Canceling order via REST API"
            );

            match client
                .cancel_order(&category, &symbol, &order_link_id)
                .await
            {
                Ok(resp) => {
                    info!(
                        %symbol,
                        %order_link_id,
                        exchange_order_id = %resp.order_id,
                        "Order canceled successfully"
                    );
                    // Order status will be updated via private stream
                }
                Err(error) => {
                    error!(
                        %symbol,
                        %order_link_id,
                        ?error,
                        "Order cancellation failed"
                    );
                    if let Ok(order_ext) = order_manager
                        .lock()
                        .unwrap()
                        .update_cancel_fail(&order_link_id)
                    {
                        tx.send(PublishEvent::LiveEvent(LiveEvent::Order {
                            symbol,
                            order: order_ext.order,
                        }))
                        .unwrap();
                    }

                    tx.send(PublishEvent::LiveEvent(LiveEvent::Error(LiveError::with(
                        ErrorKind::OrderError,
                        error.to_value(),
                    ))))
                    .unwrap();
                }
            }
        });
    }

    fn shutdown(&self, symbols: Vec<String>) -> crate::connector::BoxFuture<'_, ()> {
        Box::pin(async move {
            info!("Shutting down BybitRest connector, canceling all open orders...");
            let category = &self.config.category;
            for symbol in symbols {
                info!(%symbol, %category, "Canceling all orders for symbol");
                match self.client.cancel_all_orders(category, &symbol).await {
                    Ok(()) => {
                        info!(%symbol, "Successfully canceled all orders");
                    }
                    Err(error) => {
                        error!(%symbol, ?error, "Failed to cancel all orders");
                    }
                }
            }
            info!("BybitRest connector shutdown complete");
        })
    }
}
