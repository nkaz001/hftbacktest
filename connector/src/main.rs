use std::{
    collections::{hash_map::Entry, HashMap},
    fs::read_to_string,
    panic,
    process::exit,
    thread,
    time::Duration,
};

use clap::Parser;
use hftbacktest::{
    live::ipc::{IceoryxBuilder, LiveEventExt, PubSubError, TO_ALL},
    prelude::*,
    types::Request,
};
use iceoryx2::{
    node::NodeBuilder,
    prelude::{ipc, NodeEvent},
};
use tokio::{
    runtime::Builder,
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
};
use tracing::error;

use crate::{
    binancefutures::BinanceFutures,
    bybit::Bybit,
    connector::{Connector, ConnectorBuilder, PublishMessage},
    fuse::FusedHashMapMarketDepth,
};

#[cfg(feature = "binancefutures")]
pub mod binancefutures;
#[cfg(feature = "bybit")]
pub mod bybit;

mod connector;
mod fuse;
mod utils;

fn run_receive_task(
    name: &str,
    tx: UnboundedSender<PublishMessage>,
    connector: &mut Box<dyn Connector>,
) -> Result<(), PubSubError> {
    let node = NodeBuilder::new()
        .create::<ipc::Service>()
        .map_err(|error| PubSubError::BuildError(error.to_string()))?;
    let bot_rx = IceoryxBuilder::new(name).bot(false).receiver()?;
    loop {
        let cycle_time = Duration::from_nanos(1000);
        match node.wait(cycle_time) {
            NodeEvent::Tick => {
                while let Some((id, ev)) = bot_rx.receive()? {
                    match ev {
                        Request::Order {
                            symbol: asset,
                            order,
                        } => match order.req {
                            Status::New => {
                                // Requests to the Connector submit the new order.
                                connector.submit(asset, order, tx.clone());
                            }
                            Status::Canceled => {
                                // Requests to the Connector cancel the order.
                                connector.cancel(asset, order, tx.clone());
                            }
                            status => {
                                error!(?status, "An invalid request was received from the bot.");
                            }
                        },
                        Request::AddInstrument { symbol, tick_size } => {
                            // Makes prepare the publisher thread to also add the instrument.
                            tx.send(PublishMessage::AddInstrument {
                                id,
                                symbol: symbol.clone(),
                                tick_size,
                            })
                            .unwrap();
                            // Requests to the Connector subscribe to the necessary feeds for the
                            // instrument.
                            connector.add(symbol, id, tx.clone());
                        }
                    }
                }
            }
            NodeEvent::TerminationRequest | NodeEvent::InterruptSignal => {
                break;
            }
        }
    }
    Ok(())
}

async fn run_publish_task(
    name: &str,
    mut rx: UnboundedReceiver<PublishMessage>,
) -> Result<(), PubSubError> {
    let mut depth = HashMap::new();
    let mut position = HashMap::new();
    let bot_tx = IceoryxBuilder::new(name).bot(false).sender()?;

    while let Some(msg) = rx.recv().await {
        match msg {
            PublishMessage::AddInstrument {
                id,
                symbol,
                tick_size,
            } => {
                if let Some(qty) = position.get(&symbol) {
                    bot_tx.send(
                        id,
                        &LiveEventExt::Batch(LiveEvent::Position {
                            symbol: symbol.clone(),
                            qty: *qty,
                        }),
                    )?;
                }

                match depth.entry(symbol) {
                    Entry::Occupied(mut entry) => {
                        let depth_: &mut FusedHashMapMarketDepth = entry.get_mut();
                        let snapshot = depth_.snapshot();
                        for event in snapshot {
                            bot_tx.send(
                                id,
                                &LiveEventExt::Batch(LiveEvent::Feed {
                                    symbol: entry.key().clone(),
                                    event,
                                }),
                            )?;
                        }
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(FusedHashMapMarketDepth::new(tick_size));
                    }
                }
            }
            PublishMessage::LiveEvent(ev) => {
                // The live event will only be published if the result is true.
                if handle_ev(&ev, &mut depth, &mut position) {
                    bot_tx.send(TO_ALL, &LiveEventExt::Normal(ev))?;
                }
            }
            PublishMessage::BatchLiveEvent(ev) => {
                // The live event will only be published if the result is true.
                if handle_ev(&ev, &mut depth, &mut position) {
                    bot_tx.send(TO_ALL, &LiveEventExt::Batch(ev))?;
                }
            }
            PublishMessage::LiveEventsWithId { id, events } => {
                // This occurs when an order or position snapshot needs to be published by adding
                // the instrument.
                for ev in events {
                    bot_tx.send(id, &LiveEventExt::Batch(ev))?;
                }
            }
            PublishMessage::EndOfBatch(id) => {
                bot_tx.send(id, &LiveEventExt::EndOfBatch)?;
            }
        }
    }
    Ok(())
}

/// Maintains the market depth for all added instruments, allowing another bot to request the same
/// instrument and publishing the market depth snapshot, and fuses the market depth from different
/// streams, such as L1 or L2 with varying depths and update frequencies, to provide the most
/// granular and frequent updates.
///
/// Returns true when the received live event needs to be published; otherwise, it does not.
/// For example, publication is unnecessary if the received market depth data is outdated by more
/// recent data from a different stream due to fusion.
fn handle_ev(
    ev: &LiveEvent,
    depth: &mut HashMap<String, FusedHashMapMarketDepth>,
    position: &mut HashMap<String, f64>,
) -> bool {
    match ev {
        LiveEvent::Feed { symbol, event } => {
            if event.is(BUY_EVENT | DEPTH_EVENT) {
                let depth_ = depth.get_mut(symbol).unwrap();
                return depth_.update_bid_depth(event.px, event.qty, event.exch_ts);
            } else if event.is(SELL_EVENT | DEPTH_EVENT) {
                let depth_ = depth.get_mut(symbol).unwrap();
                return depth_.update_ask_depth(event.px, event.qty, event.exch_ts);
            } else if event.is(BUY_EVENT | DEPTH_BBO_EVENT) {
                let depth_ = depth.get_mut(symbol).unwrap();
                return depth_.update_best_bid(event.px, event.qty, event.exch_ts);
            } else if event.is(SELL_EVENT | DEPTH_BBO_EVENT) {
                let depth_ = depth.get_mut(symbol).unwrap();
                return depth_.update_best_ask(event.px, event.qty, event.exch_ts);
            } else if event.is(DEPTH_CLEAR_EVENT) {
                let depth_ = depth.get_mut(symbol).unwrap();
                depth_.clear_depth(Side::None, 0.0);
            }
        }
        LiveEvent::Position { symbol, qty } => {
            if position.contains_key(symbol) {
                *position.get_mut(symbol).unwrap() = *qty;
            } else {
                position.insert(symbol.clone(), *qty);
            }
        }
        _ => {}
    }
    true
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Name of the connector, used when connecting the bot to the connector.
    name: String,

    /// Connector
    /// * binancefutures: Binance USD-m Futures
    /// * bybit: Bybit Linear Futures
    connector: String,

    /// Connector's configuration file path.
    config: String,
}

#[tokio::main]
async fn main() {
    // Ensures that the main thread will terminate if any of its child threads panics.
    let orig_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        orig_hook(panic_info);
        exit(1);
    }));

    let args = Args::parse();

    tracing_subscriber::fmt::init();

    let (pub_tx, pub_rx) = unbounded_channel();

    let config = read_to_string(&args.config)
        .map_err(|error| {
            error!(
                ?error,
                config = args.config,
                "An error occurred while reading the configuration file."
            );
        })
        .unwrap();

    let mut connector: Box<dyn Connector> = match args.connector.as_str() {
        "binancefutures" => {
            let mut connector = BinanceFutures::build_from(&config)
                .map_err(|error| {
                    error!(?error, "Couldn't build the BinanceFutures connector.");
                })
                .unwrap();
            connector.run(pub_tx.clone());
            Box::new(connector)
        }
        "bybit" => {
            let mut connector = Bybit::build_from(&config)
                .map_err(|error| {
                    error!(?error, "Couldn't build the Bybit connector.");
                })
                .unwrap();
            connector.run(pub_tx.clone());
            Box::new(connector)
        }
        connector => {
            error!(%connector, "This connector doesn't exist.");
            exit(1);
        }
    };

    let name = args.name.clone();
    let handle = thread::spawn(move || {
        let rt = Builder::new_current_thread().enable_all().build().unwrap();

        rt.block_on(async move {
            run_publish_task(&name, pub_rx)
                .await
                .map_err(|error: PubSubError| {
                    error!(
                        ?error,
                        "An error occurred while sending a live event to the bots."
                    );
                })
                .unwrap();
        });
    });

    let name = args.name;
    run_receive_task(&name, pub_tx, &mut connector)
        .map_err(|error| {
            error!(
                ?error,
                "An error occurred while receiving a request from the bots."
            );
        })
        .unwrap();
    let _ = handle.join();
}
