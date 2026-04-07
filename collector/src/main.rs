use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::anyhow;
use clap::Parser;
use iceoryx2_bb_posix::signal::SignalHandler;
use tokio::{self, sync::mpsc::unbounded_channel};
use tracing::{error, info};

use crate::file::Writer;

mod binance;
mod binancefuturescm;
mod binancefuturesum;
mod bybit;
mod error;
mod file;
mod hyperliquid;
mod ipc_collector;
mod throttler;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path for the files where collected data will be written.
    path: String,

    /// Name of the exchange (ignored when --ipc is used)
    #[arg(default_value = "")]
    exchange: String,

    /// Symbols for which data will be collected (ignored when --ipc is used)
    symbols: Vec<String>,

    /// IPC mode: subscribe to a running connector instead of direct exchange connection.
    /// Provide the connector name (e.g., "bf" for a connector started with --name bf)
    #[arg(long)]
    ipc: Option<String>,

    /// Duration in seconds to run the collector. If not specified, runs until signal.
    #[arg(long)]
    duration: Option<u64>,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();

    tracing_subscriber::fmt::init();

    let (writer_tx, mut writer_rx) = unbounded_channel();

    // Create shutdown flag for graceful shutdown
    let shutdown = Arc::new(AtomicBool::new(false));

    // Check if IPC mode is requested
    let _handle = if let Some(connector_name) = args.ipc {
        // IPC mode: subscribe to a running connector
        if args.symbols.is_empty() {
            return Err(anyhow!(
                "At least one symbol is required for IPC mode. Example: collector ./data --ipc bf btcusdt"
            ));
        }
        info!(%connector_name, symbols = ?args.symbols, "Starting in IPC mode");
        let shutdown_clone = shutdown.clone();
        tokio::spawn(ipc_collector::run_ipc_collection(
            connector_name,
            args.symbols,
            writer_tx,
            shutdown_clone,
        ))
    } else {
        // Direct connection mode: connect to exchange WebSocket
        if args.exchange.is_empty() {
            return Err(anyhow!(
                "Exchange name is required in direct mode. Use --ipc for IPC mode."
            ));
        }

        match args.exchange.as_str() {
            "binancefutures" | "binancefuturesum" => {
                let streams = [
                    "$symbol@trade",
                    "$symbol@bookTicker",
                    "$symbol@depth@0ms",
                    // "$symbol@@markPrice@1s"
                ]
                .iter()
                .map(|stream| stream.to_string())
                .collect();

                tokio::spawn(binancefuturesum::run_collection(
                    streams,
                    args.symbols,
                    writer_tx,
                ))
            }
            "binancefuturescm" => {
                let streams = [
                    "$symbol@trade",
                    "$symbol@bookTicker",
                    "$symbol@depth@0ms",
                    // "$symbol@@markPrice@1s"
                ]
                .iter()
                .map(|stream| stream.to_string())
                .collect();

                tokio::spawn(binancefuturescm::run_collection(
                    streams,
                    args.symbols,
                    writer_tx,
                ))
            }
            "binance" | "binancespot" => {
                let streams = ["$symbol@trade", "$symbol@bookTicker", "$symbol@depth@100ms"]
                    .iter()
                    .map(|stream| stream.to_string())
                    .collect();

                tokio::spawn(binance::run_collection(streams, args.symbols, writer_tx))
            }
            "bybit" => {
                let topics = [
                    "orderbook.1.$symbol",
                    "orderbook.50.$symbol",
                    "orderbook.500.$symbol",
                    "publicTrade.$symbol",
                ]
                .iter()
                .map(|topic| topic.to_string())
                .collect();

                tokio::spawn(bybit::run_collection(topics, args.symbols, writer_tx))
            }
            "hyperliquid" => {
                let subscriptions = ["trades", "l2Book", "bbo"]
                    .iter()
                    .map(|sub| sub.to_string())
                    .collect();

                tokio::spawn(hyperliquid::run_collection(
                    subscriptions,
                    args.symbols,
                    writer_tx,
                ))
            }
            exchange => {
                return Err(anyhow!("{exchange} is not supported."));
            }
        }
    };

    let mut writer = Writer::new(&args.path);

    info!("Press Ctrl+C or send SIGTERM to stop");

    // Optional: Set up duration-based shutdown
    let start_time = std::time::Instant::now();
    let duration_limit = args.duration.map(|d| std::time::Duration::from_secs(d));
    if let Some(d) = duration_limit {
        info!("Collector will run for {} seconds", d.as_secs());
    }

    loop {
        // Check duration limit
        if let Some(limit) = duration_limit {
            if start_time.elapsed() >= limit {
                info!("Duration limit reached, shutting down...");
                shutdown.store(true, Ordering::SeqCst);
            }
        }
        // Check shutdown flag or iceoryx2's signal handler
        if shutdown.load(Ordering::SeqCst) || SignalHandler::termination_requested() {
            info!("Shutdown requested, exiting main loop...");
            break;
        }

        // Use timeout to allow checking shutdown flag
        match tokio::time::timeout(tokio::time::Duration::from_millis(100), writer_rx.recv()).await
        {
            Ok(Some((recv_time, symbol, data))) => {
                if let Err(error) = writer.write(recv_time, symbol, data) {
                    error!(?error, "write error");
                    break;
                }
            }
            Ok(None) => {
                info!("Writer channel closed");
                break;
            }
            Err(_) => {
                // Timeout, continue to check shutdown flag
                continue;
            }
        }
    }

    // Give the IPC collector time to stop gracefully
    info!("Waiting for collector to stop...");
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Explicitly flush all files before exiting
    writer.flush();
    info!("Collector shutdown complete");
    Ok(())
}
