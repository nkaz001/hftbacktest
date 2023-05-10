use std::{collections::HashMap, sync::mpsc::Sender, time::Duration};

use anyhow::Error;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use tokio::{select, time};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
};
use tracing::{error, info};

use super::{msg::Stream, parse_client_order_id, rest::BinanceFuturesClient, BinanceFuturesError};
use crate::{
    connector::binancefutures::msg::Data,
    live::AssetInfo,
    ty,
    ty::{Depth, Event, Order, OrderResponse, Position, Status, BUY, SELL},
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

pub async fn connect(
    url: &str,
    ev_tx: Sender<Event>,
    assets: HashMap<String, AssetInfo>,
    prefix: &str,
    client: BinanceFuturesClient,
) -> Result<(), anyhow::Error> {
    let mut request = url.into_client_request()?;
    let _ = request.headers_mut();

    let (ws_stream, _) = connect_async(request).await?;
    let (mut write, mut read) = ws_stream.split();
    let mut interval = time::interval(Duration::from_secs(60 * 30));
    loop {
        select! {
            _ = interval.tick() => {
                if let Err(error) = client.keepalive_user_data_stream().await {
                    error!(?error, "Failed keepalive user data stream.");
                }
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
                                match parse_depth(data.bids, data.asks) {
                                    Ok((bids, asks)) => {
                                        let ai = assets
                                            .get(&data.symbol)
                                            .ok_or(BinanceFuturesError::AssetNotFound)?;
                                        ev_tx.send(
                                            Event::Depth(
                                                Depth {
                                                    asset_no: ai.asset_no,
                                                    exch_ts: data.ev_timestamp * 1000,
                                                    local_ts: Utc::now().timestamp_nanos_opt().unwrap(),
                                                    bids,
                                                    asks,
                                                }
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
                                    Ok((price, qty)) => {
                                        let asset_info = assets
                                            .get(&data.symbol)
                                        .ok_or(BinanceFuturesError::AssetNotFound)?;
                                        ev_tx.send(
                                            Event::Trade(
                                                ty::Trade {
                                                    asset_no: asset_info.asset_no,
                                                    exch_ts: data.ev_timestamp * 1000,
                                                    local_ts: Utc::now().timestamp_nanos_opt().unwrap(),
                                                    side: {
                                                        if data.is_the_buyer_the_market_maker {
                                                            SELL as i8
                                                        } else {
                                                            BUY as i8
                                                        }
                                                    },
                                                    price,
                                                    qty,
                                                }
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
                                            Event::Position(
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
                                if let Some(order_id) = parse_client_order_id(&data.order.client_order_id, &prefix) {
                                    if let Some(asset_info) = assets.get(&data.order.symbol) {
                                        let order = Order {
                                            qty: data.order.original_qty,
                                            leaves_qty: data.order.original_qty - data.order.order_filled_accumulated_qty,
                                            price_tick: (data.order.original_price / asset_info.tick_size).round() as i32,
                                            tick_size: asset_info.tick_size,
                                            side: data.order.side,
                                            time_in_force: data.order.time_in_force,
                                            exch_timestamp: data.transaction_time * 1000,
                                            status: data.order.order_status,
                                            local_timestamp: 0,
                                            req: Status::None,
                                            exec_price_tick: (data.order.last_filled_price / asset_info.tick_size).round() as i32,
                                            exec_qty: data.order.order_last_filled_qty,
                                            order_id,
                                            q: (),
                                            maker: false,
                                            order_type: data.order.order_type
                                        };
                                        ev_tx.send(
                                            Event::Order(
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