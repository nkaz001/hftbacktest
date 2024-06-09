use std::{collections::HashMap, sync::mpsc::Sender, time::Duration};

use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use tokio::{
    select,
    sync::mpsc::{unbounded_channel, UnboundedReceiver},
    time,
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
};
use tracing::{error, info};

use crate::{
    connector::{
        bybit::{
            msg,
            msg::{
                Op,
                Order as BybitOrder,
                OrderBook,
                OrderResponseData,
                PrivateStreamMsg,
                PrivateStreamTopicMsg,
                PublicStreamMsg,
                TradeOp,
                TradeStreamMsg,
            },
            ordermanager::{OrderManager, OrderManagerError, WrappedOrderManager},
            BybitError,
        },
        util::sign_hmac_sha256,
    },
    live::Asset,
    prelude::ErrorKind,
    types,
    types::{Depth, Error, LiveEvent, Order, OrderResponse, Side, Status, Trade, BUY, SELL},
};

pub struct BybitOrderReq {
    pub op: String,
    pub bybit_order: BybitOrder,
    pub tx: Sender<LiveEvent>,
}

fn parse_depth(
    bids: Vec<(String, String)>,
    asks: Vec<(String, String)>,
) -> Result<(Vec<(f32, f32)>, Vec<(f32, f32)>), anyhow::Error> {
    let mut bids_ = Vec::with_capacity(bids.len());
    for (px, qty) in bids {
        bids_.push(parse_px_qty_tup(px, qty)?);
    }
    let mut asks_ = Vec::with_capacity(asks.len());
    for (px, qty) in asks {
        asks_.push(parse_px_qty_tup(px, qty)?);
    }
    Ok((bids_, asks_))
}

fn parse_px_qty_tup(px: String, qty: String) -> Result<(f32, f32), anyhow::Error> {
    Ok((px.parse()?, qty.parse()?))
}

pub async fn connect_public(
    url: &str,
    ev_tx: Sender<LiveEvent>,
    assets: HashMap<String, Asset>,
    topics: Vec<String>,
) -> Result<(), anyhow::Error> {
    let mut request = url.into_client_request()?;
    let _ = request.headers_mut();

    let (ws_stream, _) = connect_async(request).await?;
    let (mut write, mut read) = ws_stream.split();
    let mut interval = time::interval(Duration::from_secs(15));

    let mut args = Vec::new();
    for topic in topics {
        let mut topics_ = assets
            .keys()
            .map(|symbol| format!("{topic}.{symbol}"))
            .collect();
        args.append(&mut topics_);
    }

    let op = Op {
        req_id: "subscribe".to_string(),
        op: "subscribe".to_string(),
        args,
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
                        let stream = match serde_json::from_str::<PublicStreamMsg>(&text) {
                            Ok(stream) => stream,
                            Err(error) => {
                                error!(?error, %text, "Couldn't parse PublicStreamMsg.");
                                continue;
                            }
                        };
                        match stream {
                            PublicStreamMsg::Op(resp) => {
                                info!(?resp, "Op");
                            }
                            PublicStreamMsg::Topic(stream) => {
                                if stream.topic.starts_with("orderbook") {
                                    let data: OrderBook = serde_json::from_value(stream.data)?;
                                    let (bids, asks) = parse_depth(data.bids, data.asks)?;
                                    let asset_info = assets
                                        .get(&data.symbol)
                                        .ok_or(BybitError::AssetNotFound)?;
                                    ev_tx.send(
                                        LiveEvent::Depth(
                                            Depth {
                                                asset_no: asset_info.asset_no,
                                                exch_ts: stream.cts.unwrap() * 1_000_000,
                                                local_ts: Utc::now().timestamp_nanos_opt().unwrap(),
                                                bids,
                                                asks,
                                            }
                                        )
                                    ).unwrap();
                                } else if stream.topic.starts_with("publicTrade") {
                                    let data: Vec<msg::Trade> = serde_json::from_value(stream.data)?;
                                    for item in data {
                                        let asset_info = assets
                                            .get(&item.symbol)
                                            .ok_or(BybitError::AssetNotFound)?;
                                        ev_tx.send(
                                            LiveEvent::Trade(
                                                Trade {
                                                    asset_no: asset_info.asset_no,
                                                    exch_ts: item.ts * 1_000_000,
                                                    local_ts: Utc::now().timestamp_nanos_opt().unwrap(),
                                                    side: {
                                                        if item.side == Side::Sell {
                                                            SELL as i8
                                                        } else {
                                                            BUY as i8
                                                        }
                                                    },
                                                    price: item.trade_price,
                                                    qty: item.trade_size,
                                                }
                                            )
                                        ).unwrap();
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Binary(_))) => {}
                    Some(Ok(Message::Ping(_))) => {
                        write.send(Message::Pong(Vec::new())).await?;
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Close(close_frame))) => {
                        info!(?close_frame, "close");
                        break;
                    }
                    Some(Ok(Message::Frame(_))) => {}
                    Some(Err(e)) => {
                        return Err(anyhow::Error::from(e));
                    }
                    None => {
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}

pub async fn connect_private(
    url: &str,
    api_key: &str,
    secret: &str,
    ev_tx: Sender<LiveEvent>,
    assets: HashMap<String, Asset>,
    order_man: WrappedOrderManager,
) -> Result<(), anyhow::Error> {
    let mut request = url.into_client_request()?;
    let _ = request.headers_mut();

    let (ws_stream, _) = connect_async(request).await?;
    let (mut write, mut read) = ws_stream.split();
    let mut interval = time::interval(Duration::from_secs(10));

    let expires = Utc::now().timestamp_millis() + 5000;
    let signature = sign_hmac_sha256(secret, &format!("GET/realtime{expires}"));

    let op = Op {
        req_id: "1".to_string(),
        op: "auth".to_string(),
        args: vec![api_key.to_string(), expires.to_string(), signature],
    };
    let s = serde_json::to_string(&op).unwrap();
    write.send(Message::Text(s)).await?;

    loop {
        select! {
            _ = interval.tick() => {
                let op = Op {
                    req_id: "2".to_string(),
                    op: "ping".to_string(),
                    args: vec![]
                };
                let s = serde_json::to_string(&op).unwrap();
                write.send(Message::Text(s)).await?;
            }
            message = read.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        let stream = match serde_json::from_str::<PrivateStreamMsg>(&text) {
                            Ok(stream) => stream,
                            Err(error) => {
                                error!(?error, %text, "Couldn't parse PrivateStreamMsg.");
                                continue;
                            }
                        };
                        match stream {
                            PrivateStreamMsg::Op(resp) => {
                                info!(?resp, "OpResponse");
                                if resp.op == "auth" {
                                    if resp.success.unwrap() {
                                        let op = Op {
                                            req_id: "3".to_string(),
                                            op: "subscribe".to_string(),
                                            args: vec![
                                                "position".to_string(), "execution.fast".to_string()
                                            ]
                                        };
                                        let s = serde_json::to_string(&op).unwrap();
                                        write.send(Message::Text(s)).await?;
                                    } else {
                                        // return Err(Error::)
                                    }
                                }
                            }
                            PrivateStreamMsg::Topic(PrivateStreamTopicMsg::Position(data)) => {
                                info!(?data, "Position");
                            }
                            PrivateStreamMsg::Topic(PrivateStreamTopicMsg::FastExecution(data)) => {
                                info!(?data, "FastExecution");
                                for item in data.data {

                                }
                            }
                            PrivateStreamMsg::Topic(PrivateStreamTopicMsg::Order(data)) => {
                                for item in data.data {
                                    let mut order_man_ = order_man.lock().unwrap();
                                    order_man_.update(&item);
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Binary(_))) => {}
                    Some(Ok(Message::Ping(_))) => {
                        write.send(Message::Pong(Vec::new())).await?;
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Close(close_frame))) => {
                        info!(?close_frame, "close");
                        break;
                    }
                    Some(Ok(Message::Frame(_))) => {}
                    Some(Err(e)) => {
                        return Err(anyhow::Error::from(e));
                    }
                    None => {
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}

pub async fn connect_trade(
    url: &str,
    api_key: &str,
    secret: &str,
    ev_tx: Sender<LiveEvent>,
    order_rx: &mut UnboundedReceiver<BybitOrderReq>,
    order_man: WrappedOrderManager,
) -> Result<(), anyhow::Error> {
    let mut request = url.into_client_request()?;
    let _ = request.headers_mut();

    let (ws_stream, _) = connect_async(request).await?;
    let (mut write, mut read) = ws_stream.split();
    let mut interval = time::interval(Duration::from_secs(60));

    let expires = Utc::now().timestamp_millis() + 5000;
    let signature = sign_hmac_sha256(secret, &format!("GET/realtime{expires}"));

    let op = TradeOp {
        req_id: "auth".to_string(),
        header: Default::default(),
        op: "auth".to_string(),
        args: vec![api_key.to_string(), expires.to_string(), signature],
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
            order = order_rx.recv() => {
                match order {
                    Some(order) => {
                        let op = TradeOp {
                            req_id: order.bybit_order.order_link_id.clone(),
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
                        write.send(Message::Text(s)).await?;
                    }
                    None => {
                        break;
                    }
                }
            }
            message = read.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        if let Err(error) = handle_trade_stream(
                            &text,
                            &ev_tx,
                            &order_man
                        ).await {
                            error!(?error, %text, "Couldn't parse TradeStreamMsg.");
                        };
                    }
                    Some(Ok(Message::Binary(_))) => {}
                    Some(Ok(Message::Ping(_))) => {
                        write.send(Message::Pong(Vec::new())).await?;
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Close(close_frame))) => {
                        info!(?close_frame, "close");
                        break;
                    }
                    Some(Ok(Message::Frame(_))) => {}
                    Some(Err(e)) => {
                        return Err(anyhow::Error::from(e));
                    }
                    None => {
                        break;
                    }
                }
            }
        }
    }
    Ok(())
}

async fn handle_trade_stream(
    text: &str,
    ev_tx: &Sender<LiveEvent>,
    order_man: &WrappedOrderManager,
) -> Result<(), anyhow::Error> {
    let stream = serde_json::from_str::<TradeStreamMsg>(text)?;
    if stream.op == "auth" {
        if stream.ret_code != 0 {
            ev_tx
                .send(LiveEvent::Error(Error::with(
                    ErrorKind::CriticalConnectionError,
                    BybitError::AuthError(stream.ret_code, stream.ret_msg.clone()),
                )))
                .unwrap();
            return Err(anyhow::Error::from(BybitError::AuthError(
                stream.ret_code,
                stream.ret_msg,
            )));
        }
    } else if stream.op == "order.create" {
        let req_id = stream.req_id.ok_or(OrderManagerError::NoReqId)?;
        if stream.ret_code != 0 {
            /*
            10404: 1. op type is not found; 2. category is not correct/supported
            10429: System level frequency protection
            20006: reqId is duplicated
            10016: 1. internal server error; 2. Service is restarting
            10019: ws trade service is restarting, do not accept new request, but the request in the process is not affected. You can build new connection to be routed to normal service
             */
            let mut order_man_ = order_man.lock().unwrap();
            let (asset_no, mut order) = order_man_.update_submit_fail(&req_id)?;
            ev_tx
                .send(LiveEvent::Order(OrderResponse { asset_no, order }))
                .unwrap();
            ev_tx
                .send(LiveEvent::Error(Error::with(
                    ErrorKind::OrderError,
                    BybitError::OrderError(stream.ret_code, stream.ret_msg.clone()),
                )))
                .unwrap();
        }
    } else if stream.op == "order.cancel" {
        let req_id = stream.req_id.ok_or(OrderManagerError::NoReqId)?;
        if stream.ret_code != 0 {
            /*
            10404: 1. op type is not found; 2. category is not correct/supported
            10429: System level frequency protection
            20006: reqId is duplicated
            10016: 1. internal server error; 2. Service is restarting
            10019: ws trade service is restarting, do not accept new request, but the request in the process is not affected. You can build new connection to be routed to normal service
             */
            let mut order_man_ = order_man.lock().unwrap();
            let (asset_no, order) = order_man_.update_cancel_fail(&req_id)?;
            ev_tx
                .send(LiveEvent::Order(OrderResponse { asset_no, order }))
                .unwrap();
            ev_tx
                .send(LiveEvent::Error(Error::with(
                    ErrorKind::OrderError,
                    BybitError::OrderError(stream.ret_code, stream.ret_msg.clone()),
                )))
                .unwrap();
        }
    } else {
        info!(?stream, "trade stream");
    }
    Ok(())
}
