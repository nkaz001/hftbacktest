use std::{collections::HashMap, time::Duration};

use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use hftbacktest::types::{ErrorKind, LiveError, LiveEvent};
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
    tungstenite::{Bytes, Message, client::IntoClientRequest},
};
use tracing::{error, info};

use crate::{
    bybit::{
        BybitError,
        msg::{Op, Order, TradeOp, TradeStreamMsg},
        ordermanager::{OrderExt, SharedOrderManager},
    },
    connector::PublishEvent,
    utils::{generate_rand_string, sign_hmac_sha256},
};

#[derive(Debug, Clone)]
pub struct OrderOp {
    pub op: &'static str,
    pub bybit_order: Order,
}

pub struct TradeStream {
    api_key: String,
    secret: String,
    ev_tx: UnboundedSender<PublishEvent>,
    order_manager: SharedOrderManager,
    order_rx: Receiver<OrderOp>,
}

impl TradeStream {
    pub fn new(
        api_key: String,
        secret: String,
        ev_tx: UnboundedSender<PublishEvent>,
        order_manager: SharedOrderManager,
        order_rx: Receiver<OrderOp>,
    ) -> Self {
        Self {
            api_key,
            secret,
            ev_tx,
            order_manager,
            order_rx,
        }
    }

    pub async fn connect(&mut self, url: &str) -> Result<(), BybitError> {
        let mut request = url.into_client_request()?;
        let _ = request.headers_mut();

        let (ws_stream, _) = connect_async(request).await?;
        let (mut write, mut read) = ws_stream.split();
        let mut interval = time::interval(Duration::from_secs(60));

        let expires = Utc::now().timestamp_millis() + 5000;
        let signature = sign_hmac_sha256(&self.secret, &format!("GET/realtime{expires}"));

        let op = TradeOp {
            req_id: "auth".to_string(),
            header: Default::default(),
            op: "auth",
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
                order = self.order_rx.recv() => {
                    match order {
                        Ok(order) => {
                            let req_id = {
                                format!(
                                    "{}/{}",
                                    order.bybit_order.order_link_id.clone(),
                                    generate_rand_string(8),
                                )
                            };
                            let op = TradeOp {
                                req_id,
                                header: {
                                    let mut header = HashMap::new();
                                    header.insert(
                                        "X-BAPI-TIMESTAMP".to_string(),
                                        Utc::now().timestamp_millis().to_string()
                                    );
                                    header.insert(
                                        "X-BAPI-RECV-WINDOW".to_string(),
                                        "5000".to_string()
                                    );
                                    header
                                },
                                op: order.op,
                                args: vec![order.bybit_order]
                            };
                            let s = serde_json::to_string(&op).unwrap();
                            write.send(Message::Text(s.into())).await?;
                        }
                        Err(RecvError::Closed) => {
                            return Ok(());
                        }
                        Err(RecvError::Lagged(num)) => {
                            error!("{num} order requests were missed.");
                        }
                    }
                }
                message = read.next() => {
                    match message {
                        Some(Ok(Message::Text(text))) => {
                            if let Err(error) = self.handle_trade_stream(&text).await {
                               error!(?error, %text, "Couldn't properly handle TradeStreamMsg.");
                            };
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

    async fn handle_trade_stream(&self, text: &str) -> Result<(), BybitError> {
        let stream = serde_json::from_str::<TradeStreamMsg>(text)?;
        if stream.op == "auth" {
            if stream.ret_code != 0 {
                let error = BybitError::AuthError {
                    code: stream.ret_code,
                    msg: stream.ret_msg.clone(),
                };
                self.ev_tx
                    .send(PublishEvent::LiveEvent(LiveEvent::Error(LiveError::with(
                        ErrorKind::CriticalConnectionError,
                        error.to_value(),
                    ))))
                    .unwrap();
                return Err(error);
            }
        } else if stream.op == "order.create" {
            let req_id = stream.req_id.ok_or(BybitError::InvalidReqId)?;
            if stream.ret_code != 0 {
                /*
                10404: 1. op type is not found; 2. category is not correct/supported
                10429: System level frequency protection
                20006: reqId is duplicated
                10016: 1. internal server error; 2. Service is restarting
                10019: ws trade service is restarting, do not accept new request,
                       but the request in the process is not affected.
                       You can build new connection to be routed to normal service
                10001: Param error
                 */
                let mut order_man_ = self.order_manager.lock().unwrap();
                let order_link_id = req_id.split('/').next().ok_or(BybitError::InvalidReqId)?;
                let OrderExt { symbol, order } = order_man_.update_submit_fail(order_link_id)?;
                self.ev_tx
                    .send(PublishEvent::LiveEvent(LiveEvent::Order { symbol, order }))
                    .unwrap();
                self.ev_tx
                    .send(PublishEvent::LiveEvent(LiveEvent::Error(LiveError::with(
                        ErrorKind::OrderError,
                        BybitError::OrderError {
                            code: stream.ret_code,
                            msg: stream.ret_msg.clone(),
                        }
                        .to_value(),
                    ))))
                    .unwrap();
            }
        } else if stream.op == "order.cancel" {
            let req_id = stream.req_id.ok_or(BybitError::InvalidReqId)?;
            if stream.ret_code != 0 {
                /*
                10404: 1. op type is not found; 2. category is not correct/supported
                10429: System level frequency protection
                20006: reqId is duplicated
                10016: 1. internal server error; 2. Service is restarting
                10019: ws trade service is restarting, do not accept new request,
                       but the request in the process is not affected.
                       You can build new connection to be routed to normal service
                10001: Param error
                 */
                let mut order_man_ = self.order_manager.lock().unwrap();
                let order_link_id = req_id.split('/').next().ok_or(BybitError::InvalidReqId)?;
                let OrderExt { symbol, order } = order_man_.update_cancel_fail(order_link_id)?;
                self.ev_tx
                    .send(PublishEvent::LiveEvent(LiveEvent::Order { symbol, order }))
                    .unwrap();
                self.ev_tx
                    .send(PublishEvent::LiveEvent(LiveEvent::Error(LiveError::with(
                        ErrorKind::OrderError,
                        BybitError::OrderError {
                            code: stream.ret_code,
                            msg: stream.ret_msg.clone(),
                        }
                        .to_value(),
                    ))))
                    .unwrap();
            }
        } else {
            info!(?stream, "trade stream");
        }
        Ok(())
    }
}
