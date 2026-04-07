//! Integration test for bookTicker + dual WebSocket connection architecture.
//!
//! Connects two WebSocket connections (public + market) to Binance Futures,
//! subscribes to depth + bookTicker on public, trade on market,
//! and verifies all three event types are received and parsed correctly.
//!
//! Run with: cargo run --release --example bookticker_test
//!
//! Options:
//!   --symbol <symbol>   Trading pair (default: btcusdt)
//!   --duration <secs>   How long to run (default: 10)
//!   --url <url>         WebSocket base URL (default: wss://fstream.binancefuture.com/ws)

use std::time::Duration;

use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use tokio::time::timeout;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use tracing::{Level, error, info};

use connector::binancefutures::msg::stream::{EventStream, Stream};

#[derive(Parser, Debug)]
#[command(version, about = "Test bookTicker + dual WebSocket connections")]
struct Args {
    #[arg(short, long, default_value = "btcusdt")]
    symbol: String,

    #[arg(short, long, default_value = "10")]
    duration: u64,

    /// Base WebSocket URL (used for both public + market if no split)
    #[arg(long, default_value = "wss://fstream.binancefuture.com/ws")]
    url: String,

    /// Public stream URL (depth + bookTicker). Falls back to --url.
    #[arg(long)]
    public_url: Option<String>,

    /// Market stream URL (trade). Falls back to --url.
    #[arg(long)]
    market_url: Option<String>,
}

fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("{ts}")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let args = Args::parse();
    let symbol = args.symbol.to_lowercase();
    let public_url = args.public_url.as_deref().unwrap_or(&args.url);
    let market_url = args.market_url.as_deref().unwrap_or(&args.url);

    info!("=== bookTicker + Dual WS Test ===");
    info!("Symbol: {symbol}");
    info!("Public URL: {public_url}");
    info!("Market URL: {market_url}");
    info!("Duration: {}s", args.duration);

    // Connect public stream (depth + bookTicker)
    let pub_request = public_url.into_client_request()?;
    let (pub_ws, _) = connect_async(pub_request).await?;
    let (mut pub_write, mut pub_read) = pub_ws.split();
    info!("Connected to public stream");

    // Connect market stream (trade)
    let mkt_request = market_url.into_client_request()?;
    let (mkt_ws, _) = connect_async(mkt_request).await?;
    let (mut mkt_write, mut mkt_read) = mkt_ws.split();
    info!("Connected to market stream");

    // Subscribe depth + bookTicker on public
    let id = generate_id();
    pub_write
        .send(Message::Text(
            format!(
                r#"{{"method":"SUBSCRIBE","params":["{symbol}@depth@0ms","{symbol}@bookTicker"],"id":"{id}"}}"#
            )
            .into(),
        ))
        .await?;

    // Subscribe trade on market
    let id = generate_id();
    mkt_write
        .send(Message::Text(
            format!(r#"{{"method":"SUBSCRIBE","params":["{symbol}@trade"],"id":"{id}"}}"#).into(),
        ))
        .await?;

    info!("Subscriptions sent, listening for events...");

    let mut depth_count = 0u64;
    let mut bookticker_count = 0u64;
    let mut trade_count = 0u64;
    let mut parse_errors = 0u64;
    let mut last_bid = 0.0f64;
    let mut last_ask = 0.0f64;

    let test_duration = Duration::from_secs(args.duration);
    let start = std::time::Instant::now();

    while start.elapsed() < test_duration {
        tokio::select! {
            msg = timeout(Duration::from_secs(5), pub_read.next()) => {
                match msg {
                    Ok(Some(Ok(Message::Text(text)))) => {
                        match serde_json::from_str::<Stream>(&text) {
                            Ok(Stream::EventStream(EventStream::DepthUpdate(_))) => {
                                depth_count += 1;
                            }
                            Ok(Stream::EventStream(EventStream::BookTicker(bt))) => {
                                bookticker_count += 1;
                                if let (Ok(bid), Ok(ask)) = (bt.bid_price.parse::<f64>(), bt.ask_price.parse::<f64>()) {
                                    last_bid = bid;
                                    last_ask = ask;
                                    if bookticker_count <= 3 || bookticker_count % 500 == 0 {
                                        info!(
                                            "bookTicker #{}: bid={:.2} ask={:.2} spread={:.2} (T={})",
                                            bookticker_count, bid, ask, ask - bid, bt.transaction_time
                                        );
                                    }
                                }
                            }
                            Ok(Stream::Result(_)) => {}
                            Ok(Stream::EventStream(other)) => {
                                info!("Unexpected public event: {:?}", other);
                            }
                            Err(e) => {
                                parse_errors += 1;
                                if parse_errors <= 3 {
                                    error!("Public parse error: {e} — {text}");
                                }
                            }
                        }
                    }
                    Ok(Some(Ok(Message::Ping(data)))) => {
                        let _ = pub_write.send(Message::Pong(data)).await;
                    }
                    Ok(Some(Ok(_))) => {}
                    Ok(Some(Err(e))) => {
                        error!("Public WS error: {e:?}");
                        break;
                    }
                    Ok(None) => {
                        error!("Public stream ended");
                        break;
                    }
                    Err(_) => {} // timeout, try market
                }
            }
            msg = timeout(Duration::from_secs(5), mkt_read.next()) => {
                match msg {
                    Ok(Some(Ok(Message::Text(text)))) => {
                        match serde_json::from_str::<Stream>(&text) {
                            Ok(Stream::EventStream(EventStream::Trade(t))) => {
                                trade_count += 1;
                                if trade_count <= 3 || trade_count % 100 == 0 {
                                    info!("trade #{}: px={} qty={} side={}",
                                        trade_count, t.price, t.qty,
                                        if t.is_the_buyer_the_market_maker { "SELL" } else { "BUY" }
                                    );
                                }
                            }
                            Ok(Stream::Result(_)) => {}
                            Ok(Stream::EventStream(other)) => {
                                info!("Unexpected market event: {:?}", other);
                            }
                            Err(e) => {
                                parse_errors += 1;
                                if parse_errors <= 3 {
                                    error!("Market parse error: {e} — {text}");
                                }
                            }
                        }
                    }
                    Ok(Some(Ok(Message::Ping(data)))) => {
                        let _ = mkt_write.send(Message::Pong(data)).await;
                    }
                    Ok(Some(Ok(_))) => {}
                    Ok(Some(Err(e))) => {
                        error!("Market WS error: {e:?}");
                        break;
                    }
                    Ok(None) => {
                        error!("Market stream ended");
                        break;
                    }
                    Err(_) => {} // timeout
                }
            }
        }
    }

    let elapsed = start.elapsed().as_secs_f64();
    info!("=== Results ({elapsed:.1}s) ===");
    info!(
        "depth:      {depth_count:>6} ({:.1}/s)",
        depth_count as f64 / elapsed
    );
    info!(
        "bookTicker: {bookticker_count:>6} ({:.1}/s)",
        bookticker_count as f64 / elapsed
    );
    info!(
        "trade:      {trade_count:>6} ({:.1}/s)",
        trade_count as f64 / elapsed
    );
    info!("errors:     {parse_errors:>6}");
    if last_bid > 0.0 {
        info!(
            "Last BBO: bid={last_bid:.2} ask={last_ask:.2} spread={:.2}",
            last_ask - last_bid
        );
    }

    if bookticker_count > 0 && depth_count > 0 && trade_count > 0 && parse_errors == 0 {
        info!("PASSED: All event types received on correct connections");
        Ok(())
    } else {
        error!(
            "FAILED: Missing events (bt={bookticker_count}, depth={depth_count}, trade={trade_count}, errors={parse_errors})"
        );
        std::process::exit(1);
    }
}
