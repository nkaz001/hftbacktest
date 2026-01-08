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
use tracing::{debug, error, warn};

use crate::{
    binancepapi::{
        BinancePapiError,
        SharedSymbolSet,
        msg::stream::{EventStream, Stream},
        ordermanager::SharedOrderManager,
        rest::BinancePapiClient,
    },
    connector::PublishEvent,
};

pub struct UserDataStream {
    symbols: SharedSymbolSet,
    client: BinancePapiClient,
    ev_tx: UnboundedSender<PublishEvent>,
    order_manager: SharedOrderManager,
    symbol_rx: Receiver<String>,
}

impl UserDataStream {
    pub fn new(
        client: BinancePapiClient,
        ev_tx: UnboundedSender<PublishEvent>,
        order_manager: SharedOrderManager,
        symbols: SharedSymbolSet,
        symbol_rx: Receiver<String>,
    ) -> Self {
        Self {
            symbols,
            client,
            ev_tx,
            order_manager,
            symbol_rx,
        }
    }

    pub async fn get_listen_key(&self) -> Result<String, BinancePapiError> {
        Ok(self.client.start_user_data_stream().await?)
    }

    fn process_message(&self, stream: EventStream) -> Result<(), BinancePapiError> {
        match stream {
            EventStream::DepthUpdate(_) | EventStream::Trade(_) => unreachable!(),
            EventStream::ListenKeyExpired(_) => {
                return Err(BinancePapiError::ListenKeyExpired);
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
                    Err(BinancePapiError::PrefixUnmatched) => {
                        // This order is not created by this connector.
                    }
                    Err(BinancePapiError::OrderNotFound) => {
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
                // required to fully utilize it.
            }
        }
        Ok(())
    }

    /// Connect to PAPI user data stream
    /// URL format: wss://fstream.binance.com/pm/ws/{listenKey}
    pub async fn connect(&mut self, url: &str) -> Result<(), BinancePapiError> {
        let request = url.into_client_request()?;
        let (ws_stream, _) = connect_async(request).await?;
        let (mut write, mut read) = ws_stream.split();

        // Keepalive every 30 minutes (listenKey expires in 60 minutes)
        let mut keepalive_interval = time::interval(Duration::from_secs(60 * 30));
        let mut ping_checker = time::interval(Duration::from_secs(10));

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

            // Fetches the initial states such as positions.
            if let Err(error) =
                get_position_information(client.clone(), symbols, ev_tx.clone()).await
            {
                error!(?error, "Couldn't get position information.");
            }
        });

        loop {
            select! {
                _ = keepalive_interval.tick() => {
                    self.order_manager
                        .lock()
                        .unwrap()
                        .gc();
                    let client_ = self.client.clone();
                    tokio::spawn(async move {
                        if let Err(error) = client_.keepalive_user_data_stream().await {
                            error!(?error, "Failed keepalive user data stream.");
                        }
                    });
                }
                _ = ping_checker.tick() => {
                    if last_ping.elapsed() > Duration::from_secs(300) {
                        warn!("Ping timeout.");
                        return Err(BinancePapiError::ConnectionInterrupted);
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
                                self.process_message(stream)?;
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
                        return Err(BinancePapiError::ConnectionAbort(
                            close_frame.map(|f| f.to_string()).unwrap_or(String::new())
                        ));
                    }
                    Some(Ok(Message::Binary(_)))
                    | Some(Ok(Message::Frame(_)))
                    | Some(Ok(Message::Pong(_))) => {}
                    Some(Err(error)) => {
                        return Err(BinancePapiError::from(error));
                    }
                    None => {
                        return Err(BinancePapiError::ConnectionInterrupted);
                    }
                }
            }
        }
    }
}

pub async fn cancel_all(
    client: BinancePapiClient,
    symbol: String,
    order_manager: SharedOrderManager,
    ev_tx: UnboundedSender<PublishEvent>,
) -> Result<(), BinancePapiError> {
    client.cancel_all_um_orders(&symbol).await?;
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
    client: BinancePapiClient,
    mut symbols: HashSet<String>,
    ev_tx: UnboundedSender<PublishEvent>,
) -> Result<(), BinancePapiError> {
    let position_information = client.get_um_position_risk().await?;
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
