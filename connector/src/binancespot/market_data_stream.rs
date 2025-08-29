use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use hftbacktest::{live::ipc::TO_ALL, prelude::*};
use tokio::{
    select,
    sync::{
        broadcast::{Receiver, error::RecvError},
        mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
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
        msg::{
            rest,
            stream,
            stream::{MarketEventStream, MarketStream},
        },
        rest::BinanceSpotClient,
    },
    connector::PublishEvent,
    utils::{generate_rand_string, parse_depth, parse_px_qty_tup},
};

pub struct MarketDataStream {
    client: BinanceSpotClient,
    ev_tx: UnboundedSender<PublishEvent>,
    symbol_rx: Receiver<String>,
    pending_depth_messages: HashMap<String, Vec<stream::Depth>>,
    prev_u: HashMap<String, i64>,
    rest_tx: UnboundedSender<(String, rest::Depth)>,
    rest_rx: UnboundedReceiver<(String, rest::Depth)>,
}

impl MarketDataStream {
    pub fn new(
        client: BinanceSpotClient,
        ev_tx: UnboundedSender<PublishEvent>,
        symbol_rx: Receiver<String>,
    ) -> Self {
        let (rest_tx, rest_rx) = unbounded_channel::<(String, rest::Depth)>();
        Self {
            client,
            ev_tx,
            symbol_rx,
            pending_depth_messages: Default::default(),
            prev_u: Default::default(),
            rest_tx,
            rest_rx,
        }
    }

    fn process_message(&mut self, stream: MarketEventStream) {
        match stream {
            MarketEventStream::DepthUpdate(data) => {
                let prev_u_val = self.prev_u.get_mut(&data.symbol);
                if prev_u_val.is_none()
                /* fixme: || data.prev_update_id != **prev_u_val.as_ref().unwrap()*/
                {
                    // if !pending_depth_messages.contains_key(&data.symbol) {
                    let client_ = self.client.clone();
                    let symbol = data.symbol.clone();
                    let rest_tx = self.rest_tx.clone();
                    tokio::spawn(async move {
                        let resp = client_.get_depth(&symbol).await;
                        match resp {
                            Ok(depth) => {
                                rest_tx.send((symbol, depth)).unwrap();
                            }
                            Err(error) => {
                                error!(
                                    ?error,
                                    %symbol,
                                    "Couldn't get the market depth via REST."
                                );
                            }
                        }
                    });
                    // }
                    // pending_depth_messages
                    //     .entry(data.symbol.clone())
                    //     .or_insert(Vec::new())
                    //     .push(data);
                    // continue;
                }
                // *prev_u_val.unwrap() = data.last_update_id;
                // fixme: currently supports natural refresh only.
                *self
                    .prev_u
                    .entry(data.symbol.clone())
                    .or_insert(data.last_update_id) = data.last_update_id;

                match parse_depth(data.bids, data.asks) {
                    Ok((bids, asks)) => {
                        self.ev_tx.send(PublishEvent::BatchStart(TO_ALL)).unwrap();

                        for (px, qty) in bids {
                            self.ev_tx
                                .send(PublishEvent::LiveEvent(LiveEvent::Feed {
                                    symbol: data.symbol.clone(),
                                    event: Event {
                                        ev: LOCAL_BID_DEPTH_EVENT,
                                        exch_ts: data.event_time * 1_000_000,
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
                                        exch_ts: data.event_time * 1_000_000,
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

                        self.ev_tx.send(PublishEvent::BatchEnd(TO_ALL)).unwrap();
                    }
                    Err(error) => {
                        error!(?error, "Couldn't parse DepthUpdate stream.");
                    }
                }
            }
            MarketEventStream::Trade(data) => match parse_px_qty_tup(data.price, data.quantity) {
                Ok((px, qty)) => {
                    if data.ignore {
                        return;
                    }
                    self.ev_tx
                        .send(PublishEvent::LiveEvent(LiveEvent::Feed {
                            symbol: data.symbol,
                            event: Event {
                                ev: {
                                    if data.is_market_maker {
                                        LOCAL_SELL_TRADE_EVENT
                                    } else {
                                        LOCAL_BUY_TRADE_EVENT
                                    }
                                },
                                exch_ts: data.event_time * 1_000_000,
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
                Err(e) => {
                    error!(error = ?e, "Couldn't parse trade stream.");
                }
            },
            _ => unreachable!(),
        }
    }

    fn process_snapshot(&self, symbol: String, data: rest::Depth) {
        match parse_depth(data.bids, data.asks) {
            Ok((bids, asks)) => {
                self.ev_tx.send(PublishEvent::BatchStart(TO_ALL)).unwrap();

                for (px, qty) in bids {
                    self.ev_tx
                        .send(PublishEvent::LiveEvent(LiveEvent::Feed {
                            symbol: symbol.clone(),
                            event: Event {
                                ev: LOCAL_BID_DEPTH_EVENT,
                                exch_ts: data.last_update_id * 1_000_000,
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
                            symbol: symbol.clone(),
                            event: Event {
                                ev: LOCAL_ASK_DEPTH_EVENT,
                                exch_ts: data.last_update_id * 1_000_000,
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

                self.ev_tx.send(PublishEvent::BatchEnd(TO_ALL)).unwrap();
            }
            Err(error) => {
                error!(?error, "Couldn't parse Depth response.");
            }
        }
        // fixme: waits for pending messages without blocking.
        // prev_u.remove(&symbol);
        // let mut new_prev_u: Option<i64> = None;
        // while new_prev_u.is_none() {
        //     if let Some(msg) = pending_depth_messages.get_mut(&symbol) {
        //         for pending_depth in msg.into_iter() {
        //             // https://binance-docs.github.io/apidocs/futures/en/#how-to-manage-a-local-order-book-correctly
        //             // The first processed event should have U <= lastUpdateId AND u >= lastUpdateId
        //             if (
        //                 pending_depth.last_update_id < resp.last_update_id
        //                 || pending_depth.first_update_id > resp.last_update_id
        //             ) && new_prev_u.is_none() {
        //                 continue;
        //             }
        //             if new_prev_u.is_some() && pending_depth.prev_update_id != *new_prev_u.as_ref().unwrap() {
        //                 warn!(%symbol, ?pending_depth, "UpdateId does not match.");
        //             }
        //
        //             // Processes a pending depth message
        //             new_prev_u = Some(pending_depth.last_update_id);
        //             *prev_u.entry(symbol.clone())
        //                 .or_insert(pending_depth.last_update_id) = pending_depth.last_update_id;
        //         }
        //     }
        //     if new_prev_u.is_none() {
        //         // Waits for depth messages.
        //         todo!()
        //     }
        // }
    }

    pub async fn connect(&mut self, url: &str) -> Result<(), BinanceSpotError> {
        let request = url.into_client_request()?;
        let (ws_stream, _) = connect_async(request).await?;
        let (mut write, mut read) = ws_stream.split();
        let mut ping_checker = time::interval(Duration::from_secs(10));
        let mut last_ping = Instant::now();

        loop {
            select! {
                Some((symbol, data)) = self.rest_rx.recv() => {
                    self.process_snapshot(symbol, data);
                }
                _ = ping_checker.tick() => {
                    if last_ping.elapsed() > Duration::from_secs(300) {
                        warn!("Ping timeout.");
                        return Err(BinanceSpotError::ConnectionInterrupted);
                    }
                }
                msg = self.symbol_rx.recv() => match msg {
                    Ok(symbol) => {
                        let id = generate_rand_string(16);
                        write.send(Message::Text(format!(r#"{{
                            "method": "SUBSCRIBE",
                            "params": [
                                "{symbol}@trade",
                                "{symbol}@depth@0ms"
                            ],
                            "id": "{id}"
                        }}"#).into())).await?;
                    }
                    Err(RecvError::Closed) => {
                        return Ok(());
                    }
                    Err(RecvError::Lagged(num)) => {
                        error!("{num} subscription requests were missed.");
                    }
                },
                message = read.next() => match message {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<MarketStream>(&text) {
                            Ok(MarketStream::EventStream(stream)) => {
                                self.process_message(stream);
                            }
                            Ok(MarketStream::Result(result)) => {
                                debug!(?result, "Subscription request response is received.");
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
