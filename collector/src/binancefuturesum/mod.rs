mod http;

use std::collections::HashMap;

use chrono::{DateTime, Utc};
pub use http::{fetch_depth_snapshot, fetch_symbol_list, keep_connection};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tracing::{error, warn};

use crate::error::ConnectorError;

fn handle(
    prev_u_map: &mut HashMap<String, i64>,
    writer_tx: &UnboundedSender<(DateTime<Utc>, String, String)>,
    recv_time: DateTime<Utc>,
    data: String,
) -> Result<(), ConnectorError> {
    let j: serde_json::Value = serde_json::from_str(&data)?;
    if let Some(j_data) = j.get("data") {
        if let Some(j_symbol) = j_data
            .as_object()
            .ok_or(ConnectorError::FormatError)?
            .get("s")
        {
            let symbol = j_symbol.as_str().ok_or(ConnectorError::FormatError)?;
            let ev = j_data
                .get("e")
                .ok_or(ConnectorError::FormatError)?
                .as_str()
                .ok_or(ConnectorError::FormatError)?;
            if ev == "depthUpdate" {
                let u = j_data
                    .get("u")
                    .ok_or(ConnectorError::FormatError)?
                    .as_i64()
                    .ok_or(ConnectorError::FormatError)?;
                let pu = j_data
                    .get("pu")
                    .ok_or(ConnectorError::FormatError)?
                    .as_i64()
                    .ok_or(ConnectorError::FormatError)?;
                let prev_u = prev_u_map.get(symbol);
                if prev_u.is_none() || pu != *prev_u.unwrap() {
                    warn!(%symbol, "missing depth feed has been detected.");
                    // todo: to circumvent API limits when repeated occurrences of missing depth
                    //       feed happen within a short timeframe, implementing a backoff mechanism
                    //       may be necessary.
                    let symbol_ = symbol.to_string();
                    let writer_tx_ = writer_tx.clone();
                    tokio::spawn(async move {
                        match fetch_depth_snapshot(&symbol_).await {
                            Ok(data) => {
                                let recv_time = Utc::now();
                                let _ = writer_tx_.send((recv_time, symbol_, data));
                            }
                            Err(error) => {
                                error!(
                                    symbol = symbol_,
                                    ?error,
                                    "couldn't fetch the depth snapshot."
                                );
                            }
                        }
                    });
                }
                *prev_u_map.entry(symbol.to_string()).or_insert(0) = u;
            }
            let _ = writer_tx.send((recv_time, symbol.to_string(), data));
        }
    }
    Ok(())
}

pub async fn run_collection(
    streams: Vec<String>,
    symbols: Vec<String>,
    writer_tx: UnboundedSender<(DateTime<Utc>, String, String)>,
) -> Result<(), anyhow::Error> {
    let mut prev_u_map = HashMap::new();
    let (ws_tx, mut ws_rx) = unbounded_channel();
    let h = tokio::spawn(keep_connection(streams, symbols, ws_tx.clone()));
    loop {
        match ws_rx.recv().await {
            Some((recv_time, data)) => {
                if let Err(error) = handle(&mut prev_u_map, &writer_tx, recv_time, data) {
                    error!(?error, "couldn't handle the received data.");
                }
            }
            None => {
                break;
            }
        }
    }
    let _ = h.await;
    Ok(())
}
