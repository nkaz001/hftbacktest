use std::{collections::HashSet, time::Duration};

use futures_util::{SinkExt, StreamExt};
use hftbacktest::prelude::*;
use tokio::{select, sync::mpsc::UnboundedSender, time};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
};
use tracing::{debug, error};

use crate::{
    binancefutures::{
        msg::stream::{EventStream, Stream},
        ordermanager::SharedOrderManager,
        rest::BinanceFuturesClient,
        BinanceFuturesError,
        SharedSymbolSet,
    },
    connector::PublishEvent,
};

pub struct UserDataStream {
    symbols: SharedSymbolSet,
    client: BinanceFuturesClient,
    ev_tx: UnboundedSender<PublishEvent>,
    order_manager: SharedOrderManager,
}

impl UserDataStream {
    pub fn new(
        client: BinanceFuturesClient,
        ev_tx: UnboundedSender<PublishEvent>,
        order_manager: SharedOrderManager,
        symbols: SharedSymbolSet,
    ) -> Self {
        Self {
            symbols,
            client,
            ev_tx,
            order_manager,
        }
    }

    pub async fn cancel_all(&self) -> Result<(), BinanceFuturesError> {
        let symbols: Vec<_> = self.symbols.lock().unwrap().iter().cloned().collect();
        for symbol in symbols {
            // todo: rate-limit throttling.
            self.client.cancel_all_orders(&symbol).await?;
            let mut order_manager = self.order_manager.lock().unwrap();
            let canceled_orders = order_manager.cancel_all_from_rest(&symbol);
            for mut order in canceled_orders {
                order.status = Status::Canceled;
                self.ev_tx
                    .send(PublishEvent::LiveEvent(LiveEvent::Order {
                        symbol: symbol.clone(),
                        order,
                    }))
                    .unwrap();
            }
        }
        Ok(())
    }

    pub async fn get_position_information(&self) -> Result<(), BinanceFuturesError> {
        let position_information = self.client.get_position_information().await?;
        let mut symbols: HashSet<_> = self.symbols.lock().unwrap().iter().cloned().collect();
        position_information.into_iter().for_each(|position| {
            symbols.remove(&position.symbol);
            self.ev_tx
                .send(PublishEvent::LiveEvent(LiveEvent::Position {
                    symbol: position.symbol,
                    qty: position.position_amount,
                }))
                .unwrap();
        });
        for symbol in symbols {
            self.ev_tx
                .send(PublishEvent::LiveEvent(LiveEvent::Position {
                    symbol,
                    qty: 0.0,
                }))
                .unwrap();
        }
        Ok(())
    }

    pub async fn get_listen_key(&self) -> Result<String, BinanceFuturesError> {
        Ok(self.client.start_user_data_stream().await?)
    }

    fn process_message(&self, stream: EventStream) -> Result<(), BinanceFuturesError> {
        match stream {
            EventStream::DepthUpdate(_) | EventStream::Trade(_) => unreachable!(),
            EventStream::ListenKeyExpired(_) => {
                return Err(BinanceFuturesError::ListenKeyExpired);
            }
            EventStream::AccountUpdate(data) => {
                for position in data.account.position {
                    self.ev_tx
                        .send(PublishEvent::LiveEvent(LiveEvent::Position {
                            symbol: position.symbol,
                            qty: position.position_amount,
                        }))
                        .unwrap();
                }
            }
            EventStream::OrderTradeUpdate(data) => {
                let result = self.order_manager.lock().unwrap().update_from_ws(&data);
                match result {
                    Ok(Some(order)) => {
                        self.ev_tx
                            .send(PublishEvent::LiveEvent(LiveEvent::Order {
                                symbol: data.order.symbol,
                                order,
                            }))
                            .unwrap();
                    }
                    Ok(None) => {
                        // This order is already deleted.
                    }
                    Err(BinanceFuturesError::PrefixUnmatched) => {
                        // This order is not created by this connector.
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
        }
        Ok(())
    }

    pub async fn connect(&mut self, url: &str) -> Result<(), BinanceFuturesError> {
        let request = url.into_client_request()?;
        let (ws_stream, _) = connect_async(request).await?;
        let (mut write, mut read) = ws_stream.split();
        let mut interval = time::interval(Duration::from_secs(60 * 30));
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
                        }
                    });
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
