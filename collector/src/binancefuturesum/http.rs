use std::{
    io,
    io::ErrorKind,
    time::{Duration, Instant},
};

use anyhow::Error;
use chrono::{DateTime, Utc};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
};
use tracing::{error, warn};

pub async fn fetch_symbol_list() -> Result<Vec<String>, reqwest::Error> {
    Ok(reqwest::Client::new()
        .get("https://fapi.binance.com/fapi/v1/exchangeInfo")
        .header("Accept", "application/json")
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?
        .get("symbols")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .filter(|j_symbol| j_symbol.get("contractType").unwrap().as_str().unwrap() == "PERPETUAL")
        .map(|j_symbol| {
            j_symbol
                .get("symbol")
                .unwrap()
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect())
}

pub async fn fetch_depth_snapshot(symbol: &str) -> Result<String, reqwest::Error> {
    reqwest::Client::new()
        .get(format!(
            "https://fapi.binance.com/fapi/v1/depth?symbol={symbol}&limit=1000"
        ))
        .header("Accept", "application/json")
        .send()
        .await?
        .text()
        .await
}

pub async fn connect(
    url: &str,
    ws_tx: UnboundedSender<(DateTime<Utc>, String)>,
) -> Result<(), anyhow::Error> {
    let request = url.into_client_request()?;
    let (ws_stream, _) = connect_async(request).await?;
    let (mut write, mut read) = ws_stream.split();
    let (tx, mut rx) = unbounded_channel::<()>();

    tokio::spawn(async move {
        while rx.recv().await.is_some() {
            if write.send(Message::Pong(Vec::new())).await.is_err() {
                return;
            }
        }
    });

    loop {
        match read.next().await {
            Some(Ok(Message::Text(text))) => {
                let recv_time = Utc::now();
                if ws_tx.send((recv_time, text)).is_err() {
                    break;
                }
            }
            Some(Ok(Message::Binary(_))) => {}
            Some(Ok(Message::Ping(_))) => {
                tx.send(()).unwrap();
            }
            Some(Ok(Message::Pong(_))) => {}
            Some(Ok(Message::Close(close_frame))) => {
                warn!(?close_frame, "closed");
                return Err(Error::from(io::Error::new(
                    ErrorKind::ConnectionAborted,
                    "closed",
                )));
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
    Ok(())
}

pub async fn keep_connection(
    streams: Vec<String>,
    symbol_list: Vec<String>,
    ws_tx: UnboundedSender<(DateTime<Utc>, String)>,
) {
    let mut error_count = 0;
    loop {
        let connect_time = Instant::now();
        let streams_str = symbol_list
            .iter()
            .flat_map(|pair| {
                streams
                    .iter()
                    .cloned()
                    .map(|stream| {
                        stream
                            .replace("$symbol", pair.to_lowercase().as_str())
                            .to_string()
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
            .join("/");
        if let Err(error) = connect(
            &format!("wss://fstream.binance.com/stream?streams={streams_str}"),
            ws_tx.clone(),
        )
        .await
        {
            error!(?error, "websocket error");
            error_count += 1;
            if connect_time.elapsed() > Duration::from_secs(30) {
                error_count = 0;
            }
            if error_count > 3 {
                tokio::time::sleep(Duration::from_secs(1)).await;
            } else if error_count > 10 {
                tokio::time::sleep(Duration::from_secs(5)).await;
            } else if error_count > 20 {
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        } else {
            break;
        }
    }
}
