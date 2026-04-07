//! Live market data integration test for supported public exchange connectors.
//!
//! This example connects to public testnet WebSocket endpoints, subscribes to
//! market data streams, validates parsing and top-of-book consistency, and
//! reports per-connector statistics without placing any orders.
//!
//! Examples:
//!   cargo run -p connector --example live_market_data_test -- --connector all
//!   cargo run -p connector --example live_market_data_test -- --connector bybit --duration 10

use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use connector::binancefutures::msg::stream::{
    EventStream as BinanceFuturesEventStream, Stream as BinanceFuturesStream,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::Value;
use tokio::time::{Instant, MissedTickBehavior, interval};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};

// Public data only — no API keys, no orders. Safe for mainnet.
const BINANCE_FUTURES_URL: &str = "wss://fstream.binance.com/ws";
const BINANCE_SPOT_URL: &str = "wss://stream.binance.com:9443/ws";
const BYBIT_URL: &str = "wss://stream.bybit.com/v5/public/linear";

#[derive(Parser, Debug)]
#[command(version, about = "Live public market-data integration test")]
struct Args {
    #[arg(long, value_enum, default_value_t = ConnectorChoice::All)]
    connector: ConnectorChoice,

    #[arg(long, default_value = "btcusdt")]
    symbol: String,

    #[arg(long, default_value_t = 15)]
    duration: u64,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "lower")]
enum ConnectorChoice {
    BinanceFutures,
    BinanceSpot,
    Bybit,
    All,
}

#[derive(Copy, Clone, Debug)]
enum ConnectorKind {
    BinanceFutures,
    BinanceSpot,
    Bybit,
}

impl ConnectorKind {
    fn display_name(self) -> &'static str {
        match self {
            Self::BinanceFutures => "BinanceFutures",
            Self::BinanceSpot => "BinanceSpot",
            Self::Bybit => "Bybit",
        }
    }

    fn bbo_label(self) -> &'static str {
        match self {
            Self::Bybit => "bbo",
            Self::BinanceFutures | Self::BinanceSpot => "bookTicker",
        }
    }

    fn expects_bbo_stream(self) -> bool {
        true
    }
}

#[derive(Clone, Copy, Debug)]
struct BboSnapshot {
    bid: f64,
    ask: f64,
}

#[derive(Debug)]
struct TestStats {
    kind: ConnectorKind,
    depth_count: u64,
    bbo_count: u64,
    trade_count: u64,
    parse_errors: u64,
    bbo_crosses: u64,
    last_bbo: Option<BboSnapshot>,
    runtime_error: Option<String>,
}

impl TestStats {
    fn new(kind: ConnectorKind) -> Self {
        Self {
            kind,
            depth_count: 0,
            bbo_count: 0,
            trade_count: 0,
            parse_errors: 0,
            bbo_crosses: 0,
            last_bbo: None,
            runtime_error: None,
        }
    }

    fn passed(&self) -> bool {
        self.parse_errors == 0
            && self.bbo_crosses == 0
            && self.depth_count > 0
            && self.trade_count > 0
            && (!self.kind.expects_bbo_stream() || self.bbo_count > 0)
            && self.runtime_error.is_none()
    }

    fn print_report(&self, duration_secs: u64) {
        let elapsed = duration_secs as f64;
        println!("=== {} ({}s) ===", self.kind.display_name(), duration_secs);
        println!(
            "  depth:       {:>6} ({:>5.1}/s)",
            self.depth_count,
            self.depth_count as f64 / elapsed
        );
        println!(
            "  {}:  {:>6} ({:>5.1}/s)",
            self.kind.bbo_label(),
            self.bbo_count,
            self.bbo_count as f64 / elapsed
        );
        println!(
            "  trade:       {:>6} ({:>5.1}/s)",
            self.trade_count,
            self.trade_count as f64 / elapsed
        );
        println!("  parse errors: {}", self.parse_errors);
        println!("  BBO crosses:  {}", self.bbo_crosses);
        if let Some(last_bbo) = self.last_bbo {
            println!(
                "  Last BBO: bid={:.6} ask={:.6} spread={:.6}",
                last_bbo.bid,
                last_bbo.ask,
                last_bbo.ask - last_bbo.bid
            );
        } else {
            println!("  Last BBO: n/a");
        }
        if let Some(error) = &self.runtime_error {
            println!("  Runtime error: {error}");
        }
        println!(
            "  {} {}\n",
            if self.passed() { "PASS" } else { "FAIL" },
            status_mark(self.passed())
        );
    }

    fn summary_status(&self) -> &'static str {
        if self.passed() { "PASS" } else { "FAIL" }
    }
}

fn status_mark(passed: bool) -> char {
    if passed { '✓' } else { '✗' }
}

fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_millis();
    ts.to_string()
}

fn parse_price(value: &str) -> Result<f64> {
    value
        .parse::<f64>()
        .with_context(|| format!("invalid decimal value: {value}"))
}

fn record_bbo(stats: &mut TestStats, bid: f64, ask: f64) {
    stats.last_bbo = Some(BboSnapshot { bid, ask });
    if bid <= 0.0 || ask <= 0.0 || bid >= ask {
        stats.bbo_crosses += 1;
    }
}

fn record_bbo_from_strings(stats: &mut TestStats, bid: &str, ask: &str) {
    match (parse_price(bid), parse_price(ask)) {
        (Ok(bid), Ok(ask)) => record_bbo(stats, bid, ask),
        _ => stats.parse_errors += 1,
    }
}

fn record_bbo_from_levels(
    stats: &mut TestStats,
    bids: &[(String, String)],
    asks: &[(String, String)],
) {
    match (bids.first(), asks.first()) {
        (Some((bid, _)), Some((ask, _))) => record_bbo_from_strings(stats, bid, ask),
        _ => stats.parse_errors += 1,
    }
}

async fn run_binance_futures(symbol: &str, duration_secs: u64) -> Result<TestStats> {
    let mut stats = TestStats::new(ConnectorKind::BinanceFutures);
    let request = BINANCE_FUTURES_URL.into_client_request()?;
    let (ws_stream, _) = connect_async(request)
        .await
        .context("failed to connect to Binance Futures")?;
    let (mut write, mut read) = ws_stream.split();

    let subscription = format!(
        r#"{{"method":"SUBSCRIBE","params":["{symbol}@depth@100ms","{symbol}@bookTicker","{symbol}@trade"],"id":"{}"}}"#,
        generate_id()
    );
    write
        .send(Message::Text(subscription.into()))
        .await
        .context("failed to subscribe to Binance Futures streams")?;

    let stop_at = Instant::now() + Duration::from_secs(duration_secs);
    let stop = tokio::time::sleep_until(stop_at);
    tokio::pin!(stop);

    loop {
        tokio::select! {
            _ = &mut stop => break,
            message = read.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<BinanceFuturesStream>(&text) {
                            Ok(BinanceFuturesStream::Result(_)) => {}
                            Ok(BinanceFuturesStream::EventStream(BinanceFuturesEventStream::DepthUpdate(_))) => {
                                stats.depth_count += 1;
                            }
                            Ok(BinanceFuturesStream::EventStream(BinanceFuturesEventStream::BookTicker(book_ticker))) => {
                                stats.bbo_count += 1;
                                record_bbo_from_strings(&mut stats, &book_ticker.bid_price, &book_ticker.ask_price);
                            }
                            Ok(BinanceFuturesStream::EventStream(BinanceFuturesEventStream::Trade(_))) => {
                                stats.trade_count += 1;
                            }
                            Ok(BinanceFuturesStream::EventStream(_)) => {}
                            Err(_) => {
                                stats.parse_errors += 1;
                            }
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if let Err(error) = write.send(Message::Pong(payload)).await {
                            stats.runtime_error = Some(error.to_string());
                            break;
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Binary(_))) => {}
                    Some(Ok(Message::Frame(_))) => {}
                    Some(Ok(Message::Close(frame))) => {
                        stats.runtime_error = Some(match frame {
                            Some(frame) => frame.to_string(),
                            None => "connection closed".to_string(),
                        });
                        break;
                    }
                    Some(Err(error)) => {
                        stats.runtime_error = Some(error.to_string());
                        break;
                    }
                    None => {
                        stats.runtime_error = Some("stream ended unexpectedly".to_string());
                        break;
                    }
                }
            }
        }
    }

    Ok(stats)
}

async fn run_binance_spot(symbol: &str, duration_secs: u64) -> Result<TestStats> {
    let mut stats = TestStats::new(ConnectorKind::BinanceSpot);
    let request = BINANCE_SPOT_URL.into_client_request()?;
    let (ws_stream, _) = connect_async(request)
        .await
        .context("failed to connect to Binance Spot")?;
    let (mut write, mut read) = ws_stream.split();

    let subscription = format!(
        r#"{{"method":"SUBSCRIBE","params":["{symbol}@depth@100ms","{symbol}@bookTicker","{symbol}@aggTrade"],"id":"{}"}}"#,
        generate_id()
    );
    write
        .send(Message::Text(subscription.into()))
        .await
        .context("failed to subscribe to Binance Spot streams")?;

    let stop_at = Instant::now() + Duration::from_secs(duration_secs);
    let stop = tokio::time::sleep_until(stop_at);
    tokio::pin!(stop);

    loop {
        tokio::select! {
            _ = &mut stop => break,
            message = read.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<SpotStream>(&text) {
                            Ok(SpotStream::Result(_)) => {}
                            Ok(SpotStream::BookTicker(book_ticker)) => {
                                stats.bbo_count += 1;
                                record_bbo_from_strings(
                                    &mut stats,
                                    &book_ticker.bid_price,
                                    &book_ticker.ask_price,
                                );
                            }
                            Ok(SpotStream::TaggedEvent(SpotMarketEventStream::DepthUpdate(_))) => {
                                stats.depth_count += 1;
                            }
                            Ok(SpotStream::TaggedEvent(SpotMarketEventStream::AggTrade(_))) => {
                                stats.trade_count += 1;
                            }
                            Err(_) => {
                                stats.parse_errors += 1;
                            }
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if let Err(error) = write.send(Message::Pong(payload)).await {
                            stats.runtime_error = Some(error.to_string());
                            break;
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Binary(_))) => {}
                    Some(Ok(Message::Frame(_))) => {}
                    Some(Ok(Message::Close(frame))) => {
                        stats.runtime_error = Some(match frame {
                            Some(frame) => frame.to_string(),
                            None => "connection closed".to_string(),
                        });
                        break;
                    }
                    Some(Err(error)) => {
                        stats.runtime_error = Some(error.to_string());
                        break;
                    }
                    None => {
                        stats.runtime_error = Some("stream ended unexpectedly".to_string());
                        break;
                    }
                }
            }
        }
    }

    Ok(stats)
}

async fn run_bybit(symbol: &str, duration_secs: u64) -> Result<TestStats> {
    let mut stats = TestStats::new(ConnectorKind::Bybit);
    let request = BYBIT_URL.into_client_request()?;
    let (ws_stream, _) = connect_async(request)
        .await
        .context("failed to connect to Bybit")?;
    let (mut write, mut read) = ws_stream.split();

    let bybit_symbol = symbol.to_uppercase();
    let subscription = format!(
        r#"{{"op":"subscribe","args":["orderbook.1.{bybit_symbol}","orderbook.50.{bybit_symbol}","publicTrade.{bybit_symbol}"]}}"#
    );
    write
        .send(Message::Text(subscription.into()))
        .await
        .context("failed to subscribe to Bybit streams")?;

    let stop_at = Instant::now() + Duration::from_secs(duration_secs);
    let stop = tokio::time::sleep_until(stop_at);
    tokio::pin!(stop);

    let mut ping_interval = interval(Duration::from_secs(15));
    ping_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = &mut stop => break,
            _ = ping_interval.tick() => {
                if let Err(error) = write.send(Message::Text(r#"{"op":"ping"}"#.into())).await {
                    stats.runtime_error = Some(error.to_string());
                    break;
                }
            }
            message = read.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<BybitPublicStreamMsg>(&text) {
                            Ok(BybitPublicStreamMsg::Op(response)) => {
                                if response.success == Some(false) || !response.fail_topics.is_empty() {
                                    stats.parse_errors += 1;
                                }
                            }
                            Ok(BybitPublicStreamMsg::Topic(stream)) => {
                                if stream.topic.starts_with("orderbook.1.") {
                                    match serde_json::from_value::<BybitOrderBook>(stream.data) {
                                        Ok(orderbook) => {
                                            stats.bbo_count += 1;
                                            record_bbo_from_levels(&mut stats, &orderbook.bids, &orderbook.asks);
                                        }
                                        Err(_) => stats.parse_errors += 1,
                                    }
                                } else if stream.topic.starts_with("orderbook.50.") {
                                    match serde_json::from_value::<BybitOrderBook>(stream.data) {
                                        Ok(_) => {
                                            stats.depth_count += 1;
                                        }
                                        Err(_) => stats.parse_errors += 1,
                                    }
                                } else if stream.topic.starts_with("publicTrade.") {
                                    match serde_json::from_value::<Vec<BybitTrade>>(stream.data) {
                                        Ok(trades) => {
                                            stats.trade_count += trades.len() as u64;
                                        }
                                        Err(_) => stats.parse_errors += 1,
                                    }
                                }
                            }
                            Err(_) => {
                                stats.parse_errors += 1;
                            }
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if let Err(error) = write.send(Message::Pong(payload)).await {
                            stats.runtime_error = Some(error.to_string());
                            break;
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Binary(_))) => {}
                    Some(Ok(Message::Frame(_))) => {}
                    Some(Ok(Message::Close(frame))) => {
                        stats.runtime_error = Some(match frame {
                            Some(frame) => frame.to_string(),
                            None => "connection closed".to_string(),
                        });
                        break;
                    }
                    Some(Err(error)) => {
                        stats.runtime_error = Some(error.to_string());
                        break;
                    }
                    None => {
                        stats.runtime_error = Some("stream ended unexpectedly".to_string());
                        break;
                    }
                }
            }
        }
    }

    Ok(stats)
}

fn selected_connectors(choice: ConnectorChoice) -> Vec<ConnectorKind> {
    match choice {
        ConnectorChoice::BinanceFutures => vec![ConnectorKind::BinanceFutures],
        ConnectorChoice::BinanceSpot => vec![ConnectorKind::BinanceSpot],
        ConnectorChoice::Bybit => vec![ConnectorKind::Bybit],
        ConnectorChoice::All => vec![
            ConnectorKind::BinanceFutures,
            ConnectorKind::BinanceSpot,
            ConnectorKind::Bybit,
        ],
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let symbol = args.symbol.to_lowercase();
    let mut results = Vec::new();

    for connector in selected_connectors(args.connector) {
        let stats = match connector {
            ConnectorKind::BinanceFutures => run_binance_futures(&symbol, args.duration).await?,
            ConnectorKind::BinanceSpot => run_binance_spot(&symbol, args.duration).await?,
            ConnectorKind::Bybit => run_bybit(&symbol, args.duration).await?,
        };
        stats.print_report(args.duration);
        results.push(stats);
    }

    println!("=== Summary ===");
    for stats in &results {
        println!(
            "  {:<15} {} ({} errors, {} crosses)",
            format!("{}:", stats.kind.display_name()),
            stats.summary_status(),
            stats.parse_errors,
            stats.bbo_crosses
        );
    }

    if results.iter().all(TestStats::passed) {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SpotStream {
    BookTicker(SpotBookTicker),
    TaggedEvent(SpotMarketEventStream),
    Result(SpotResult),
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(tag = "e")]
enum SpotMarketEventStream {
    #[serde(rename = "depthUpdate")]
    DepthUpdate(SpotDepthUpdate),
    #[serde(rename = "aggTrade")]
    AggTrade(SpotAggTrade),
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct SpotResult {
    result: Option<Value>,
    id: Value,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct SpotDepthUpdate {
    #[serde(rename = "s")]
    symbol: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct SpotBookTicker {
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "b")]
    bid_price: String,
    #[serde(rename = "a")]
    ask_price: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct SpotAggTrade {
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "p")]
    price: String,
    #[serde(rename = "q")]
    quantity: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum BybitPublicStreamMsg {
    Topic(BybitPublicStream),
    Op(BybitOpResponse),
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct BybitOpResponse {
    success: Option<bool>,
    #[serde(rename = "failTopics", default)]
    fail_topics: Vec<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct BybitPublicStream {
    topic: String,
    data: Value,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct BybitOrderBook {
    #[serde(rename = "b")]
    bids: Vec<(String, String)>,
    #[serde(rename = "a")]
    asks: Vec<(String, String)>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct BybitTrade {
    #[serde(rename = "T")]
    trade_time: i64,
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "p")]
    trade_price: String,
    #[serde(rename = "v")]
    trade_size: String,
}
