use anyhow::anyhow;
use clap::Parser;
use tokio::{self, select, signal, sync::mpsc::unbounded_channel};
use tracing::{error, info};

use crate::file::Writer;

mod binancefutures;
mod bybit;
mod error;
mod file;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path for the files where collected data will be written.
    path: String,

    /// Name of the exchange
    exchange: String,

    /// Symbols for which data will be collected.
    symbols: Vec<String>,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), anyhow::Error> {
    let args = Args::parse();

    tracing_subscriber::fmt::init();

    let (writer_tx, mut writer_rx) = unbounded_channel();

    let handle = match args.exchange.as_str() {
        "binancefutures" => {
            let streams = vec![
                "$symbol@trade",
                // "$symbol@aggTrade",
                "$symbol@bookTicker",
                "$symbol@depth@0ms",
                // "$symbol@@markPrice@1s"
            ]
            .iter()
            .map(|stream| stream.to_string())
            .collect();

            tokio::spawn(binancefutures::run_collection(
                streams,
                args.symbols,
                writer_tx,
            ))
        }
        "bybit" => {
            let topics = vec![
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
        exchange => {
            return Err(anyhow!("{exchange} is not supported."));
        }
    };

    let mut writer = Writer::new(&args.path);
    loop {
        select! {
            _ = signal::ctrl_c() => {
                info!("ctrl-c received");
                break;
            }
            r = writer_rx.recv() => match r {
                Some((recv_time, symbol, data)) => {
                    if let Err(error) = writer.write(recv_time, symbol, data) {
                        error!(?error, "write error");
                        break;
                    }
                }
                None => {
                    break;
                }
            }
        }
    }
    // let _ = handle.await;
    Ok(())
}
