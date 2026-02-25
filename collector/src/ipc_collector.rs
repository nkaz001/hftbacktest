//! IPC Collector - subscribes to a running Connector to collect market data.
//!
//! This module allows the collector to receive market data through IPC (shared memory)
//! from a running Connector process, instead of directly connecting to the exchange.
//!
//! Benefits:
//! - Single connection to exchange (via Connector)
//! - Data consistency with live trading
//! - No additional WebSocket connections needed

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use chrono::{DateTime, Utc};
use hftbacktest::{
    live::ipc::iceoryx::{IceoryxBuilder, IceoryxReceiver, IceoryxSender},
    types::{
        Event, LiveEvent, LiveRequest, DEPTH_BBO_EVENT, DEPTH_CLEAR_EVENT, DEPTH_EVENT,
        DEPTH_SNAPSHOT_EVENT, TRADE_EVENT,
    },
};
use iceoryx2::node::NodeWaitFailure;
use iceoryx2::prelude::{Node, NodeBuilder, ipc};
use iceoryx2_bb_posix::signal::SignalHandler;
use serde::Serialize;
use tokio::sync::mpsc::UnboundedSender;
use tracing::{debug, info, warn};

use crate::error::ConnectorError;

/// Unique ID for this collector instance
const COLLECTOR_ID: u64 = 0xC011EC70; // "COLLECTO" in hex-ish

/// JSON format for depth update events (compatible with existing collector format)
#[derive(Serialize)]
#[allow(non_snake_case)]
struct DepthUpdate {
    e: &'static str,     // event type: "depthUpdate"
    E: i64,              // event time (exchange timestamp ms)
    s: String,           // symbol
    b: Vec<[String; 2]>, // bids [[price, qty], ...]
    a: Vec<[String; 2]>, // asks [[price, qty], ...]
}

/// JSON format for trade events
#[derive(Serialize)]
#[allow(non_snake_case)]
struct Trade {
    e: &'static str, // event type: "trade"
    E: i64,          // event time (exchange timestamp ms)
    s: String,       // symbol
    p: String,       // price
    q: String,       // quantity
    m: bool,         // is buyer maker
}

/// JSON format for BBO (best bid/offer) events
#[derive(Serialize)]
#[allow(non_snake_case)]
struct BookTicker {
    e: &'static str, // event type: "bookTicker"
    s: String,       // symbol
    b: String,       // best bid price
    B: String,       // best bid qty
    a: String,       // best ask price
    A: String,       // best ask qty
    E: i64,          // event time
}

/// IPC Collector that subscribes to a Connector's market data feed
pub struct IpcCollector {
    connector_name: String,
    sender: IceoryxSender<LiveRequest>,
    receiver: IceoryxReceiver<LiveEvent>,
    node: Node<ipc::Service>,
    writer_tx: UnboundedSender<(DateTime<Utc>, String, String)>,
    symbols: Vec<String>,
    event_count: u64,
    shutdown: Arc<AtomicBool>,
}

impl IpcCollector {
    /// Create a new IPC Collector that subscribes to the specified connector
    pub fn new(
        connector_name: &str,
        symbols: Vec<String>,
        writer_tx: UnboundedSender<(DateTime<Utc>, String, String)>,
        shutdown: Arc<AtomicBool>,
    ) -> Result<Self, ConnectorError> {
        info!(%connector_name, ?symbols, "Creating IPC collector");

        // Create sender to send RegisterInstrument requests
        let sender: IceoryxSender<LiveRequest> = IceoryxBuilder::new(connector_name)
            .bot(true) // Act as a bot (sender to FromBot)
            .sender()
            .map_err(|e| ConnectorError::IpcError(e.to_string()))?;

        // Create receiver to subscribe to connector's ToBot service
        let receiver: IceoryxReceiver<LiveEvent> = IceoryxBuilder::new(connector_name)
            .bot(true) // Act as a subscriber (like a Bot)
            .receiver()
            .map_err(|e| ConnectorError::IpcError(e.to_string()))?;

        // Create a node for waiting on events
        let node = NodeBuilder::new()
            .create::<ipc::Service>()
            .map_err(|e| ConnectorError::IpcError(e.to_string()))?;

        Ok(Self {
            connector_name: connector_name.to_string(),
            sender,
            receiver,
            node,
            writer_tx,
            symbols,
            event_count: 0,
            shutdown,
        })
    }

    /// Register symbols with the connector
    fn register_symbols(&self) -> Result<(), ConnectorError> {
        for symbol in &self.symbols {
            info!(%symbol, "Registering symbol with connector");
            let request = LiveRequest::RegisterInstrument {
                symbol: symbol.clone(),
                tick_size: 0.01, // Default tick size
                lot_size: 0.001, // Default lot size
            };
            self.sender
                .send(COLLECTOR_ID, &request)
                .map_err(|e| ConnectorError::IpcError(e.to_string()))?;
        }
        Ok(())
    }

    /// Run the IPC collector event loop
    pub fn run(&mut self) -> Result<(), ConnectorError> {
        info!(connector = %self.connector_name, "IPC collector started");

        // Register symbols first
        self.register_symbols()?;

        info!("Waiting for market data events...");

        loop {
            // Check shutdown flag (set by main thread)
            if self.shutdown.load(Ordering::Relaxed) {
                info!("IPC collector received shutdown signal from main thread");
                break;
            }

            // Check iceoryx2's built-in signal handler
            if SignalHandler::termination_requested() {
                info!("IPC collector received termination request via iceoryx2");
                self.shutdown.store(true, Ordering::SeqCst);
                break;
            }

            // Wait for events with a small timeout
            match self.node.wait(Duration::from_millis(100)) {
                Ok(()) => {
                    // Try to receive all available events
                    while let Some((id, event)) = self
                        .receiver
                        .receive()
                        .map_err(|e| ConnectorError::IpcError(e.to_string()))?
                    {
                        // Accept broadcasts (id=0) or messages sent to us
                        if id == 0 || id == COLLECTOR_ID {
                            self.handle_event(event)?;
                        }
                    }
                }
                Err(NodeWaitFailure::TerminationRequest) => {
                    info!("IPC collector received TerminationRequest from node.wait()");
                    self.shutdown.store(true, Ordering::SeqCst);
                    break;
                }
                Err(NodeWaitFailure::Interrupt) => {
                    // Interrupted by signal, check if we should terminate
                    if SignalHandler::termination_requested() {
                        info!("IPC collector interrupted with termination request");
                        self.shutdown.store(true, Ordering::SeqCst);
                        break;
                    }
                    continue;
                }
            }
        }

        info!("IPC collector stopped");
        Ok(())
    }

    /// Handle a received LiveEvent
    fn handle_event(&mut self, event: LiveEvent) -> Result<(), ConnectorError> {
        match event {
            LiveEvent::Feed { symbol, event } => {
                self.handle_feed_event(&symbol, &event)?;
                self.event_count += 1;
                if self.event_count % 1000 == 0 {
                    info!(count = self.event_count, "Events received");
                }
            }
            LiveEvent::BatchStart => {
                debug!("Batch start");
            }
            LiveEvent::BatchEnd => {
                debug!("Batch end");
            }
            LiveEvent::Order { symbol, order } => {
                debug!(%symbol, ?order, "Order event (ignored for data collection)");
            }
            LiveEvent::Position { symbol, qty, .. } => {
                debug!(%symbol, %qty, "Position event (ignored for data collection)");
            }
            LiveEvent::Error(err) => {
                warn!(?err, "Received error event from connector");
            }
            LiveEvent::InstrumentInfo { symbol, tick_size, lot_size } => {
                debug!(%symbol, %tick_size, %lot_size, "InstrumentInfo event (ignored for data collection)");
            }
        }
        Ok(())
    }

    /// Handle a feed event and convert to JSON for storage
    fn handle_feed_event(&mut self, symbol: &str, event: &Event) -> Result<(), ConnectorError> {
        let recv_time = Utc::now();
        let exch_ts_ms = event.exch_ts / 1_000_000; // Convert ns to ms

        // Determine event type from flags
        let ev_type = event.ev & 0xFF; // Lower 8 bits are event type
        let is_buy = (event.ev & hftbacktest::types::BUY_EVENT) != 0;

        let json = match ev_type {
            DEPTH_EVENT | DEPTH_SNAPSHOT_EVENT => {
                // Depth update event
                let (bids, asks) = if is_buy {
                    (
                        vec![[format!("{}", event.px), format!("{}", event.qty)]],
                        vec![],
                    )
                } else {
                    (
                        vec![],
                        vec![[format!("{}", event.px), format!("{}", event.qty)]],
                    )
                };

                let update = DepthUpdate {
                    e: if ev_type == DEPTH_SNAPSHOT_EVENT {
                        "depthSnapshot"
                    } else {
                        "depthUpdate"
                    },
                    E: exch_ts_ms,
                    s: symbol.to_uppercase(),
                    b: bids,
                    a: asks,
                };
                serde_json::to_string(&update)
                    .map_err(|e| ConnectorError::IpcError(e.to_string()))?
            }
            TRADE_EVENT => {
                let trade = Trade {
                    e: "trade",
                    E: exch_ts_ms,
                    s: symbol.to_uppercase(),
                    p: format!("{}", event.px),
                    q: format!("{}", event.qty),
                    m: !is_buy, // is_buyer_maker is opposite of is_buy (taker side)
                };
                serde_json::to_string(&trade)
                    .map_err(|e| ConnectorError::IpcError(e.to_string()))?
            }
            DEPTH_BBO_EVENT => {
                // BBO event - we need to handle this differently
                // The event contains either bid or ask based on is_buy flag
                let ticker = BookTicker {
                    e: "bookTicker",
                    s: symbol.to_uppercase(),
                    b: if is_buy {
                        format!("{}", event.px)
                    } else {
                        "0".to_string()
                    },
                    B: if is_buy {
                        format!("{}", event.qty)
                    } else {
                        "0".to_string()
                    },
                    a: if !is_buy {
                        format!("{}", event.px)
                    } else {
                        "0".to_string()
                    },
                    A: if !is_buy {
                        format!("{}", event.qty)
                    } else {
                        "0".to_string()
                    },
                    E: exch_ts_ms,
                };
                serde_json::to_string(&ticker)
                    .map_err(|e| ConnectorError::IpcError(e.to_string()))?
            }
            DEPTH_CLEAR_EVENT => {
                // Depth clear event - skip or log
                debug!(%symbol, "Depth clear event");
                return Ok(());
            }
            _ => {
                debug!(%symbol, ev_type, "Unknown event type");
                return Ok(());
            }
        };

        // Send to writer
        self.writer_tx
            .send((recv_time, symbol.to_lowercase(), json))
            .map_err(|e| ConnectorError::IpcError(e.to_string()))?;

        Ok(())
    }
}

/// Run IPC collection from a connector with shutdown support
pub async fn run_ipc_collection(
    connector_name: String,
    symbols: Vec<String>,
    writer_tx: UnboundedSender<(DateTime<Utc>, String, String)>,
    shutdown: Arc<AtomicBool>,
) -> Result<(), anyhow::Error> {
    info!(%connector_name, ?symbols, "Starting IPC collection");

    // Run the blocking IPC collector in a separate thread
    let handle = tokio::task::spawn_blocking(move || {
        let mut collector = IpcCollector::new(&connector_name, symbols, writer_tx, shutdown)?;
        collector.run()
    });

    handle.await??;
    Ok(())
}
