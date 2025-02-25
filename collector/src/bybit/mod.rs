use chrono::{DateTime, Utc};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio_tungstenite::tungstenite::Utf8Bytes;
use tracing::error;

use self::http::keep_connection;
use crate::error::ConnectorError;

mod http;

fn handle(
    writer_tx: &UnboundedSender<(DateTime<Utc>, String, String)>,
    recv_time: DateTime<Utc>,
    data: Utf8Bytes,
) -> Result<(), ConnectorError> {
    let j: serde_json::Value = serde_json::from_str(data.as_str())?;
    if let Some(j_topic) = j.get("topic") {
        let topic = j_topic.as_str().ok_or(ConnectorError::FormatError)?;
        let symbol = topic.split(".").last().ok_or(ConnectorError::FormatError)?;
        let _ = writer_tx.send((recv_time, symbol.to_string(), data.to_string()));
    } else if let Some(j_success) = j.get("success") {
        let success = j_success.as_bool().ok_or(ConnectorError::FormatError)?;
        if !success {
            error!(%data, "couldn't subscribe the topics.");
            return Err(ConnectorError::ConnectionAbort);
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
