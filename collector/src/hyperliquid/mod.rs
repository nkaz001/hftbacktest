mod http;

use chrono::{DateTime, Utc};
pub use http::keep_connection;
use tokio::sync::mpsc::UnboundedSender;
use tokio_tungstenite::tungstenite::Utf8Bytes;
use tracing::error;

use crate::error::ConnectorError;

fn handle(
    writer_tx: &UnboundedSender<(DateTime<Utc>, String, String)>,
    recv_time: DateTime<Utc>,
    data: Utf8Bytes,
) -> Result<(), ConnectorError> {
    let j: serde_json::Value = serde_json::from_str(data.as_str())?;

    if let Some(channel) = j.get("channel") {
        let channel_str = channel.as_str().ok_or(ConnectorError::FormatError)?;

        if channel_str == "subscriptionResponse" {
            return Ok(());
        }

        if let Some(data_obj) = j.get("data") {
            let symbol = match channel_str {
                "trades" => {
                    if let Some(trades) = data_obj.as_array() {
                        if let Some(first_trade) = trades.first() {
                            first_trade
                                .get("coin")
                                .and_then(|c| c.as_str())
                                .ok_or(ConnectorError::FormatError)?
                        } else {
                            return Ok(());
                        }
                    } else {
                        return Err(ConnectorError::FormatError);
                    }
                }
                "l2Book" => data_obj
                    .get("coin")
                    .and_then(|c| c.as_str())
                    .ok_or(ConnectorError::FormatError)?,
                "bbo" => data_obj
                    .get("coin")
                    .and_then(|c| c.as_str())
                    .ok_or(ConnectorError::FormatError)?,
                _ => {
                    if let Some(coin) = data_obj.get("coin").and_then(|c| c.as_str()) {
                        coin
                    } else {
                        return Ok(());
                    }
                }
            };

            let _ = writer_tx.send((recv_time, symbol.to_string(), data.to_string()));
        }
    }

    Ok(())
}

pub async fn run_collection(
    subscriptions: Vec<String>,
    symbols: Vec<String>,
    writer_tx: UnboundedSender<(DateTime<Utc>, String, String)>,
) -> Result<(), anyhow::Error> {
    let (ws_tx, mut ws_rx) = tokio::sync::mpsc::unbounded_channel();
    let h = tokio::spawn(keep_connection(subscriptions, symbols, ws_tx.clone()));

    while let Some((recv_time, data)) = ws_rx.recv().await {
        if let Err(error) = handle(&writer_tx, recv_time, data) {
            error!(?error, "couldn't handle the received data.");
        }
    }
    let _ = h.await;
    Ok(())
}
