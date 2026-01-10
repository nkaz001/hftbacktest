use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

use futures_util::{SinkExt, StreamExt};
use hftbacktest::prelude::*;
use tokio::{
    select,
    sync::{
        broadcast::{Receiver, error::RecvError},
        mpsc::UnboundedSender,
    },
    time,
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use tracing::{debug, error, info, warn};

use crate::{
    binancefutures::{
        BinanceFuturesError,
        SharedSymbolSet,
        msg::stream::{EventStream, OrderTradeUpdate, Stream},
        ordermanager::SharedOrderManager,
        rest::BinanceFuturesClient,
    },
    connector::PublishEvent,
};

#[cfg(feature = "recording")]
use order_recorder::{
    events::{IncomeEvent, RecordingEvent},
    service::RecordingPublisher,
};

pub struct UserDataStream {
    symbols: SharedSymbolSet,
    client: BinanceFuturesClient,
    ev_tx: UnboundedSender<PublishEvent>,
    order_manager: SharedOrderManager,
    symbol_rx: Receiver<String>,
    connector_name: String,
    #[cfg(feature = "recording")]
    recording_service: Option<String>,
}

impl UserDataStream {
    pub fn new(
        client: BinanceFuturesClient,
        ev_tx: UnboundedSender<PublishEvent>,
        order_manager: SharedOrderManager,
        symbols: SharedSymbolSet,
        symbol_rx: Receiver<String>,
        connector_name: String,
        #[cfg(feature = "recording")] recording_service: Option<&str>,
    ) -> Self {
        Self {
            symbols,
            client,
            ev_tx,
            order_manager,
            symbol_rx,
            connector_name,
            #[cfg(feature = "recording")]
            recording_service: recording_service.map(|s| s.to_string()),
        }
    }

    pub async fn get_listen_key(&self) -> Result<String, BinanceFuturesError> {
        Ok(self.client.start_user_data_stream().await?)
    }

    /// Publish realized PnL as income event when a trade has non-zero realized profit
    #[cfg(feature = "recording")]
    fn publish_realized_pnl(
        data: &OrderTradeUpdate,
        connector_name: &str,
        publisher: &RecordingPublisher,
    ) {
        if data.order.realized_profit.abs() < 1e-10 {
            return; // No realized profit
        }

        let commission = data.order.commission
            .as_ref()
            .and_then(|c| c.parse::<f64>().ok())
            .unwrap_or(0.0);

        let income_event = IncomeEvent {
            timestamp: data.transaction_time * 1_000_000, // Convert ms to ns
            connector: connector_name.to_string(),
            strategy_id: String::new(), // Will be filled by order-recorder if needed
            symbol: data.order.symbol.clone(),
            income_type: "REALIZED_PNL".to_string(),
            income: data.order.realized_profit,
            asset: data.order.commission_asset.clone().unwrap_or_else(|| "USDT".to_string()),
            info: format!("trade_id={} commission={}", data.order.trade_id, commission),
            tran_id: data.order.trade_id,
            trade_id: data.order.trade_id.to_string(),
        };

        info!(
            "Publishing realized PnL: {} {} = {:.4} {}",
            income_event.symbol, income_event.income_type, income_event.income, income_event.asset
        );

        if let Err(e) = publisher.publish(&RecordingEvent::Income(income_event)) {
            error!("Failed to publish realized PnL: {:?}", e);
        }
    }

    fn process_message(
        &self,
        stream: EventStream,
        #[cfg(feature = "recording")] income_publisher: Option<&RecordingPublisher>,
    ) -> Result<(), BinanceFuturesError> {
        match stream {
            EventStream::DepthUpdate(_) | EventStream::Trade(_) => unreachable!(),
            EventStream::ListenKeyExpired(_) => {
                return Err(BinanceFuturesError::ListenKeyExpired);
            }
            EventStream::AccountUpdate(data) => {
                for position in data.account.position {
                    // Ignore send errors during shutdown
                    let _ = self.ev_tx
                        .send(PublishEvent::LiveEvent(LiveEvent::Position {
                            symbol: position.symbol,
                            qty: position.position_amount,
                            exch_ts: data.transaction_time * 1_000_000,
                        }));
                }
            }
            EventStream::OrderTradeUpdate(data) => {
                // Publish realized PnL if available
                #[cfg(feature = "recording")]
                if let Some(publisher) = income_publisher {
                    Self::publish_realized_pnl(&data, &self.connector_name, publisher);
                }

                match self.order_manager.lock().unwrap().update_from_ws(&data) {
                    Ok(Some(order)) => {
                        // Ignore send errors during shutdown
                        let _ = self.ev_tx
                            .send(PublishEvent::LiveEvent(LiveEvent::Order {
                                symbol: data.order.symbol,
                                order,
                            }));
                    }
                    Ok(None) => {
                        // This order is already deleted.
                    }
                    Err(BinanceFuturesError::PrefixUnmatched) => {
                        // This order is not created by this connector.
                    }
                    Err(BinanceFuturesError::OrderNotFound) => {
                        // Order not in tracking map - likely from previous run, cancelled on startup.
                        // This is expected after connector restart, silently ignore.
                    }
                    Err(error) => {
                        error!(
                            ?error,
                            ?data,
                            "Couldn't update the order from OrderTradeUpdate message."
                        );
                    }
                }
            }
            EventStream::TradeLite(_data) => {
                // Since this message does not include the order status, additional logic is
                // required to fully utilize it. To reduce latency— which first needs to be
                // measured—a new logic must be implemented to reconstruct the order status and open
                // position by using the last filled quantity and reconciling it with data from the
                // ORDER_TRADE_UPDATE message.
            }
        }
        Ok(())
    }

    pub async fn connect(&mut self, url: &str) -> Result<(), BinanceFuturesError> {
        let request = url.into_client_request()?;
        let (ws_stream, _) = connect_async(request).await?;
        let (mut write, mut read) = ws_stream.split();
        let mut interval = time::interval(Duration::from_secs(60 * 30));
        let mut ping_checker = time::interval(Duration::from_secs(10));

        // Create income publisher inside connect (not in struct) because RecordingPublisher is not Send
        #[cfg(feature = "recording")]
        let income_publisher = self.recording_service.as_ref().and_then(|service| {
            match RecordingPublisher::new(service) {
                Ok(publisher) => {
                    info!("Income publisher created for connector {}", self.connector_name);
                    Some(publisher)
                }
                Err(e) => {
                    error!("Failed to create income publisher: {:?}", e);
                    None
                }
            }
        });

        let symbols: HashSet<_> = self.symbols.lock().unwrap().iter().cloned().collect();
        let client = self.client.clone();
        let order_manager = self.order_manager.clone();
        let ev_tx = self.ev_tx.clone();
        let mut last_ping = Instant::now();

        tokio::spawn(async move {
            // Cancel all orders before connecting to the stream in order to start with the
            // clean state.
            for symbol in &symbols {
                if let Err(error) = cancel_all(
                    client.clone(),
                    symbol.clone(),
                    order_manager.clone(),
                    ev_tx.clone(),
                )
                .await
                {
                    error!(?error, %symbol, "Couldn't cancel all orders.");
                }
            }

            // Fetches the initial states such as positions and open orders.
            if let Err(error) =
                get_position_information(client.clone(), symbols, ev_tx.clone()).await
            {
                error!(?error, "Couldn't get position information.");
            }
        });

        loop {
            select! {
                _ = interval.tick() => {
                    self.order_manager
                        .lock()
                        .unwrap()
                        .gc();
                    let client_ = self.client.clone();
                    tokio::spawn(async move {
                        if let Err(error) = client_.keepalive_user_data_stream().await {
                            error!(?error, "Failed keepalive user data stream.");
                            // todo: reset the connection.
                        }
                    });
                }
                _ = ping_checker.tick() => {
                    if last_ping.elapsed() > Duration::from_secs(300) {
                        warn!("Ping timeout.");
                        return Err(BinanceFuturesError::ConnectionInterrupted);
                    }
                }
                msg = self.symbol_rx.recv() => {
                    match msg {
                        Ok(symbol) => {
                            let client = self.client.clone();
                            let order_manager = self.order_manager.clone();
                            let ev_tx = self.ev_tx.clone();

                            tokio::spawn(async move {
                                if let Err(error) = cancel_all(
                                    client.clone(),
                                    symbol.clone(),
                                    order_manager.clone(),
                                    ev_tx.clone()
                                ).await {
                                    error!(?error, %symbol, "Couldn't cancel all orders.");
                                }
                            });
                        }
                        Err(RecvError::Closed) => {
                            return Ok(());
                        }
                        Err(RecvError::Lagged(num)) => {
                            error!("{num} subscription requests were missed.");
                        }
                    }
                }
                message = read.next() => match message {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<Stream>(&text) {
                            Ok(Stream::EventStream(stream)) => {
                                self.process_message(
                                    stream,
                                    #[cfg(feature = "recording")]
                                    income_publisher.as_ref(),
                                )?;
                            }
                            Ok(Stream::Result(result)) => {
                                debug!(?result, "Subscription request response is received.");
                            }
                            Err(error) => {
                                error!(?error, %text, "Couldn't parse Stream.");
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        write.send(Message::Pong(data)).await?;
                        last_ping = Instant::now();
                    }
                    Some(Ok(Message::Close(close_frame))) => {
                        return Err(BinanceFuturesError::ConnectionAbort(
                            close_frame.map(|f| f.to_string()).unwrap_or(String::new())
                        ));
                    }
                    Some(Ok(Message::Binary(_)))
                    | Some(Ok(Message::Frame(_)))
                    | Some(Ok(Message::Pong(_))) => {}
                    Some(Err(error)) => {
                        return Err(BinanceFuturesError::from(error));
                    }
                    None => {
                        return Err(BinanceFuturesError::ConnectionInterrupted);
                    }
                }
            }
        }
    }
}

pub async fn cancel_all(
    client: BinanceFuturesClient,
    symbol: String,
    order_manager: SharedOrderManager,
    ev_tx: UnboundedSender<PublishEvent>,
) -> Result<(), BinanceFuturesError> {
    // todo: rate-limit throttling.
    client.cancel_all_orders(&symbol).await?;
    let orders = order_manager.lock().unwrap().cancel_all_from_rest(&symbol);
    for order in orders {
        // Ignore send errors during shutdown
        let _ = ev_tx
            .send(PublishEvent::LiveEvent(LiveEvent::Order {
                symbol: symbol.clone(),
                order,
            }));
    }
    Ok(())
}

pub async fn get_position_information(
    client: BinanceFuturesClient,
    mut symbols: HashSet<String>,
    ev_tx: UnboundedSender<PublishEvent>,
) -> Result<(), BinanceFuturesError> {
    // todo: rate-limit throttling.
    let position_information = client.get_position_information().await?;
    position_information.into_iter().for_each(|position| {
        symbols.remove(&position.symbol);
        // Ignore send errors during shutdown
        let _ = ev_tx
            .send(PublishEvent::LiveEvent(LiveEvent::Position {
                symbol: position.symbol,
                qty: position.position_amount,
                exch_ts: position.update_time * 1_000_000,
            }));
    });
    for symbol in symbols {
        // Ignore send errors during shutdown
        let _ = ev_tx
            .send(PublishEvent::LiveEvent(LiveEvent::Position {
                symbol,
                qty: 0.0,
                exch_ts: 0,
            }));
    }
    Ok(())
}
