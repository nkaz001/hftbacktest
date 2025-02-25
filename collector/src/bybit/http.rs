use std::{
    io,
    io::ErrorKind,
    time::{Duration, Instant},
};

use anyhow::Error;
use chrono::{DateTime, Utc};
use futures_util::{SinkExt, StreamExt};
use tokio::{
    select,
    sync::mpsc::{UnboundedSender, unbounded_channel},
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Bytes, Message, Utf8Bytes, client::IntoClientRequest},
};
use tracing::{error, warn};

pub async fn connect(
    url: &str,
    topics: Vec<String>,
    ws_tx: UnboundedSender<(DateTime<Utc>, Utf8Bytes)>,
) -> Result<(), anyhow::Error> {
    let request = url.into_client_request()?;
    let (ws_stream, _) = connect_async(request).await?;
    let (mut write, mut read) = ws_stream.split();
    let (tx, mut rx) = unbounded_channel::<()>();

    write
        .send(Message::Text(
            format!(
                r#"{{"req_id": "subscribe", "op": "subscribe", "args": [{}]}}"#,
                topics
                    .iter()
                    .map(|s| format!("\"{s}\""))
                    .collect::<Vec<_>>()
                    .join(",")
            )
            .into(),
        ))
        .await?;

    tokio::spawn(async move {
        let mut ping_interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            select! {
                result = rx.recv() => {
                    match result {
                        Some(_) => {
                            if write.send(Message::Pong(Bytes::default())).await.is_err() {
                                return;
                            }
                        }
                        None => {
                            break;
                        }
                    }
                }
                _ = ping_interval.tick() => {
                    if write.send(
                        Message::Text(r#"{"req_id": "ping", "op": "ping"}"#.into())
                    ).await.is_err() {
                        return;
                    }
                }
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
    topics: Vec<String>,
    symbol_list: Vec<String>,
    ws_tx: UnboundedSender<(DateTime<Utc>, Utf8Bytes)>,
) {
    let mut error_count = 0;
    loop {
        let connect_time = Instant::now();
        let topics_ = symbol_list
            .iter()
            .flat_map(|pair| {
                topics
                    .iter()
                    .cloned()
                    .map(|stream| {
                        stream
                            .replace("$symbol", pair.to_uppercase().as_str())
                            .to_string()
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        if let Err(error) = connect(
            "wss://stream.bybit.com/v5/public/linear",
            topics_,
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
