use std::time::{Duration, Instant};

use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use hftbacktest::prelude::{
    Event,
    LOCAL_ASK_DEPTH_BBO_EVENT,
    LOCAL_ASK_DEPTH_EVENT,
    LOCAL_BID_DEPTH_BBO_EVENT,
    LOCAL_BID_DEPTH_EVENT,
    LOCAL_BUY_TRADE_EVENT,
    LOCAL_DEPTH_CLEAR_EVENT,
    LOCAL_SELL_TRADE_EVENT,
    LiveEvent,
    Side,
};
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
use tracing::{debug, error, warn};

use crate::{
    bybit::{
        BybitError,
        msg,
        msg::{Op, OrderBook, PublicStreamMsg},
    },
    connector::PublishEvent,
    utils::parse_depth,
};

pub struct PublicStream {
    ev_tx: UnboundedSender<PublishEvent>,
    symbol_rx: Receiver<String>,
}

impl PublicStream {
    pub fn new(ev_tx: UnboundedSender<PublishEvent>, symbol_rx: Receiver<String>) -> Self {
        Self { ev_tx, symbol_rx }
    }

    async fn handle_public_stream(&self, text: &str) -> Result<(), BybitError> {
        let stream = serde_json::from_str::<PublicStreamMsg>(text)?;
        match stream {
            PublicStreamMsg::Op(resp) => {
                debug!(?resp, "Op");
            }
            PublicStreamMsg::Topic(stream) => {
                // Check if this is a snapshot message (Bybit sends "snapshot" for initial data, "delta" for updates)
                let is_snapshot = stream.msg_type.as_deref() == Some("snapshot");

                if stream.topic.starts_with("orderbook.1") {
                    let data: OrderBook = serde_json::from_value(stream.data)?;
                    let (bids, asks) = parse_depth(data.bids, data.asks)?;

                    // Send clear event before snapshot data to properly initialize orderbook
                    if is_snapshot {
                        self.ev_tx
                            .send(PublishEvent::LiveEvent(LiveEvent::Feed {
                                symbol: data.symbol.clone(),
                                event: Event {
                                    ev: LOCAL_DEPTH_CLEAR_EVENT,
                                    exch_ts: stream.cts.unwrap_or(stream.ts) * 1_000_000,
                                    local_ts: Utc::now().timestamp_nanos_opt().unwrap(),
                                    order_id: 0,
                                    px: 0.0,
                                    qty: 0.0,
                                    ival: 0,
                                    fval: 0.0,
                                },
                            }))
                            .unwrap();
                    }

                    for (px, qty) in bids {
                        self.ev_tx
                            .send(PublishEvent::LiveEvent(LiveEvent::Feed {
                                symbol: data.symbol.clone(),
                                event: Event {
                                    ev: LOCAL_BID_DEPTH_BBO_EVENT,
                                    exch_ts: stream.cts.unwrap() * 1_000_000,
                                    local_ts: Utc::now().timestamp_nanos_opt().unwrap(),
                                    order_id: 0,
                                    px,
                                    qty,
                                    ival: 0,
                                    fval: 0.0,
                                },
                            }))
                            .unwrap();
                    }

                    for (px, qty) in asks {
                        self.ev_tx
                            .send(PublishEvent::LiveEvent(LiveEvent::Feed {
                                symbol: data.symbol.clone(),
                                event: Event {
                                    ev: LOCAL_ASK_DEPTH_BBO_EVENT,
                                    exch_ts: stream.cts.unwrap() * 1_000_000,
                                    local_ts: Utc::now().timestamp_nanos_opt().unwrap(),
                                    order_id: 0,
                                    px,
                                    qty,
                                    ival: 0,
                                    fval: 0.0,
                                },
                            }))
                            .unwrap();
                    }
                } else if stream.topic.starts_with("orderbook") {
                    let data: OrderBook = serde_json::from_value(stream.data)?;
                    let (bids, asks) = parse_depth(data.bids, data.asks)?;

                    // Send clear event before snapshot data to properly initialize orderbook
                    if is_snapshot {
                        self.ev_tx
                            .send(PublishEvent::LiveEvent(LiveEvent::Feed {
                                symbol: data.symbol.clone(),
                                event: Event {
                                    ev: LOCAL_DEPTH_CLEAR_EVENT,
                                    exch_ts: stream.cts.unwrap_or(stream.ts) * 1_000_000,
                                    local_ts: Utc::now().timestamp_nanos_opt().unwrap(),
                                    order_id: 0,
                                    px: 0.0,
                                    qty: 0.0,
                                    ival: 0,
                                    fval: 0.0,
                                },
                            }))
                            .unwrap();
                    }

                    for (px, qty) in bids {
                        self.ev_tx
                            .send(PublishEvent::LiveEvent(LiveEvent::Feed {
                                symbol: data.symbol.clone(),
                                event: Event {
                                    ev: LOCAL_BID_DEPTH_EVENT,
                                    exch_ts: stream.cts.unwrap() * 1_000_000,
                                    local_ts: Utc::now().timestamp_nanos_opt().unwrap(),
                                    order_id: 0,
                                    px,
                                    qty,
                                    ival: 0,
                                    fval: 0.0,
                                },
                            }))
                            .unwrap();
                    }

                    for (px, qty) in asks {
                        self.ev_tx
                            .send(PublishEvent::LiveEvent(LiveEvent::Feed {
                                symbol: data.symbol.clone(),
                                event: Event {
                                    ev: LOCAL_ASK_DEPTH_EVENT,
                                    exch_ts: stream.cts.unwrap() * 1_000_000,
                                    local_ts: Utc::now().timestamp_nanos_opt().unwrap(),
                                    order_id: 0,
                                    px,
                                    qty,
                                    ival: 0,
                                    fval: 0.0,
                                },
                            }))
                            .unwrap();
                    }
                } else if stream.topic.starts_with("publicTrade") {
                    let data: Vec<msg::Trade> = serde_json::from_value(stream.data)?;
                    for item in data {
                        self.ev_tx
                            .send(PublishEvent::LiveEvent(LiveEvent::Feed {
                                symbol: item.symbol.clone(),
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
        let mut ping_interval = time::interval(Duration::from_secs(15));
        let mut health_checker = time::interval(Duration::from_secs(10));

        // Track message activity for health detection
        let mut last_message_time = Instant::now();

        // Inactivity threshold: 90 seconds (public streams should be very active)
        const MESSAGE_INACTIVITY_TIMEOUT: Duration = Duration::from_secs(90);

        loop {
            select! {
                _ = ping_interval.tick() => {
                    let op = Op {
                        req_id: "ping".to_string(),
                        op: "ping".to_string(),
                        args: vec![]
                    };
                    let s = serde_json::to_string(&op).unwrap();
                    write.send(Message::Text(s.into())).await?;
                }
                _ = health_checker.tick() => {
                    // Check message activity - public stream should be very active
                    if last_message_time.elapsed() > MESSAGE_INACTIVITY_TIMEOUT {
                        warn!(
                            elapsed_secs = last_message_time.elapsed().as_secs(),
                            "No message activity on public stream - connection may be dead."
                        );
                        return Err(BybitError::ConnectionInterrupted);
                    }
                }
                msg = self.symbol_rx.recv() => match msg {
                    Ok(symbol) => {
                        // Subscribes to the orderbook.1, orderbook.50 and orderbook.200 topics to
                        // obtain a wider range of depth and the most frequent updates.
                        // The different updates are handled by data fusion.
                        // Please see: `<https://bybit-exchange.github.io/docs/v5/websocket/public/orderbook>`
                        let args = vec![
                            format!("orderbook.1.{symbol}"),
                            format!("orderbook.50.{symbol}"),
                            format!("orderbook.200.{symbol}"),
                            format!("orderbook.1000.{symbol}"),
                            format!("publicTrade.{symbol}")
                        ];
                        let op = Op {
                            req_id: "subscribe".to_string(),
                            op: "subscribe".to_string(),
                            args,
                        };
                        let s = serde_json::to_string(&op).unwrap();
                        write.send(Message::Text(s.into())).await?;
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
                            last_message_time = Instant::now();
                            if let Err(error) = self.handle_public_stream(&text).await {
                                error!(?error, %text, "Couldn't handle PublicStreamMsg.");
                            }
                        }
                        Some(Ok(Message::Ping(_))) => {
                            last_message_time = Instant::now();
                            write.send(Message::Pong(Bytes::default())).await?;
                        }
                        Some(Ok(Message::Pong(_))) => {
                            last_message_time = Instant::now();
                        }
                        Some(Ok(Message::Close(close_frame))) => {
                            return Err(BybitError::ConnectionAbort(
                                close_frame
                                    .map(|f| f.to_string())
                                    .unwrap_or(String::new())
                            ));
                        }
                        Some(Ok(Message::Binary(_)))
                        | Some(Ok(Message::Frame(_))) => {
                            last_message_time = Instant::now();
                        }
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
