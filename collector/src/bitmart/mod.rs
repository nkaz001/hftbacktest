mod http;

use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use chrono::{DateTime, Utc};
pub use http::{fetch_depth_snapshot, keep_connection};
use tracing::{error, warn};
use tokio_tungstenite::tungstenite::Utf8Bytes;

use crate::{error::ConnectorError, throttler::Throttler};
use std::collections::HashMap;

fn handle(
    writer_tx: &UnboundedSender<(DateTime<Utc>, String, String)>,
    recv_time: DateTime<Utc>,
    data: Utf8Bytes
) -> Result<(), ConnectorError> {
    let j: serde_json::Value = serde_json::from_str(data.as_str())?;
    let group = j.get("group").ok_or(ConnectorError::FormatError)?.as_str().ok_or(ConnectorError::FormatError)?;
    // If the group string starts with "futures/trade"
    if group.starts_with("futures/trade") {
        let symbol = group.split("/trade:").last().ok_or(ConnectorError::FormatError)?;
        let _ = writer_tx.send((recv_time, symbol.to_string(), data.to_string()));
    } else if group.starts_with("futures/depthIncrease50") {
        if let Some(j_data) = j.get("data") {
            if let Some(j_symbol) = j_data
                .as_object()
                .ok_or(ConnectorError::FormatError)?
                .get("symbol")
            {
                let symbol = j_symbol.as_str().ok_or(ConnectorError::FormatError)?;
                let _ = writer_tx.send((recv_time, symbol.to_string(), data.to_string()));
            }
        }
    }
    Ok(())
}

pub async fn run_collection(
    topics: Vec<String>,
    symbols: Vec<String>,
    writer_tx: UnboundedSender<(DateTime<Utc>, String, String)>,
) -> Result<(), anyhow::Error> {
    let (ws_tx, mut ws_rx) = unbounded_channel();
    let h = tokio::spawn(keep_connection(topics, symbols, ws_tx.clone()));
    while let Some((recv_time, data)) = ws_rx.recv().await {
        if let Err(error) = handle(&writer_tx, recv_time, data) {
            error!(?error, "couldn't handle the received data.");
        }
    }
    let _ = h.await;
    Ok(())
}