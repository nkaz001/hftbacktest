use std::time::Duration;

use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use hftbacktest::prelude::{
    Event,
    LiveEvent,
    Side,
    LOCAL_ASK_DEPTH_BBO_EVENT,
    LOCAL_ASK_DEPTH_EVENT,
    LOCAL_BID_DEPTH_BBO_EVENT,
    LOCAL_BID_DEPTH_EVENT,
    LOCAL_BUY_TRADE_EVENT,
    LOCAL_SELL_TRADE_EVENT,
};
use tokio::{
    select,
    sync::{
        broadcast::{error::RecvError, Receiver},
        mpsc::UnboundedSender,
    },
    time,
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
};
use tracing::{error, info};

use crate::{
    bybit::{
        msg,
        msg::{Op, OrderBook, PublicStreamMsg},
        BybitError,
    },
    connector::PublishMessage,
};

pub type PxQty = (f64, f64);

fn parse_depth(
    bids: Vec<(String, String)>,
    asks: Vec<(String, String)>,
) -> Result<(Vec<PxQty>, Vec<PxQty>), BybitError> {
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

fn parse_px_qty_tup(px: String, qty: String) -> Result<PxQty, BybitError> {
    Ok((px.parse()?, qty.parse()?))
}

pub struct PublicStream {
    ev_tx: UnboundedSender<PublishMessage>,
    symbol_rx: Receiver<String>,
}

impl PublicStream {
    pub fn new(ev_tx: UnboundedSender<PublishMessage>, symbol_rx: Receiver<String>) -> Self {
        Self { ev_tx, symbol_rx }
    }

    async fn handle_public_stream(&self, text: &str) -> Result<(), BybitError> {
        let stream = serde_json::from_str::<PublicStreamMsg>(text)?;
        match stream {
            PublicStreamMsg::Op(resp) => {
                info!(?resp, "Op");
            }
            PublicStreamMsg::Topic(stream) => {
                if stream.topic.starts_with("orderbook.1") {
                    let data: OrderBook = serde_json::from_value(stream.data)?;
                    let (bids, asks) = parse_depth(data.bids, data.asks)?;
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
                    for event in events {
                        self.ev_tx
                            .send(PublishMessage::LiveEvent(LiveEvent::Feed {
                                symbol: data.symbol.clone(),
                                event,
                            }))
                            .unwrap();
                    }
                } else if stream.topic.starts_with("orderbook") {
                    let data: OrderBook = serde_json::from_value(stream.data)?;
                    let (bids, asks) = parse_depth(data.bids, data.asks)?;
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
                    for event in events {
                        self.ev_tx
                            .send(PublishMessage::LiveEvent(LiveEvent::Feed {
                                symbol: data.symbol.clone(),
                                event,
                            }))
                            .unwrap();
                    }
                } else if stream.topic.starts_with("publicTrade") {
                    let data: Vec<msg::Trade> = serde_json::from_value(stream.data)?;
                    for item in data {
                        self.ev_tx
                            .send(PublishMessage::LiveEvent(LiveEvent::Feed {
                                symbol: item.symbol,
                                event: Event {
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
                                },
                            }))
                            .unwrap();
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn connect(&mut self, url: &str) -> Result<(), BybitError> {
        let mut request = url.into_client_request()?;
        let _ = request.headers_mut();

        let (ws_stream, _) = connect_async(request).await?;
        let (mut write, mut read) = ws_stream.split();
        let mut interval = time::interval(Duration::from_secs(15));

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
                msg = self.symbol_rx.recv() => match msg {
                    Ok(symbol) => {
                        let args = vec![
                            format!("orderbook.50.{symbol}"),
                            format!("publicTrade.{symbol}")
                        ];
                        let op = Op {
                            req_id: "subscribe".to_string(),
                            op: "subscribe".to_string(),
                            args,
                        };
                        let s = serde_json::to_string(&op).unwrap();
                        write.send(Message::Text(s)).await?;
                    }
                    Err(RecvError::Closed) => {
                        return Ok(());
                    }
                    Err(RecvError::Lagged(num)) => {
                        error!("{num} subscription requests were missed.");
                    }
                },
                message = read.next() => {
                    match message {
                        Some(Ok(Message::Text(text))) => {
                            if let Err(error) = self.handle_public_stream(&text).await {
                                error!(?error, %text, "Couldn't handle PublicStreamMsg.");
                            }
                        }
                        Some(Ok(Message::Ping(_))) => {
                            write.send(Message::Pong(Vec::new())).await?;
                        }
                        Some(Ok(Message::Close(close_frame))) => {
                            return Err(BybitError::ConnectionAbort(
                                close_frame
                                    .map(|f| f.to_string())
                                    .unwrap_or(String::new())
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
