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
    binancespot::{
        BinanceSpotError,
        SharedSymbolSet,
        msg::stream::{
            SignParams,
            SignRequest,
            UserEventStream,
            UserStream,
            UserStreamSubscribeRequest,
        },
        ordermanager::SharedOrderManager,
        rest::BinanceSpotClient,
    },
    connector::PublishEvent,
    utils::{generate_rand_string, get_timestamp, sign_ed25519},
};

pub struct UserDataStream {
    symbols: SharedSymbolSet,
    client: BinanceSpotClient,
    ev_tx: UnboundedSender<PublishEvent>,
    order_manager: SharedOrderManager,
    symbol_rx: Receiver<String>,
}

impl UserDataStream {
    pub fn new(
        client: BinanceSpotClient,
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

    // pub async fn get_listen_key(&self) -> Result<String, BinanceSpotError> {
    //     Ok(self.client.start_user_data_stream().await?)
    // }

    fn process_message(&self, stream: UserEventStream) -> Result<(), BinanceSpotError> {
        match stream {
            UserEventStream::OutboundAccountPosition(data) => {
                let event_time = data.event_time;
                for balance in data.balances {
                    self.ev_tx
                        .send(PublishEvent::LiveEvent(LiveEvent::Position {
                            symbol: balance.asset,
                            qty: balance.free,
                            exch_ts: event_time * 1_000_000,
                        }))
                        .unwrap();
                }
            }
            UserEventStream::BalanceUpdate(_data) => {}
            UserEventStream::ExecutionReport(data) => {
                match self.order_manager.lock().unwrap().update_from_ws(&data) {
                    Ok(Some(order)) => {
                        self.ev_tx
                            .send(PublishEvent::LiveEvent(LiveEvent::Order {
                                symbol: data.symbol.clone(),
                                order,
                            }))
                            .unwrap();
                    }
                    Ok(None) => {
                        // order已经删除
                    }
                    Err(BinanceSpotError::PrefixUnmatched) => {
                        // order不是当前connector创建的
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
            UserEventStream::ListStatus(_data) => {}
        }
        Ok(())
    }

    pub async fn connect(&mut self, url: &str) -> Result<(), BinanceSpotError> {
        let request = url.into_client_request()?;
        let (ws_stream, _) = connect_async(request).await?;
        let (mut write, mut read) = ws_stream.split();
        let mut ping_checker = time::interval(Duration::from_secs(10));

        let symbols: HashSet<_> = self.symbols.lock().unwrap().iter().cloned().collect();
        let client = self.client.clone();
        let order_manager = self.order_manager.clone();
        let ev_tx = self.ev_tx.clone();
        let mut last_ping = Instant::now();

        let mut req = SignRequest {
            id: generate_rand_string(16),
            method: "session.logon".to_string(),
            params: SignParams {
                api_key: self.client.api_key.clone(),
                signature: None,
                timestamp: get_timestamp(),
            },
        };
        if let Ok(payload) = serde_qs::to_string(&req) {
            let signature = sign_ed25519(&self.client.secret, &payload);
            req.params.signature = Some(signature);
            let _ = write
                .send(Message::Text(serde_json::to_string(&req).unwrap().into()))
                .await;
        }

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
                _ = ping_checker.tick() => {
                    if last_ping.elapsed() > Duration::from_secs(300) {
                        warn!("Ping timeout.");
                        return Err(BinanceSpotError::ConnectionInterrupted);
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
                        match serde_json::from_str::<UserStream>(&text) {
                            Ok(UserStream::EventStream(stream)) => {
                                self.process_message(stream.event)?;
                            }
                            Ok(UserStream::AuthResponse(result)) => {
                                debug!(?result, "Subscription request response is received.");
                                if result.status == 200 {
                                    write.send(Message::Text(
                                        serde_json::to_string(&UserStreamSubscribeRequest {
                                            id: generate_rand_string(16),
                                            method: "userDataStream.subscribe".to_string(),
                                        })
                                        .unwrap().into(),
                                    )).await?;
                                }
                            }
                            Ok(UserStream::SubscribeResponse(resp)) => {
                                debug!(?resp, "Subscription request error response is received.");
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
                        return Err(BinanceSpotError::ConnectionAbort(
                            close_frame.map(|f| f.to_string()).unwrap_or(String::new())
                        ));
                    }
                    Some(Ok(Message::Binary(_)))
                    | Some(Ok(Message::Frame(_)))
                    | Some(Ok(Message::Pong(_))) => {}
                    Some(Err(error)) => {
                        return Err(BinanceSpotError::from(error));
                    }
                    None => {
                        return Err(BinanceSpotError::ConnectionInterrupted);
                    }
                }
            }
        }
    }
}

pub async fn cancel_all(
    client: BinanceSpotClient,
    symbol: String,
    order_manager: SharedOrderManager,
    ev_tx: UnboundedSender<PublishEvent>,
) -> Result<(), BinanceSpotError> {
    // todo: rate-limit throttling.
    client.cancel_all_orders(&symbol).await?;
    let orders = order_manager.lock().unwrap().cancel_all_from_rest(&symbol);
    for order in orders {
        ev_tx
            .send(PublishEvent::LiveEvent(LiveEvent::Order {
                symbol: symbol.clone(),
                order,
            }))
            .unwrap();
    }
    Ok(())
}

pub async fn get_position_information(
    client: BinanceSpotClient,
    mut symbols: HashSet<String>,
    ev_tx: UnboundedSender<PublishEvent>,
) -> Result<(), BinanceSpotError> {
    // todo: rate-limit throttling.
    let account_infomation = client.get_account_information().await?;
    let exch_ts = account_infomation.update_time * 1_000_000;
    account_infomation.balances.into_iter().for_each(|balance| {
        symbols.remove(&balance.asset);
        ev_tx
            .send(PublishEvent::LiveEvent(LiveEvent::Position {
                symbol: balance.asset,
                qty: balance.free,
                exch_ts,
            }))
            .unwrap();
    });
    for symbol in symbols {
        ev_tx
            .send(PublishEvent::LiveEvent(LiveEvent::Position {
                symbol,
                qty: 0.0,
                exch_ts: 0,
            }))
            .unwrap();
    }
    Ok(())
}
