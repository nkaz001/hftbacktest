mod http;

use chrono::{DateTime, Utc};
pub use http::{fetch_depth_snapshot, keep_connection};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio_tungstenite::tungstenite::Utf8Bytes;
use tracing::{debug, error, info, warn};

use crate::{error::ConnectorError, throttler::Throttler};

// Reuse orderbookmanager from connector
use connector::binancefutures::{
    msg::{rest, stream},
    orderbookmanager::OrderBookManager,
};

/// Message type for internal snapshot channel
struct SnapshotData {
    symbol: String,
    json: String,
}

fn handle(
    order_book_manager: &mut OrderBookManager,
    writer_tx: &UnboundedSender<(DateTime<Utc>, String, String)>,
    snapshot_tx: &UnboundedSender<SnapshotData>,
    recv_time: DateTime<Utc>,
    data: Utf8Bytes,
    throttler: &Throttler,
) -> Result<(), ConnectorError> {
    let j: serde_json::Value = serde_json::from_str(data.as_str())?;
    if let Some(j_data) = j.get("data")
        && let Some(j_symbol) = j_data
            .as_object()
            .ok_or(ConnectorError::FormatError)?
            .get("s")
    {
        let symbol_upper = j_symbol.as_str().ok_or(ConnectorError::FormatError)?;
        let symbol = symbol_upper.to_lowercase();
        let ev = j_data
            .get("e")
            .ok_or(ConnectorError::FormatError)?
            .as_str()
            .ok_or(ConnectorError::FormatError)?;

        if ev == "depthUpdate" {
            // Parse the depth update
            let depth_data = stream::Depth {
                transaction_time: j_data.get("T").and_then(|v| v.as_i64()).unwrap_or(0),
                event_time: j_data.get("E").and_then(|v| v.as_i64()).unwrap_or(0),
                symbol: symbol.clone(),
                first_update_id: j_data
                    .get("U")
                    .ok_or(ConnectorError::FormatError)?
                    .as_i64()
                    .ok_or(ConnectorError::FormatError)?,
                last_update_id: j_data
                    .get("u")
                    .ok_or(ConnectorError::FormatError)?
                    .as_i64()
                    .ok_or(ConnectorError::FormatError)?,
                prev_update_id: j_data
                    .get("pu")
                    .ok_or(ConnectorError::FormatError)?
                    .as_i64()
                    .ok_or(ConnectorError::FormatError)?,
                bids: vec![], // Not needed for gap detection
                asks: vec![],
            };

            // Use OrderBookManager to check sync state
            let (should_process, should_request_snapshot) =
                order_book_manager.on_depth_update(&depth_data);

            if should_request_snapshot {
                info!(
                    symbol = %symbol,
                    U = depth_data.first_update_id,
                    u = depth_data.last_update_id,
                    "Requesting depth snapshot"
                );
                let symbol_ = symbol_upper.to_string();
                let symbol_lower = symbol.clone();
                let snapshot_tx_ = snapshot_tx.clone();
                let mut throttler_ = throttler.clone();
                tokio::spawn(async move {
                    match throttler_.execute(fetch_depth_snapshot(&symbol_)).await {
                        Some(Ok(data)) => {
                            // Send the snapshot to be processed by OrderBookManager
                            let _ = snapshot_tx_.send(SnapshotData {
                                symbol: symbol_lower,
                                json: data,
                            });
                        }
                        Some(Err(error)) => {
                            error!(
                                symbol = symbol_,
                                ?error,
                                "couldn't fetch the depth snapshot."
                            );
                        }
                        None => {
                            warn!(
                                symbol = symbol_,
                                "Fetching the depth snapshot is rate-limited."
                            )
                        }
                    }
                });
            }

            if !should_process {
                debug!(
                    symbol = %symbol,
                    U = depth_data.first_update_id,
                    u = depth_data.last_update_id,
                    "Buffering event (not yet synced)"
                );
            }
        }

        // Always write the raw data to file
        let _ = writer_tx.send((recv_time, symbol, data.to_string()));
    }
    Ok(())
}

/// Handle REST snapshot response and apply to OrderBookManager
fn handle_snapshot(
    order_book_manager: &mut OrderBookManager,
    symbol: &str,
    snapshot_json: &str,
) -> Result<bool, ConnectorError> {
    let j: serde_json::Value = serde_json::from_str(snapshot_json)?;

    let snapshot = rest::Depth {
        last_update_id: j
            .get("lastUpdateId")
            .ok_or(ConnectorError::FormatError)?
            .as_i64()
            .ok_or(ConnectorError::FormatError)?,
        event_time: j.get("E").and_then(|v| v.as_i64()).unwrap_or(0),
        transaction_time: j.get("T").and_then(|v| v.as_i64()).unwrap_or(0),
        bids: vec![], // Not needed for sync logic
        asks: vec![],
    };

    let events = order_book_manager.on_snapshot(symbol, &snapshot);

    if events.is_empty() {
        warn!(symbol = %symbol, "Gap detected during snapshot processing, will retry");
        Ok(false)
    } else {
        info!(
            symbol = %symbol,
            last_update_id = snapshot.last_update_id,
            events_count = events.len(),
            "Snapshot applied successfully"
        );
        Ok(true)
    }
}

pub async fn run_collection(
    streams: Vec<String>,
    symbols: Vec<String>,
    writer_tx: UnboundedSender<(DateTime<Utc>, String, String)>,
) -> Result<(), anyhow::Error> {
    // Use the proper OrderBookManager from connector
    let mut order_book_manager = OrderBookManager::new();

    // Initialize all symbols
    for symbol in &symbols {
        order_book_manager.start_init(&symbol.to_lowercase());
    }

    let (ws_tx, mut ws_rx) = unbounded_channel();
    let (snapshot_tx, mut snapshot_rx) = unbounded_channel::<SnapshotData>();
    let h = tokio::spawn(keep_connection(streams, symbols, ws_tx.clone()));

    // https://www.binance.com/en/support/faq/rate-limits-on-binance-futures-281596e222414cdd9051664ea621cdc3
    // The default rate limit per IP is 2,400/min and the weight is 20 at a depth of 1000.
    // The maximum request rate for fetching snapshots is 120 per minute.
    // Sets the rate limit with a margin to account for connection requests.
    let throttler = Throttler::new(100);

    loop {
        tokio::select! {
            // Handle WebSocket data
            ws_data = ws_rx.recv() => {
                match ws_data {
                    Some((recv_time, data)) => {
                        if let Err(error) = handle(
                            &mut order_book_manager,
                            &writer_tx,
                            &snapshot_tx,
                            recv_time,
                            data,
                            &throttler,
                        ) {
                            error!(?error, "couldn't handle the received data.");
                        }
                    }
                    None => {
                        // WebSocket channel closed
                        break;
                    }
                }
            }
            // Handle snapshot responses
            snapshot_data = snapshot_rx.recv() => {
                if let Some(snapshot) = snapshot_data {
                    let recv_time = Utc::now();

                    // Process snapshot through OrderBookManager
                    match handle_snapshot(&mut order_book_manager, &snapshot.symbol, &snapshot.json) {
                        Ok(true) => {
                            // Snapshot applied successfully, write to file
                            let _ = writer_tx.send((recv_time, snapshot.symbol, snapshot.json));
                        }
                        Ok(false) => {
                            // Gap detected, OrderBookManager will request new snapshot on next depth update
                            // Still write the snapshot to file for completeness
                            let _ = writer_tx.send((recv_time, snapshot.symbol, snapshot.json));
                        }
                        Err(error) => {
                            error!(?error, "Failed to parse snapshot");
                        }
                    }
                }
            }
        }
    }
    let _ = h.await;
    Ok(())
}
