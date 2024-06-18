/// Binance Futures Websocket module
/// https://binance-docs.github.io/apidocs/futures/en/
use std::{collections::HashMap, sync::mpsc::Sender, time::Duration};

use anyhow::Error;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use tokio::{select, sync::mpsc::unbounded_channel, time};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
};
use tracing::{error, info};

use super::{
    msg::stream::{Data, Stream},
    rest::BinanceFuturesClient,
    BinanceFuturesError,
    WrappedOrderManager,
};
use crate::{
    connector::binancefutures::{
        msg::{rest, stream},
        ordermanager::OrderManager,
    },
    live::Asset,
    types::{
        Event,
        LiveEvent,
        Order,
        OrderResponse,
        Position,
        Status,
        LOCAL_ASK_DEPTH_EVENT,
        LOCAL_BID_DEPTH_EVENT,
        LOCAL_BUY_TRADE_EVENT,
        LOCAL_SELL_TRADE_EVENT,
    },
};

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

pub enum DepthManageMode {
    WaitUntilGapFill,
    GapFillOnTheFly,
    NaturalRefresh,
}

pub async fn connect(
    url: &str,
    ev_tx: Sender<LiveEvent>,
    assets: HashMap<String, Asset>,
    prefix: &str,
    orders: WrappedOrderManager,
    client: BinanceFuturesClient,
) -> Result<(), anyhow::Error> {
    let mut request = url.into_client_request()?;
    let _ = request.headers_mut();

    let depth_mode = DepthManageMode::NaturalRefresh;
    let mut pending_depth_messages: HashMap<String, Vec<stream::Depth>> = HashMap::new();
    let mut prev_u: HashMap<String, i64> = HashMap::new();

    let (ws_stream, _) = connect_async(request).await?;
    let (mut write, mut read) = ws_stream.split();
    let mut interval = time::interval(Duration::from_secs(60 * 30));
    let (rest_tx, mut rest_rx) = unbounded_channel::<(String, rest::Depth)>();
    loop {
        select! {
            _ = interval.tick() => {
                let client_ = client.clone();
                tokio::spawn(async move {
                    if let Err(error) = client_.keepalive_user_data_stream().await {
                        error!(?error, "Failed keepalive user data stream.");
                    }
                });
            }
            Some((symbol, data)) = rest_rx.recv() => {
                // Processes the REST depth.
                match parse_depth(data.bids, data.asks) {
                    Ok((bids, asks)) => {
                        let asset = assets
                            .get(&symbol)
                            .ok_or(BinanceFuturesError::AssetNotFound)?;
                        let mut bid_events: Vec<_> = bids
                            .iter()
                            .map(|&(px, qty)| Event {
                                    ev: LOCAL_BID_DEPTH_EVENT,
                                    exch_ts: data.transaction_time * 1_000_000,
                                    local_ts: Utc::now().timestamp_nanos_opt().unwrap(),
                                    px,
                                    qty,
                                })
                            .collect();
                        let mut ask_events: Vec<_> = asks
                            .iter()
                            .map(|&(px, qty)| Event {
                                    ev: LOCAL_ASK_DEPTH_EVENT,
                                    exch_ts: data.transaction_time * 1_000_000,
                                    local_ts: Utc::now().timestamp_nanos_opt().unwrap(),
                                    px,
                                    qty,
                                })
                            .collect();
                        let mut events = Vec::new();
                        events.append(&mut bid_events);
                        events.append(&mut ask_events);
                        ev_tx.send(
                            LiveEvent::L2Feed(
                                asset.asset_no,
                                events
                            )
                        ).unwrap();
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
            message = read.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        let stream = match serde_json::from_str::<Stream>(&text) {
                            Ok(stream) => stream,
                            Err(error) => {
                                error!(?error, %text, "Couldn't parse Stream.");
                                continue;
                            }
                        };
                        match stream.data {
                            Data::DepthUpdate(data) => {
                                let mut prev_u_val = prev_u.get_mut(&data.symbol);
                                if prev_u_val.is_none()
                                    /* fixme: || data.prev_update_id != **prev_u_val.as_ref().unwrap()*/
                                {
                                    // if !pending_depth_messages.contains_key(&data.symbol) {
                                        let client_ = client.clone();
                                        let symbol = data.symbol.clone();
                                        let rest_tx_ = rest_tx.clone();
                                        tokio::spawn(async move {
                                            let resp = client_
                                                .get_depth(&symbol)
                                                .await;
                                            match resp {
                                                Ok(depth) => {
                                                    rest_tx_.send((symbol, depth)).unwrap();
                                                }
                                                Err(error) => {
                                                    error!(
                                                        ?error,
                                                        %symbol,
                                                        "Couldn't get the market depth via REST."
                                                    )
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
                                *prev_u.entry(data.symbol.clone()).or_insert(data.last_update_id) = data.last_update_id;

                                match parse_depth(data.bids, data.asks) {
                                    Ok((bids, asks)) => {
                                        let asset_info = assets
                                            .get(&data.symbol)
                                            .ok_or(BinanceFuturesError::AssetNotFound)?;
                                        let mut bid_events: Vec<_> = bids
                                            .iter()
                                            .map(|&(px, qty)| Event {
                                                    ev: LOCAL_BID_DEPTH_EVENT,
                                                    exch_ts: data.transaction_time * 1_000_000,
                                                    local_ts: Utc::now().timestamp_nanos_opt().unwrap(),
                                                    px,
                                                    qty,
                                                })
                                            .collect();
                                        let mut ask_events: Vec<_> = asks
                                            .iter()
                                            .map(|&(px, qty)| Event {
                                                    ev: LOCAL_ASK_DEPTH_EVENT,
                                                    exch_ts: data.transaction_time * 1_000_000,
                                                    local_ts: Utc::now().timestamp_nanos_opt().unwrap(),
                                                    px,
                                                    qty,
                                                })
                                            .collect();
                                        let mut events = Vec::new();
                                        events.append(&mut bid_events);
                                        events.append(&mut ask_events);
                                        ev_tx.send(
                                            LiveEvent::L2Feed(
                                                asset_info.asset_no,
                                                events
                                            )
                                        ).unwrap();
                                    }
                                    Err(error) => {
                                        error!(?error, "Couldn't parse DepthUpdate stream.");
                                    }
                                }
                            }
                            Data::Trade(data) => {
                                match parse_px_qty_tup(data.price, data.qty) {
                                    Ok((px, qty)) => {
                                        let asset_info = assets
                                            .get(&data.symbol)
                                        .ok_or(BinanceFuturesError::AssetNotFound)?;
                                        ev_tx.send(
                                            LiveEvent::L2Feed(
                                                asset_info.asset_no,
                                                vec![Event {
                                                    ev: {
                                                        if data.is_the_buyer_the_market_maker {
                                                            LOCAL_SELL_TRADE_EVENT
                                                        } else {
                                                            LOCAL_BUY_TRADE_EVENT
                                                        }
                                                    },
                                                    exch_ts: data.transaction_time * 1_000_000,
                                                    local_ts: Utc::now().timestamp_nanos_opt().unwrap(),
                                                    px,
                                                    qty,
                                                }]
                                            )
                                        ).unwrap();
                                    }
                                    Err(e) => {
                                        error!(error = ?e, "Couldn't parse trade stream.");
                                    }
                                }
                            }
                            Data::ListenKeyExpired(_) => {
                                error!("Listen key is expired.");
                                // fixme: it should return an error.
                                break;
                            }
                            Data::AccountUpdate(data) => {
                                for position in data.account.position {
                                    if let Some(asset_info) = assets.get(&position.symbol) {
                                        ev_tx.send(
                                            LiveEvent::Position(
                                                Position {
                                                    asset_no: asset_info.asset_no,
                                                    symbol: position.symbol,
                                                    qty: position.position_amount
                                                }
                                            )
                                        ).unwrap();
                                    }
                                }
                            }
                            Data::OrderTradeUpdate(data) => {
                                if let Some(asset_info) = assets.get(&data.order.symbol) {
                                    if let Some(order_id) = OrderManager::parse_client_order_id(&data.order.client_order_id, &prefix) {
                                        let order = Order {
                                            qty: data.order.original_qty,
                                            leaves_qty: data.order.original_qty - data.order.order_filled_accumulated_qty,
                                            price_tick: (data.order.original_price / asset_info.tick_size).round() as i32,
                                            tick_size: asset_info.tick_size,
                                            side: data.order.side,
                                            time_in_force: data.order.time_in_force,
                                            exch_timestamp: data.transaction_time * 1_000_000,
                                            status: data.order.order_status,
                                            local_timestamp: 0,
                                            req: Status::None,
                                            exec_price_tick: (data.order.last_filled_price / asset_info.tick_size).round() as i32,
                                            exec_qty: data.order.order_last_filled_qty,
                                            order_id,
                                            order_type: data.order.order_type,
                                            // Invalid information
                                            q: Box::new(()),
                                            maker: false
                                        };

                                        let order = orders
                                            .lock()
                                            .unwrap()
                                            .update_from_ws(
                                                asset_info.asset_no,
                                                data.order.client_order_id,
                                                order
                                            );
                                        if let Some(order) = order {
                                            ev_tx.send(
                                                LiveEvent::Order(
                                                    OrderResponse {
                                                        asset_no: asset_info.asset_no,
                                                        order
                                                    }
                                                )
                                            ).unwrap();
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Binary(_))) => {}
                    Some(Ok(Message::Ping(_))) => {
                        orders.lock()
                            .unwrap()
                            .gc();
                        write.send(Message::Pong(Vec::new())).await?;
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Close(close_frame))) => {
                        info!(?close_frame, "close");
                        break;
                    }
                    Some(Ok(Message::Frame(_))) => {}
                    Some(Err(e)) => {
                        return Err(Error::from(e));
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
