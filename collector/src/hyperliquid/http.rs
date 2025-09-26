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
    tungstenite::{Message, Utf8Bytes, client::IntoClientRequest},
};
use tracing::{error, info, warn};

pub async fn connect(
    url: &str,
    subscriptions: Vec<serde_json::Value>,
    ws_tx: UnboundedSender<(DateTime<Utc>, Utf8Bytes)>,
) -> Result<(), anyhow::Error> {
    let request = url.into_client_request()?;
    let (ws_stream, _) = connect_async(request).await?;
    let (mut write, mut read) = ws_stream.split();
    let (_ping_tx, mut ping_rx) = unbounded_channel::<()>();

    for subscription in subscriptions {
        write
            .send(Message::Text(subscription.to_string().into()))
            .await?;
    }

    tokio::spawn(async move {
        let mut ping_interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            select! {
                _ = ping_interval.tick() => {
                    if write.send(Message::Text(r#"{"method":"ping"}"#.into())).await.is_err() {
                        return;
                    }
                }
                result = ping_rx.recv() => {
                    if result.is_none() {
                        break;
                    }
                }
            }
        }
    });

    loop {
        match read.next().await {
            Some(Ok(Message::Text(text))) => {
                let recv_time = Utc::now();

                if let Ok(j) = serde_json::from_str::<serde_json::Value>(&text)
                    && j.get("channel").and_then(|c| c.as_str()) == Some("pong")
                {
                    continue;
                }

                if ws_tx.send((recv_time, text)).is_err() {
                    break;
                }
            }
            Some(Ok(Message::Binary(_))) => {}
            Some(Ok(Message::Ping(_))) => {
                // Hyperliquid uses JSON ping/pong, not WebSocket ping/pong
            }
            Some(Ok(Message::Pong(_))) => {}
            Some(Ok(Message::Close(close_frame))) => {
                warn!(?close_frame, "connection closed");
                return Err(Error::from(io::Error::new(
                    ErrorKind::ConnectionAborted,
                    "connection closed",
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
    subscription_types: Vec<String>,
    symbol_list: Vec<String>,
    ws_tx: UnboundedSender<(DateTime<Utc>, Utf8Bytes)>,
) {
    let mut error_count = 0;
    loop {
        let connect_time = Instant::now();

        let subscriptions: Vec<serde_json::Value> = symbol_list
            .iter()
            .flat_map(|symbol| {
                subscription_types.iter().map(move |sub_type| {
                    serde_json::json!({
                        "method": "subscribe",
                        "subscription": {
                            "type": sub_type,
                            "coin": symbol
                        }
                    })
                })
            })
            .collect();

        info!(
            "Connecting to Hyperliquid WebSocket with {} subscriptions",
            subscriptions.len()
        );

        if let Err(error) =
            connect("wss://api.hyperliquid.xyz/ws", subscriptions, ws_tx.clone()).await
        {
            error!(?error, "websocket error");
            error_count += 1;
            if connect_time.elapsed() > Duration::from_secs(30) {
                error_count = 0;
            }

            let sleep_duration = if error_count > 20 {
                Duration::from_secs(10)
            } else if error_count > 10 {
                Duration::from_secs(5)
            } else if error_count > 3 {
                Duration::from_secs(1)
            } else {
                Duration::from_millis(500)
            };

            tokio::time::sleep(sleep_duration).await;
        } else {
            break;
        }
    }
}
