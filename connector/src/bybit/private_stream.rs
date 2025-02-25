use std::time::Duration;

use chrono::Utc;
use futures_util::{SinkExt, StreamExt, stream::SplitSink};
use hftbacktest::prelude::LiveEvent;
use tokio::{
    net::TcpStream,
    select,
    sync::{
        broadcast::{Receiver, error::RecvError},
        mpsc::UnboundedSender,
    },
    time,
};
use tokio_tungstenite::{
    MaybeTlsStream,
    WebSocketStream,
    connect_async,
    tungstenite::{Bytes, Message, client::IntoClientRequest},
};
use tracing::{debug, error};

use crate::{
    bybit::{
        BybitError,
        SharedSymbolSet,
        msg::{Op, PrivateStreamMsg, PrivateStreamTopicMsg},
        ordermanager::{OrderExt, SharedOrderManager},
        rest::BybitClient,
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
    category: String,
    client: BybitClient,
    symbol_rx: Receiver<String>,
}

impl PrivateStream {
    pub fn new(
        api_key: String,
        secret: String,
        ev_tx: UnboundedSender<PublishEvent>,
        order_manager: SharedOrderManager,
        symbols: SharedSymbolSet,
        category: String,
        client: BybitClient,
        symbol_rx: Receiver<String>,
    ) -> Self {
        Self {
            api_key,
            secret,
            ev_tx,
            order_manager,
            symbols,
            category,
            client,
            symbol_rx,
        }
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
                            req_id: "subscribe".to_string(),
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
                        write.send(Message::Text(s.into())).await?;
                    } else {
                        return Err(BybitError::AuthError {
                            msg: resp.ret_msg.unwrap(),
                            code: 0,
                        });
                    }
                } else if resp.op == "subscribe" {
                    if resp.success.unwrap() {
                        let symbols = self
                            .symbols
                            .lock()
                            .unwrap()
                            .iter()
                            .cloned()
                            .collect::<Vec<_>>();
                        let client = self.client.clone();
                        let category = self.category.clone();
                        let order_manager = self.order_manager.clone();
                        let ev_tx = self.ev_tx.clone();

                        tokio::spawn(async move {
                            for symbol in symbols {
                                // Cancel all orders in order to start with the clean state.
                                if let Err(error) = cancel_all(
                                    client.clone(),
                                    category.clone(),
                                    symbol.clone(),
                                    order_manager.clone(),
                                    ev_tx.clone(),
                                )
                                .await
                                {
                                    error!(
                                        ?error,
                                        %category,
                                        %symbol,
                                        "Couldn't cancel all orders."
                                    );
                                }

                                // Fetches the initial states such as positions and open orders.
                                if let Err(error) = get_position(
                                    client.clone(),
                                    category.clone(),
                                    symbol.clone(),
                                    ev_tx.clone(),
                                )
                                .await
                                {
                                    error!(
                                        ?error,
                                        %category,
                                        %symbol,
                                        "Couldn't cancel all orders."
                                    );
                                }
                            }
                        });
                    } else {
                        // todo
                    }
                }
            }
            PrivateStreamMsg::Topic(PrivateStreamTopicMsg::Position(data)) => {
                debug!(?data, "Position");
                for position in data.data {
                    let qty = match position.side.as_str() {
                        "Buy" => position.size,
                        "Sell" => -position.size,
                        _ => {
                            if position.size != 0.0 {
                                panic!("Unknown position side. position={position:?}");
                            }
                            0.0
                        }
                    };
                    self.ev_tx
                        .send(PublishEvent::LiveEvent(LiveEvent::Position {
                            symbol: position.symbol,
                            qty,
                            exch_ts: position.updated_time * 1_000_000,
                        }))
                        .unwrap();
                }
            }
            PrivateStreamMsg::Topic(PrivateStreamTopicMsg::Execution(data)) => {
                debug!(?data, "Execution");
                let mut order_manager = self.order_manager.lock().unwrap();
                for execution in &data.data {
                    match order_manager.update_execution(execution) {
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
                let mut order_manager = self.order_manager.lock().unwrap();
                for fast_execution in &data.data {
                    match order_manager.update_fast_execution(fast_execution) {
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
                for private_order in &data.data {
                    let mut order_manager = self.order_manager.lock().unwrap();
                    match order_manager.update_order(private_order) {
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

    pub async fn connect(&mut self, url: &str) -> Result<(), BybitError> {
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
        write.send(Message::Text(s.into())).await?;

        loop {
            select! {
                _ = interval.tick() => {
                    let op = Op {
                        req_id: "ping".to_string(),
                        op: "ping".to_string(),
                        args: vec![]
                    };
                    let s = serde_json::to_string(&op).unwrap();
                    write.send(Message::Text(s.into())).await?;
                }
                msg = self.symbol_rx.recv() => {
                    match msg {
                        Ok(symbol) => {
                            let client = self.client.clone();
                            let category = self.category.clone();
                            let order_manager = self.order_manager.clone();
                            let ev_tx = self.ev_tx.clone();

                            tokio::spawn(async move {
                                // Cancel all orders in order to start with the clean state.
                                if let Err(error) = cancel_all(
                                    client.clone(),
                                    category.clone(),
                                    symbol.clone(),
                                    order_manager.clone(),
                                    ev_tx.clone()
                                ).await {
                                    error!(
                                        ?error,
                                        %category,
                                        %symbol,
                                        "Couldn't cancel all orders."
                                    );
                                }

                                // Fetches the initial states such as positions and open orders.
                                if let Err(error) = get_position(
                                    client.clone(),
                                    category.clone(),
                                    symbol.clone(),
                                    ev_tx.clone()
                                ).await {
                                    error!(
                                        ?error,
                                        %category,
                                        %symbol,
                                        "Couldn't cancel all orders."
                                    );
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
                            write.send(Message::Pong(Bytes::default())).await?;
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

pub async fn get_position(
    client: BybitClient,
    category: String,
    symbol: String,
    ev_tx: UnboundedSender<PublishEvent>,
) -> Result<(), BybitError> {
    // todo: rate-limit throttling.
    let position = client.get_position_information(&category, &symbol).await?;
    position.into_iter().for_each(|position| {
        let qty = match position.side.as_str() {
            "Buy" => position.size,
            "Sell" => -position.size,
            _ => {
                if position.size != 0.0 {
                    panic!("Unknown position side. position={position:?}");
                }
                0.0
            }
        };
        ev_tx
            .send(PublishEvent::LiveEvent(LiveEvent::Position {
                symbol: symbol.to_string(),
                qty,
                exch_ts: position.updated_time,
            }))
            .unwrap();
    });
    Ok(())
}

pub async fn cancel_all(
    client: BybitClient,
    category: String,
    symbol: String,
    order_manager: SharedOrderManager,
    ev_tx: UnboundedSender<PublishEvent>,
) -> Result<(), BybitError> {
    // todo: rate-limit throttling.
    client.cancel_all_orders(&category, &symbol).await?;
    let orders = order_manager.lock().unwrap().cancel_all(&symbol);
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
