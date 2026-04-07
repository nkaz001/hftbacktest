//! Integration test for Binance Futures order book synchronization
//!
//! This test directly uses the actual orderbookmanager.rs implementation
//! to verify it works correctly with real Binance Futures market data.
//!
//! It also maintains an actual orderbook and displays top levels to verify correctness.
//!
//! Run with: cargo run --release --example binancefutures_orderbook_test
//!
//! Optional arguments:
//!   --symbol <symbol>   Trading pair to test (default: btcusdt)
//!   --duration <secs>   How long to run the test (default: 60)

use std::{collections::BTreeMap, time::Duration};

use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use tokio::time::timeout;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use tracing::{Level, debug, error, info, warn};

// Use the actual implementation from the connector library
use connector::binancefutures::{
    msg::{rest, stream},
    orderbookmanager::{DepthEvent, OrderBookManager},
};

const STREAM_URL: &str = "wss://fstream.binance.com/ws";
const API_URL: &str = "https://fapi.binance.com";

/// Simple orderbook using BTreeMap for sorted price levels
struct OrderBook {
    /// Bids: price -> quantity (sorted descending by price)
    bids: BTreeMap<i64, f64>, // price in ticks
    /// Asks: price -> quantity (sorted ascending by price)
    asks: BTreeMap<i64, f64>, // price in ticks
    /// Price precision (tick size)
    tick_size: f64,
}

impl OrderBook {
    fn new(tick_size: f64) -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            tick_size,
        }
    }

    fn price_to_tick(&self, price: f64) -> i64 {
        (price / self.tick_size).round() as i64
    }

    fn tick_to_price(&self, tick: i64) -> f64 {
        tick as f64 * self.tick_size
    }

    fn clear(&mut self) {
        self.bids.clear();
        self.asks.clear();
    }

    fn update_bid(&mut self, price: f64, qty: f64) {
        let tick = self.price_to_tick(price);
        if qty == 0.0 {
            self.bids.remove(&tick);
        } else {
            self.bids.insert(tick, qty);
        }
    }

    fn update_ask(&mut self, price: f64, qty: f64) {
        let tick = self.price_to_tick(price);
        if qty == 0.0 {
            self.asks.remove(&tick);
        } else {
            self.asks.insert(tick, qty);
        }
    }

    fn apply_snapshot(&mut self, bids: &[(String, String)], asks: &[(String, String)]) {
        self.clear();
        for (px_str, qty_str) in bids {
            if let (Ok(px), Ok(qty)) = (px_str.parse::<f64>(), qty_str.parse::<f64>()) {
                if qty > 0.0 {
                    self.update_bid(px, qty);
                }
            }
        }
        for (px_str, qty_str) in asks {
            if let (Ok(px), Ok(qty)) = (px_str.parse::<f64>(), qty_str.parse::<f64>()) {
                if qty > 0.0 {
                    self.update_ask(px, qty);
                }
            }
        }
    }

    fn apply_update(&mut self, bids: &[(String, String)], asks: &[(String, String)]) {
        for (px_str, qty_str) in bids {
            if let (Ok(px), Ok(qty)) = (px_str.parse::<f64>(), qty_str.parse::<f64>()) {
                self.update_bid(px, qty);
            }
        }
        for (px_str, qty_str) in asks {
            if let (Ok(px), Ok(qty)) = (px_str.parse::<f64>(), qty_str.parse::<f64>()) {
                self.update_ask(px, qty);
            }
        }
    }

    fn best_bid(&self) -> Option<(f64, f64)> {
        self.bids
            .iter()
            .next_back()
            .map(|(&tick, &qty)| (self.tick_to_price(tick), qty))
    }

    fn best_ask(&self) -> Option<(f64, f64)> {
        self.asks
            .iter()
            .next()
            .map(|(&tick, &qty)| (self.tick_to_price(tick), qty))
    }

    fn display_top(&self, levels: usize) {
        println!("\n{:=^60}", " ORDER BOOK ");
        println!(
            "{:>15} {:>15} | {:>15} {:>15}",
            "Bid Qty", "Bid Price", "Ask Price", "Ask Qty"
        );
        println!("{:-^60}", "");

        let top_bids: Vec<_> = self.bids.iter().rev().take(levels).collect();
        let top_asks: Vec<_> = self.asks.iter().take(levels).collect();

        for i in 0..levels {
            let bid_str = if i < top_bids.len() {
                let (tick, qty) = top_bids[i];
                format!("{:>15.4} {:>15.2}", qty, self.tick_to_price(*tick))
            } else {
                format!("{:>15} {:>15}", "", "")
            };

            let ask_str = if i < top_asks.len() {
                let (tick, qty) = top_asks[i];
                format!("{:>15.2} {:>15.4}", self.tick_to_price(*tick), qty)
            } else {
                format!("{:>15} {:>15}", "", "")
            };

            println!("{} | {}", bid_str, ask_str);
        }

        if let (Some((bid_px, _)), Some((ask_px, _))) = (self.best_bid(), self.best_ask()) {
            let spread = ask_px - bid_px;
            let mid = (bid_px + ask_px) / 2.0;
            println!("{:-^60}", "");
            println!(
                "Mid: {:.2}  Spread: {:.2} ({:.4}%)",
                mid,
                spread,
                spread / mid * 100.0
            );
        }
        println!(
            "Total levels: {} bids, {} asks",
            self.bids.len(),
            self.asks.len()
        );
        println!("{:=^60}\n", "");
    }

    fn validate(&self) -> bool {
        if let (Some((bid, _)), Some((ask, _))) = (self.best_bid(), self.best_ask()) {
            if bid >= ask {
                error!("INVALID ORDERBOOK: best bid {} >= best ask {}", bid, ask);
                return false;
            }
        }
        true
    }
}

#[derive(Parser, Debug)]
#[command(version, about = "Test Binance Futures order book synchronization")]
struct Args {
    /// Trading pair to test
    #[arg(short, long, default_value = "btcusdt")]
    symbol: String,

    /// How long to run the test in seconds
    #[arg(short, long, default_value = "60")]
    duration: u64,

    /// Show orderbook every N events
    #[arg(long, default_value = "50")]
    show_every: usize,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let args = Args::parse();
    let symbol = args.symbol.to_lowercase();

    info!("===========================================");
    info!("Binance Futures Order Book Sync Test");
    info!("Testing ACTUAL orderbookmanager.rs implementation");
    info!("Symbol: {}", symbol);
    info!("Duration: {} seconds", args.duration);
    info!("===========================================");

    let client = reqwest::Client::new();

    // Use the ACTUAL OrderBookManager from orderbookmanager.rs
    let mut order_book_manager = OrderBookManager::new();

    // Actual orderbook for verification
    let mut orderbook = OrderBook::new(0.01); // BTCUSDT tick size

    // Connect to WebSocket
    let url = format!("{}/{}@depth@100ms", STREAM_URL, symbol);
    info!("Connecting to WebSocket: {}", url);

    let request = url.into_client_request()?;
    let (ws_stream, _) = connect_async(request).await?;
    let (mut write, mut read) = ws_stream.split();

    info!("Connected to WebSocket");

    order_book_manager.start_init(&symbol);

    let mut buffered_count = 0;
    let mut synced_count = 0;
    let mut snapshot_requested = false;
    let mut snapshot: Option<rest::Depth> = None;
    let mut sync_complete = false;
    let mut gap_count = 0;
    let mut validation_errors = 0;

    let test_duration = Duration::from_secs(args.duration);
    let start_time = std::time::Instant::now();

    while start_time.elapsed() < test_duration {
        match timeout(Duration::from_secs(10), read.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                match serde_json::from_str::<stream::EventStream>(&text) {
                    Ok(stream::EventStream::DepthUpdate(depth_data)) => {
                        let (should_process, should_request_snapshot) =
                            order_book_manager.on_depth_update(&depth_data);

                        if !sync_complete {
                            buffered_count += 1;
                        }

                        // Request snapshot if needed
                        if should_request_snapshot && !snapshot_requested {
                            info!("Requesting REST snapshot...");
                            snapshot_requested = true;

                            let snapshot_url = format!(
                                "{}/fapi/v1/depth?symbol={}&limit=1000",
                                API_URL,
                                symbol.to_uppercase()
                            );

                            match client.get(&snapshot_url).send().await {
                                Ok(resp) => match resp.json::<rest::Depth>().await {
                                    Ok(snap) => {
                                        info!(
                                            "Received snapshot: lastUpdateId={}",
                                            snap.last_update_id
                                        );
                                        snapshot = Some(snap);
                                    }
                                    Err(e) => error!("Failed to parse snapshot: {:?}", e),
                                },
                                Err(e) => error!("Failed to fetch snapshot: {:?}", e),
                            }
                        }

                        // Apply snapshot
                        if !sync_complete {
                            if let Some(snap) = snapshot.take() {
                                let events = order_book_manager.on_snapshot(&symbol, &snap);

                                if events.is_empty() {
                                    warn!("Gap detected, retrying snapshot...");
                                    snapshot_requested = false;
                                    gap_count += 1;
                                } else {
                                    // Apply events to our orderbook
                                    for event in &events {
                                        match event {
                                            DepthEvent::Snapshot { bids, asks, .. } => {
                                                orderbook.apply_snapshot(bids, asks);
                                                info!(
                                                    "Applied snapshot: {} bids, {} asks",
                                                    orderbook.bids.len(),
                                                    orderbook.asks.len()
                                                );
                                            }
                                            DepthEvent::Update { bids, asks, .. } => {
                                                orderbook.apply_update(bids, asks);
                                            }
                                        }
                                    }

                                    if !order_book_manager.needs_snapshot(&symbol) {
                                        info!(
                                            "Order book synchronized! Processed {} events",
                                            events.len()
                                        );
                                        sync_complete = true;
                                        orderbook.display_top(5);
                                    }
                                }
                            }
                        }

                        // Apply synchronized updates
                        if sync_complete && should_process {
                            synced_count += 1;
                            orderbook.apply_update(&depth_data.bids, &depth_data.asks);

                            // Validate orderbook
                            if !orderbook.validate() {
                                validation_errors += 1;
                            }

                            // Display periodically
                            if synced_count % args.show_every == 0 {
                                info!("Synced #{} events", synced_count);
                                orderbook.display_top(5);
                            }
                        }

                        // Check for post-sync gaps
                        if sync_complete && order_book_manager.needs_snapshot(&symbol) {
                            error!("Gap detected after sync!");
                            sync_complete = false;
                            snapshot_requested = false;
                            gap_count += 1;
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        debug!("Parse error: {}", e);
                    }
                }
            }
            Ok(Some(Ok(Message::Ping(data)))) => {
                let _ = write.send(Message::Pong(data)).await;
            }
            Ok(Some(Err(e))) => {
                error!("WebSocket error: {:?}", e);
                break;
            }
            Ok(None) => {
                warn!("WebSocket closed");
                break;
            }
            Err(_) => {
                warn!("Timeout");
            }
            _ => {}
        }
    }

    // Final orderbook display
    info!("===========================================");
    info!("Final Order Book State");
    info!("===========================================");
    orderbook.display_top(10);

    // Summary
    info!("===========================================");
    info!("Test Complete");
    info!("===========================================");
    info!("Buffered events before sync: {}", buffered_count);
    info!("Synchronized events: {}", synced_count);
    info!("Gaps detected: {}", gap_count);
    info!("Validation errors: {}", validation_errors);

    if sync_complete && synced_count > 0 && validation_errors == 0 {
        info!("Result: PASSED");
        Ok(())
    } else if sync_complete && validation_errors == 0 {
        info!("Result: PASSED (synced)");
        Ok(())
    } else {
        error!("Result: FAILED");
        std::process::exit(1);
    }
}
