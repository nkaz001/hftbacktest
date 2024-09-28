use std::{
    collections::{hash_map::Entry, HashMap},
    fs::read_to_string,
    process::exit,
    time::Duration,
};

use clap::Parser;
use hftbacktest::{
    live::ipc::{IceoryxReceiver, IceoryxSender, PubSubError, TO_ALL},
    prelude::*,
    types::Request,
};
use iceoryx2::{iox2::Iox2, prelude::Iox2Event};
use tokio::{
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    task::LocalSet,
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
) -> Result<(), anyhow::Error> {
    let bot_rx = IceoryxReceiver::<Request>::build(name)?;
    loop {
        let cycle_time = Duration::from_nanos(1000);
        match Iox2::wait(cycle_time) {
            Iox2Event::Tick => {
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
                            connector.add(symbol, tick_size, id, tx.clone());
                        }
                    }
                }
            }
            Iox2Event::TerminationRequest | Iox2Event::InterruptSignal => {
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
    // The size is constrained by the buffer size of the IPC payload.
    // todo: check the right size.
    const LIVE_EVENT_CHUNK_SIZE: usize = 10;

    let mut depth = HashMap::new();
    let bot_tx = IceoryxSender::<LiveEvent>::build(name)?;

    while let Some(msg) = rx.recv().await {
        match msg {
            PublishMessage::AddInstrument {
                id,
                symbol,
                tick_size,
            } => match depth.entry(symbol) {
                Entry::Occupied(mut entry) => {
                    let depth_: &mut FusedHashMapMarketDepth = entry.get_mut();
                    let snapshot = depth_.snapshot();
                    for chunk in snapshot.chunks(LIVE_EVENT_CHUNK_SIZE).map(|s| s.into()) {
                        let ev = LiveEvent::FeedBatch {
                            symbol: entry.key().clone(),
                            events: chunk,
                        };
                        bot_tx.send(id, &ev)?;
                    }
                }
                Entry::Vacant(entry) => {
                    entry.insert(FusedHashMapMarketDepth::new(tick_size));
                }
            },
            PublishMessage::LiveEvent(ev) => {
                // The live event will only be published if the result is true.
                if handle_ev(&ev, &mut depth) {
                    bot_tx.send(TO_ALL, &ev)?;
                }
            }
            PublishMessage::LiveEventsWithId { id, events } => {
                // This occurs when an order or position snapshot needs to be published by adding
                // the instrument.
                for ev in events {
                    bot_tx.send(id, &ev)?;
                }
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
fn handle_ev(lev: &LiveEvent, depth: &mut HashMap<String, FusedHashMapMarketDepth>) -> bool {
    if let LiveEvent::Feed { symbol, event } = lev {
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
    true
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Name of the connector
    connector: String,

    /// Connector's configuration file path.
    config: String,
}

#[tokio::main]
async fn main() {
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

    let local = LocalSet::new();
    let connector_name = args.connector.clone();
    let handle = local.spawn_local(async move {
        run_publish_task(&connector_name, pub_rx)
            .await
            .map_err(|error: PubSubError| {
                error!(
                    ?error,
                    "An error occurred while sending a live event to the bots."
                );
            })
            .unwrap();
    });

    let connector_name = args.connector;
    run_receive_task(&connector_name, pub_tx, &mut connector)
        .map_err(|error| {
            error!(
                ?error,
                "An error occurred while receiving a request from the bots."
            );
        })
        .unwrap();
    let _ = handle.await;
}
