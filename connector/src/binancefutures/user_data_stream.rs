use std::{collections::HashSet, time::Duration};

use futures_util::{SinkExt, StreamExt};
use hftbacktest::prelude::*;
use tokio::{select, sync::mpsc::UnboundedSender, time};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
};
use tracing::error;

use crate::{
    binancefutures::{
        msg::stream::Stream,
        ordermanager::{OrderManager, SharedOrderManager},
        rest::BinanceFuturesClient,
        BinanceFuturesError,
        SharedInstrumentMap,
    },
    connector::PublishMessage,
};

pub struct UserDataStream {
    instruments: SharedInstrumentMap,
    client: BinanceFuturesClient,
    ev_tx: UnboundedSender<PublishMessage>,
    order_manager: SharedOrderManager,
    prefix: String,
}

impl UserDataStream {
    pub fn new(
        client: BinanceFuturesClient,
        ev_tx: UnboundedSender<PublishMessage>,
        order_manager: SharedOrderManager,
        instruments: SharedInstrumentMap,
        prefix: String,
    ) -> Self {
        Self {
            instruments,
            client,
            ev_tx,
            order_manager,
            prefix,
        }
    }

    pub async fn cancel_all(&self) -> Result<(), BinanceFuturesError> {
        let symbols: Vec<_> = self.instruments.lock().unwrap().keys().cloned().collect();
        for symbol in symbols {
            // todo: rate-limit throttling.
            self.client.cancel_all_orders(&symbol).await?;
            let mut order_manager = self.order_manager.lock().unwrap();
            let canceled_orders = order_manager.cancel_all_from_rest(&symbol);
            for mut order in canceled_orders {
                order.status = Status::Canceled;
                self.ev_tx
                    .send(PublishMessage::LiveEvent(LiveEvent::Order {
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
        let mut symbols: HashSet<_> = self.instruments.lock().unwrap().keys().cloned().collect();
        position_information.into_iter().for_each(|position| {
            symbols.remove(&position.symbol);
            self.ev_tx
                .send(PublishMessage::LiveEvent(LiveEvent::Position {
                    symbol: position.symbol,
                    qty: position.position_amount,
                }))
                .unwrap();
        });
        for symbol in symbols {
            self.ev_tx
                .send(PublishMessage::LiveEvent(LiveEvent::Position {
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

    fn process_message(&self, stream: Stream) -> Result<(), BinanceFuturesError> {
        match stream {
            Stream::DepthUpdate(_) | Stream::Trade(_) => unreachable!(),
            Stream::ListenKeyExpired(_) => {
                return Err(BinanceFuturesError::ListenKeyExpired);
            }
            Stream::AccountUpdate(data) => {
                for position in data.account.position {
                    self.ev_tx
                        .send(PublishMessage::LiveEvent(LiveEvent::Position {
                            symbol: position.symbol,
                            qty: position.position_amount,
                        }))
                        .unwrap();
                }
            }
            Stream::OrderTradeUpdate(data) => {
                if let Some(asset_info) = self.instruments.lock().unwrap().get(&data.order.symbol) {
                    if let Some(order_id) = OrderManager::parse_client_order_id(
                        &data.order.client_order_id,
                        &self.prefix,
                        &data.order.symbol,
                    ) {
                        let order = Order {
                            qty: data.order.original_qty,
                            leaves_qty: data.order.original_qty
                                - data.order.order_filled_accumulated_qty,
                            price_tick: (data.order.original_price / asset_info.tick_size).round()
                                as i64,
                            tick_size: asset_info.tick_size,
                            side: data.order.side,
                            time_in_force: data.order.time_in_force,
                            exch_timestamp: data.transaction_time * 1_000_000,
                            status: data.order.order_status,
                            local_timestamp: 0,
                            req: Status::None,
                            exec_price_tick: (data.order.last_filled_price / asset_info.tick_size)
                                .round() as i64,
                            exec_qty: data.order.order_last_filled_qty,
                            order_id,
                            order_type: data.order.order_type,
                            // Invalid information
                            q: Box::new(()),
                            maker: false,
                        };

                        let order = self.order_manager.lock().unwrap().update_from_ws(
                            asset_info.symbol.clone(),
                            data.order.client_order_id,
                            order,
                        );
                        if let Some(order) = order {
                            self.ev_tx
                                .send(PublishMessage::LiveEvent(LiveEvent::Order {
                                    symbol: data.order.symbol,
                                    order,
                                }))
                                .unwrap();
                        }
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
                            Ok(stream) => {
                                self.process_message(stream)?;
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
