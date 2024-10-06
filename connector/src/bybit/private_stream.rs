use std::time::Duration;

use chrono::Utc;
use futures_util::{stream::SplitSink, SinkExt, StreamExt};
use hftbacktest::{prelude::LiveEvent, types::Status};
use tokio::{net::TcpStream, select, sync::mpsc::UnboundedSender, time};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
    MaybeTlsStream,
    WebSocketStream,
};
use tracing::{debug, error};

use crate::{
    bybit::{
        msg::{Op, PrivateStreamMsg, PrivateStreamTopicMsg},
        ordermanager::{OrderExt, SharedOrderManager},
        rest::BybitClient,
        BybitError,
        SharedSymbolSet,
    },
    connector::PublishEvent,
    utils::sign_hmac_sha256,
};

pub struct PrivateStream {
    api_key: String,
    secret: String,
    ev_tx: UnboundedSender<PublishEvent>,
    order_manager: SharedOrderManager,
    symbols: SharedSymbolSet,
    client: BybitClient,
}

impl PrivateStream {
    pub fn new(
        api_key: String,
        secret: String,
        ev_tx: UnboundedSender<PublishEvent>,
        order_manager: SharedOrderManager,
        symbols: SharedSymbolSet,
        client: BybitClient,
    ) -> Self {
        Self {
            api_key,
            secret,
            ev_tx,
            order_manager,
            symbols,
            client,
        }
    }

    pub async fn cancel_all(&self, category: &str) -> Result<(), BybitError> {
        let symbols: Vec<_> = self.symbols.lock().unwrap().iter().cloned().collect();
        for symbol in symbols {
            // todo: rate-limit throttling.
            self.client.cancel_all_orders(category, &symbol).await?;

            let mut order_manager_ = self.order_manager.lock().unwrap();
            let orders = order_manager_.clear_orders(&symbol);
            for mut order in orders {
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

    pub async fn get_all_position(&self, category: &str) -> Result<(), BybitError> {
        let symbols: Vec<_> = self.symbols.lock().unwrap().iter().cloned().collect();
        for symbol in symbols {
            // todo: rate-limit throttling.
            let position = self
                .client
                .get_position_information(category, &symbol)
                .await?;
            position.into_iter().for_each(|position| {
                self.ev_tx
                    .send(PublishEvent::LiveEvent(LiveEvent::Position {
                        symbol: symbol.clone(),
                        qty: position.size,
                    }))
                    .unwrap();
            });
        }
        Ok(())
    }

    async fn handle_private_stream(
        &self,
        text: &str,
        write: &mut SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
    ) -> Result<(), BybitError> {
        let stream = serde_json::from_str::<PrivateStreamMsg>(text)?;
        match stream {
            PrivateStreamMsg::Op(resp) => {
                debug!(?resp, "OpResponse");
                if resp.op == "auth" {
                    if resp.success.unwrap() {
                        let op = Op {
                            req_id: "3".to_string(),
                            op: "subscribe".to_string(),
                            args: vec![
                                "order".to_string(),
                                "position".to_string(),
                                "execution".to_string(),
                                // todo: there is no orderLinkId, it requires a separate orderId
                                //       management
                                // "execution.fast".to_string()
                            ],
                        };
                        let s = serde_json::to_string(&op).unwrap();
                        write.send(Message::Text(s)).await?;
                    } else {
                        // return Err(Error::)
                    }
                }
            }
            PrivateStreamMsg::Topic(PrivateStreamTopicMsg::Position(data)) => {
                debug!(?data, "Position");
                for item in data.data {
                    self.ev_tx
                        .send(PublishEvent::LiveEvent(LiveEvent::Position {
                            symbol: item.symbol,
                            qty: item.size,
                        }))
                        .unwrap();
                }
            }
            PrivateStreamMsg::Topic(PrivateStreamTopicMsg::Execution(data)) => {
                debug!(?data, "Execution");
                let mut order_man_ = self.order_manager.lock().unwrap();
                for item in &data.data {
                    match order_man_.update_execution(item) {
                        Ok(OrderExt {
                            symbol: asset,
                            order,
                        }) => {
                            self.ev_tx
                                .send(PublishEvent::LiveEvent(LiveEvent::Order {
                                    symbol: asset,
                                    order,
                                }))
                                .unwrap();
                        }
                        Err(error) => {
                            error!(?error, ?data, "Couldn't update the execution data");
                        }
                    }
                }
            }
            PrivateStreamMsg::Topic(PrivateStreamTopicMsg::FastExecution(data)) => {
                debug!(?data, "FastExecution");
                let mut order_man_ = self.order_manager.lock().unwrap();
                for item in &data.data {
                    match order_man_.update_fast_execution(item) {
                        Ok(OrderExt {
                            symbol: asset,
                            order,
                        }) => {
                            self.ev_tx
                                .send(PublishEvent::LiveEvent(LiveEvent::Order {
                                    symbol: asset,
                                    order,
                                }))
                                .unwrap();
                        }
                        Err(error) => {
                            error!(?error, ?data, "Couldn't update the fast execution data");
                        }
                    }
                }
            }
            PrivateStreamMsg::Topic(PrivateStreamTopicMsg::Order(data)) => {
                debug!(?data, "Order");
                for order_msg in &data.data {
                    let mut order_manager = self.order_manager.lock().unwrap();
                    match order_manager.update_order(order_msg) {
                        Ok(OrderExt { symbol, order }) => {
                            self.ev_tx
                                .send(PublishEvent::LiveEvent(LiveEvent::Order { symbol, order }))
                                .unwrap();
                        }
                        Err(BybitError::PrefixUnmatched) => {
                            // The order is not created by this connector.
                        }
                        Err(error) => {
                            error!(?error, ?data, "Couldn't update the order data");
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn connect(&self, url: &str) -> Result<(), BybitError> {
        let request = url.into_client_request()?;
        let (ws_stream, _) = connect_async(request).await?;
        let (mut write, mut read) = ws_stream.split();
        let mut interval = time::interval(Duration::from_secs(10));

        let expires = Utc::now().timestamp_millis() + 5000;
        let signature = sign_hmac_sha256(&self.secret, &format!("GET/realtime{expires}"));

        let op = Op {
            req_id: "auth".to_string(),
            op: "auth".to_string(),
            args: vec![self.api_key.clone(), expires.to_string(), signature],
        };
        let s = serde_json::to_string(&op).unwrap();
        write.send(Message::Text(s)).await?;

        loop {
            select! {
                _ = interval.tick() => {
                    let op = Op {
                        req_id: "ping".to_string(),
                        op: "ping".to_string(),
                        args: vec![]
                    };
                    let s = serde_json::to_string(&op).unwrap();
                    write.send(Message::Text(s)).await?;
                }
                message = read.next() => {
                    match message {
                        Some(Ok(Message::Text(text))) => {
                            match self.handle_private_stream(
                                &text,
                                &mut write,
                            ).await {
                                Ok(_) => {}
                                Err(BybitError::PrefixUnmatched) => {
                                    // The order is not created by this connector.
                                }
                                Err(error) => {
                                    error!(%text, ?error, "Couldn't properly handle PrivateStreamMsg");
                                }
                            }
                        }
                        Some(Ok(Message::Ping(_))) => {
                            write.send(Message::Pong(Vec::new())).await?;
                        }
                        Some(Ok(Message::Close(close_frame))) => {
                            return Err(BybitError::ConnectionAbort(
                                close_frame.map(|f| f.to_string()).unwrap_or(String::new())
                            ));
                        }
                        Some(Ok(Message::Binary(_)))
                        | Some(Ok(Message::Frame(_)))
                        | Some(Ok(Message::Pong(_))) => {}
                        Some(Err(error)) => {
                            return Err(BybitError::from(error));
                        }
                        None => {
                            return Err(BybitError::ConnectionInterrupted);
                        }
                    }
                }
            }
        }
    }
}
