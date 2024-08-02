use std::{collections::HashMap, sync::mpsc::Sender, time::Duration};

use chrono::Utc;
use futures_util::{stream::SplitSink, SinkExt, StreamExt};
use tokio::{net::TcpStream, select, sync::mpsc::UnboundedReceiver, time};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
    MaybeTlsStream,
    WebSocketStream,
};
use tracing::{debug, error, info, warn};

use crate::{
    connector::{
        bybit::{
            msg,
            msg::{
                Op,
                Order as BybitOrder,
                OrderBook,
                PrivateStreamMsg,
                PrivateStreamTopicMsg,
                PublicStreamMsg,
                TradeOp,
                TradeStreamMsg,
            },
            ordermanager::{HandleError, OrderManagerWrapper},
            BybitError,
        },
        util::{gen_random_string, sign_hmac_sha256},
    },
    live::Asset,
    types::{
        ErrorKind,
        Event,
        LiveError,
        LiveEvent,
        Side,
        LOCAL_ASK_DEPTH_BBO_EVENT,
        LOCAL_ASK_DEPTH_EVENT,
        LOCAL_BID_DEPTH_BBO_EVENT,
        LOCAL_BID_DEPTH_EVENT,
        LOCAL_BUY_TRADE_EVENT,
        LOCAL_SELL_TRADE_EVENT,
    },
};

pub struct OrderOp {
    pub op: String,
    pub bybit_order: BybitOrder,
    pub tx: Sender<LiveEvent>,
}

fn parse_depth(
    bids: Vec<(String, String)>,
    asks: Vec<(String, String)>,
) -> Result<(Vec<(f64, f64)>, Vec<(f64, f64)>), HandleError> {
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

fn parse_px_qty_tup(px: String, qty: String) -> Result<(f64, f64), HandleError> {
    Ok((px.parse()?, qty.parse()?))
}

async fn handle_public_stream(
    text: &str,
    ev_tx: &Sender<LiveEvent>,
    assets: &HashMap<String, Asset>,
) -> Result<(), HandleError> {
    let stream = serde_json::from_str::<PublicStreamMsg>(&text)?;
    match stream {
        PublicStreamMsg::Op(resp) => {
            info!(?resp, "Op");
        }
        PublicStreamMsg::Topic(stream) => {
            if stream.topic.starts_with("orderbook.1") {
                let data: OrderBook = serde_json::from_value(stream.data)?;
                let (bids, asks) = parse_depth(data.bids, data.asks)?;
                let asset = assets.get(&data.symbol).ok_or(HandleError::AssetNotFound)?;
                assert_eq!(bids.len(), 1);
                let mut bid_events: Vec<_> = bids
                    .iter()
                    .map(|&(px, qty)| Event {
                        ev: LOCAL_BID_DEPTH_BBO_EVENT,
                        exch_ts: stream.cts.unwrap() * 1_000_000,
                        local_ts: Utc::now().timestamp_nanos_opt().unwrap(),
                        order_id: 0,
                        px,
                        qty,
                        ival: 0,
                        fval: 0.0,
                    })
                    .collect();
                assert_eq!(asks.len(), 1);
                let mut ask_events: Vec<_> = asks
                    .iter()
                    .map(|&(px, qty)| Event {
                        ev: LOCAL_ASK_DEPTH_BBO_EVENT,
                        exch_ts: stream.cts.unwrap() * 1_000_000,
                        local_ts: Utc::now().timestamp_nanos_opt().unwrap(),
                        order_id: 0,
                        px,
                        qty,
                        ival: 0,
                        fval: 0.0,
                    })
                    .collect();
                let mut events = Vec::new();
                events.append(&mut bid_events);
                events.append(&mut ask_events);
                ev_tx
                    .send(LiveEvent::FeedBatch {
                        asset_no: asset.asset_no,
                        events,
                    })
                    .unwrap();
            } else if stream.topic.starts_with("orderbook") {
                let data: OrderBook = serde_json::from_value(stream.data)?;
                let (bids, asks) = parse_depth(data.bids, data.asks)?;
                let asset = assets.get(&data.symbol).ok_or(HandleError::AssetNotFound)?;
                let mut bid_events: Vec<_> = bids
                    .iter()
                    .map(|&(px, qty)| Event {
                        ev: LOCAL_BID_DEPTH_EVENT,
                        exch_ts: stream.cts.unwrap() * 1_000_000,
                        local_ts: Utc::now().timestamp_nanos_opt().unwrap(),
                        order_id: 0,
                        px,
                        qty,
                        ival: 0,
                        fval: 0.0,
                    })
                    .collect();
                let mut ask_events: Vec<_> = asks
                    .iter()
                    .map(|&(px, qty)| Event {
                        ev: LOCAL_ASK_DEPTH_EVENT,
                        exch_ts: stream.cts.unwrap() * 1_000_000,
                        local_ts: Utc::now().timestamp_nanos_opt().unwrap(),
                        order_id: 0,
                        px,
                        qty,
                        ival: 0,
                        fval: 0.0,
                    })
                    .collect();
                let mut events = Vec::new();
                events.append(&mut bid_events);
                events.append(&mut ask_events);
                ev_tx
                    .send(LiveEvent::FeedBatch {
                        asset_no: asset.asset_no,
                        events,
                    })
                    .unwrap();
            } else if stream.topic.starts_with("publicTrade") {
                let data: Vec<msg::Trade> = serde_json::from_value(stream.data)?;
                for item in data {
                    let asset_info = assets.get(&item.symbol).ok_or(HandleError::AssetNotFound)?;
                    ev_tx
                        .send(LiveEvent::FeedBatch {
                            asset_no: asset_info.asset_no,
                            events: vec![Event {
                                ev: {
                                    if item.side == Side::Sell {
                                        LOCAL_SELL_TRADE_EVENT
                                    } else {
                                        LOCAL_BUY_TRADE_EVENT
                                    }
                                },
                                exch_ts: item.ts * 1_000_000,
                                local_ts: Utc::now().timestamp_nanos_opt().unwrap(),
                                order_id: 0,
                                px: item.trade_price,
                                qty: item.trade_size,
                                ival: 0,
                                fval: 0.0,
                            }],
                        })
                        .unwrap();
                }
            }
        }
    }
    Ok(())
}

pub async fn connect_public(
    url: &str,
    ev_tx: Sender<LiveEvent>,
    assets: HashMap<String, Asset>,
    topics: Vec<String>,
) -> Result<(), HandleError> {
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
                        if let Err(error) = handle_public_stream(&text, &ev_tx, &assets).await {
                            error!(?error, %text, "Couldn't handle PublicStreamMsg.");
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
                        return Err(HandleError::from(e));
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

async fn handle_private_stream(
    text: &str,
    write: &mut SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
    assets: &HashMap<String, Asset>,
    ev_tx: &Sender<LiveEvent>,
    order_man: &OrderManagerWrapper,
) -> Result<(), HandleError> {
    let stream = serde_json::from_str::<PrivateStreamMsg>(&text)?;
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
                let asset = assets.get(&item.symbol).ok_or(HandleError::AssetNotFound)?;
                ev_tx
                    .send(LiveEvent::Position {
                        asset_no: asset.asset_no,
                        qty: item.size,
                    })
                    .unwrap();
            }
        }
        PrivateStreamMsg::Topic(PrivateStreamTopicMsg::Execution(data)) => {
            debug!(?data, "Execution");
            let mut order_man_ = order_man.lock().unwrap();
            for item in &data.data {
                match order_man_.update_execution(&item) {
                    Ok((asset_no, order)) => {
                        ev_tx.send(LiveEvent::Order { asset_no, order }).unwrap();
                    }
                    Err(error) => {
                        error!(?error, ?data, "Couldn't update the execution data");
                    }
                }
            }
        }
        PrivateStreamMsg::Topic(PrivateStreamTopicMsg::FastExecution(data)) => {
            debug!(?data, "FastExecution");
            let mut order_man_ = order_man.lock().unwrap();
            for item in &data.data {
                match order_man_.update_fast_execution(&item) {
                    Ok((asset_no, order)) => {
                        ev_tx.send(LiveEvent::Order { asset_no, order }).unwrap();
                    }
                    Err(error) => {
                        error!(?error, ?data, "Couldn't update the fast execution data");
                    }
                }
            }
        }
        PrivateStreamMsg::Topic(PrivateStreamTopicMsg::Order(data)) => {
            debug!(?data, "Order");
            for item in &data.data {
                let mut order_man_ = order_man.lock().unwrap();
                match order_man_.update_order(&item) {
                    Ok((asset_no, order)) => {
                        ev_tx.send(LiveEvent::Order { asset_no, order }).unwrap();
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

pub async fn connect_private(
    url: &str,
    api_key: &str,
    secret: &str,
    ev_tx: Sender<LiveEvent>,
    assets: HashMap<String, Asset>,
    order_man: OrderManagerWrapper,
) -> Result<(), HandleError> {
    let mut request = url.into_client_request()?;
    let _ = request.headers_mut();

    let (ws_stream, _) = connect_async(request).await?;
    let (mut write, mut read) = ws_stream.split();
    let mut interval = time::interval(Duration::from_secs(10));

    let expires = Utc::now().timestamp_millis() + 5000;
    let signature = sign_hmac_sha256(secret, &format!("GET/realtime{expires}"));

    let op = Op {
        req_id: "auth".to_string(),
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
            message = read.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        match handle_private_stream(
                            &text,
                            &mut write,
                            &assets,
                            &ev_tx,
                            &order_man
                        ).await {
                            Ok(_) => {}
                            Err(HandleError::PrefixUnmatched) => {
                                warn!(%text, "PrefixUnmatched");
                            }
                            Err(error) => {
                                error!(%text, ?error, "Couldn't properly handle PrivateStreamMsg");
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
                        return Err(HandleError::from(e));
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
    order_rx: &mut UnboundedReceiver<OrderOp>,
    order_man: OrderManagerWrapper,
) -> Result<(), HandleError> {
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
                        let req_id = {
                            let rand_id = gen_random_string(8);
                            format!(
                                "{}/{}",
                                order.bybit_order.order_link_id.clone(),
                                rand_id,
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
                        match handle_trade_stream(
                            &text,
                            &ev_tx,
                            &order_man
                        ).await {
                            Ok(_) => {}
                            Err(error) => {
                               error!(?error, %text, "Couldn't properly handle TradeStreamMsg.");
                            }
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
                        return Err(HandleError::from(e));
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
    order_man: &OrderManagerWrapper,
) -> Result<(), anyhow::Error> {
    let stream = serde_json::from_str::<TradeStreamMsg>(text)?;
    if stream.op == "auth" {
        if stream.ret_code != 0 {
            let err = BybitError::AuthError {
                code: stream.ret_code,
                msg: stream.ret_msg.clone(),
            };
            ev_tx
                .send(LiveEvent::Error(LiveError::with(
                    ErrorKind::CriticalConnectionError,
                    err.clone().into(),
                )))
                .unwrap();
            return Err(anyhow::Error::from(err));
        }
    } else if stream.op == "order.create" {
        let req_id = stream.req_id.ok_or(HandleError::InvalidReqId)?;
        if stream.ret_code != 0 {
            /*
            10404: 1. op type is not found; 2. category is not correct/supported
            10429: System level frequency protection
            20006: reqId is duplicated
            10016: 1. internal server error; 2. Service is restarting
            10019: ws trade service is restarting, do not accept new request, but the request in the process is not affected. You can build new connection to be routed to normal service
             */
            let mut order_man_ = order_man.lock().unwrap();
            let order_link_id = req_id.split('/').next().ok_or(HandleError::InvalidReqId)?;
            let (asset_no, order) = order_man_.update_submit_fail(order_link_id)?;
            ev_tx.send(LiveEvent::Order { asset_no, order }).unwrap();
            ev_tx
                .send(LiveEvent::Error(LiveError::with(
                    ErrorKind::OrderError,
                    BybitError::OrderError {
                        code: stream.ret_code,
                        msg: stream.ret_msg.clone(),
                    }
                    .into(),
                )))
                .unwrap();
        }
    } else if stream.op == "order.cancel" {
        let req_id = stream.req_id.ok_or(HandleError::InvalidReqId)?;
        if stream.ret_code != 0 {
            /*
            10404: 1. op type is not found; 2. category is not correct/supported
            10429: System level frequency protection
            20006: reqId is duplicated
            10016: 1. internal server error; 2. Service is restarting
            10019: ws trade service is restarting, do not accept new request, but the request in the process is not affected. You can build new connection to be routed to normal service
             */
            let mut order_man_ = order_man.lock().unwrap();
            let order_link_id = req_id.split('/').next().ok_or(HandleError::InvalidReqId)?;
            let (asset_no, order) = order_man_.update_cancel_fail(order_link_id)?;
            ev_tx.send(LiveEvent::Order { asset_no, order }).unwrap();
            ev_tx
                .send(LiveEvent::Error(LiveError::with(
                    ErrorKind::OrderError,
                    BybitError::OrderError {
                        code: stream.ret_code,
                        msg: stream.ret_msg.clone(),
                    }
                    .into(),
                )))
                .unwrap();
        }
    } else {
        info!(?stream, "trade stream");
    }
    Ok(())
}
