use std::{
    collections::{HashMap, hash_map::Entry},
    fs::read_to_string,
    panic,
    process::exit,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use clap::Parser;
use hftbacktest::{
    live::ipc::{
        TO_ALL,
        iceoryx::{ChannelError, IceoryxBuilder},
    },
    prelude::*,
};
use iceoryx2::{
    node::{NodeBuilder, NodeState, NodeView},
    prelude::{CallbackProgression, Config, SignalHandlingMode, ipc},
};
use tokio::{
    runtime::Builder,
    select,
    signal,
    sync::{
        Notify,
        mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    },
};
use tracing::{debug, error, info};

use connector::{
    binancefutures::BinanceFutures,
    binancepapi::BinancePapi,
    binancespot::BinanceSpot,
    bybit::Bybit,
    bybit_rest::BybitRest,
    connector::{Connector, ConnectorBuilder, GetOrders, PublishEvent},
};

#[cfg(feature = "recording")]
use connector::sync::recording::{SyncConfig, SyncTask};

struct Position {
    qty: f64,
    exch_ts: i64,
}

fn run_receive_task(
    name: &str,
    tx: UnboundedSender<PublishEvent>,
    connector: &mut Box<dyn Connector>,
    registered_symbols: Arc<Mutex<Vec<String>>>,
) -> Result<(), ChannelError> {
    let node = NodeBuilder::new()
        .signal_handling_mode(SignalHandlingMode::Disabled)
        .create::<ipc::Service>()
        .map_err(|error| ChannelError::BuildError(error.to_string()))?;
    let bot_rx = IceoryxBuilder::new(name).bot(false).receiver()?;
    info!("Connector receive task started, listening for bot requests...");
    loop {
        let cycle_time = Duration::from_nanos(1000);
        match node.wait(cycle_time) {
            Ok(()) => {
                while let Some((id, ev)) = bot_rx.receive()? {
                    debug!(bot_id = id, "Received request from bot");
                    match ev {
                        LiveRequest::Order {
                            symbol: asset,
                            order,
                        } => match order.req {
                            Status::New => {
                                info!(
                                    %asset,
                                    order_id = order.order_id,
                                    side = ?order.side,
                                    price_tick = order.price_tick,
                                    qty = order.qty,
                                    "Received NEW order request from bot"
                                );
                                // Requests to the Connector submit the new order.
                                connector.submit(asset, order, tx.clone());
                            }
                            Status::Canceled => {
                                info!(
                                    %asset,
                                    order_id = order.order_id,
                                    "Received CANCEL order request from bot"
                                );
                                // Requests to the Connector cancel the order.
                                connector.cancel(asset, order, tx.clone());
                            }
                            status => {
                                error!(?status, "An invalid request was received from the bot.");
                            }
                        },
                        LiveRequest::RegisterInstrument {
                            symbol,
                            tick_size,
                            lot_size,
                        } => {
                            info!(
                                %symbol,
                                %tick_size,
                                %lot_size,
                                "Received RegisterInstrument request from bot"
                            );

                            // Auto-resolve tick_size/lot_size from exchange if either is 0.0
                            let (resolved_tick, resolved_lot) = if tick_size == 0.0 || lot_size == 0.0 {
                                match tokio::task::block_in_place(|| {
                                    tokio::runtime::Handle::current().block_on(
                                        connector.resolve_instrument_info(&symbol)
                                    )
                                }) {
                                    Some((t, l)) => {
                                        info!(
                                            %symbol,
                                            resolved_tick_size = %t,
                                            resolved_lot_size = %l,
                                            "Auto-resolved instrument info from exchange"
                                        );
                                        (
                                            if tick_size == 0.0 { t } else { tick_size },
                                            if lot_size == 0.0 { l } else { lot_size },
                                        )
                                    }
                                    None => {
                                        error!(
                                            %symbol,
                                            "Failed to auto-resolve instrument info, using 0.0 values"
                                        );
                                        (tick_size, lot_size)
                                    }
                                }
                            } else {
                                (tick_size, lot_size)
                            };

                            // Track registered symbols for cleanup
                            registered_symbols.lock().unwrap().push(symbol.clone());
                            // Makes prepare the publisher thread to also add the instrument.
                            tx.send(PublishEvent::RegisterInstrument {
                                id,
                                symbol: symbol.clone(),
                                tick_size: resolved_tick,
                                lot_size: resolved_lot,
                            })
                            .unwrap();
                            // Requests to the Connector subscribe to the necessary feeds for the
                            // instrument.
                            connector.register(symbol);
                        }
                    }
                }
            }
            Err(_error) => {
                break;
            }
        }
    }
    Ok(())
}

/// Clean up stale IPC resources from dead nodes
fn cleanup_stale_ipc_resources() {
    info!("Cleaning up stale IPC resources...");
    if let Err(e) = iceoryx2::node::Node::<ipc::Service>::list(Config::global_config(), |node_state| {
        if let NodeState::<ipc::Service>::Dead(view) = node_state {
            info!("Found dead node, cleaning up resources: {:?}", view.id());
            if let Err(e) = view.remove_stale_resources() {
                error!("Failed to cleanup stale resources: {:?}", e);
            }
        }
        CallbackProgression::Continue
    }) {
        error!("Failed to list nodes for cleanup: {:?}", e);
    }
    info!("IPC cleanup complete");
}

async fn run_publish_task(
    name: &str,
    order_manager: Arc<Mutex<dyn GetOrders>>,
    mut rx: UnboundedReceiver<PublishEvent>,
    shutdown_signal: Arc<Notify>,
) -> Result<(), ChannelError> {
    let mut depth = HashMap::new();
    let mut position: HashMap<String, Position> = HashMap::new();
    let bot_tx = IceoryxBuilder::new(name).bot(false).sender()?;

    loop {
        select! {
            _ = shutdown_signal.notified() => {
                break;
            }
            Some(msg) = rx.recv() => {
                match msg {
                    PublishEvent::RegisterInstrument {
                        id,
                        symbol,
                        tick_size,
                        lot_size,
                    } => {
                        // Sends the current state (orders, position, and market depth) to the bot that
                        // requested to add this instrument in batch mode.
                        bot_tx.send(id, &LiveEvent::BatchStart)?;

                        // Send InstrumentInfo first so bot can update tick_size/lot_size
                        // before processing depth data
                        bot_tx.send(
                            id,
                            &LiveEvent::InstrumentInfo {
                                symbol: symbol.clone(),
                                tick_size,
                                lot_size,
                            },
                        )?;

                        for order in order_manager.lock().unwrap().orders(Some(symbol.clone())) {
                            bot_tx.send(
                                id,
                                &LiveEvent::Order {
                                    symbol: symbol.clone(),
                                    order,
                                },
                            )?;
                        }

                        if let Some(position) = position.get(&symbol) {
                            bot_tx.send(
                                id,
                                &LiveEvent::Position {
                                    symbol: symbol.clone(),
                                    qty: position.qty,
                                    exch_ts: position.exch_ts,
                                },
                            )?;
                        }

                        match depth.entry(symbol) {
                            Entry::Occupied(mut entry) => {
                                let depth_: &mut FusedHashMapMarketDepth = entry.get_mut();
                                let snapshot = depth_.snapshot();
                                for event in snapshot {
                                    bot_tx.send(
                                        id,
                                        &LiveEvent::Feed {
                                            symbol: entry.key().clone(),
                                            event,
                                        },
                                    )?;
                                }
                            }
                            Entry::Vacant(entry) => {
                                entry.insert(FusedHashMapMarketDepth::new(tick_size, lot_size));
                            }
                        }

                        bot_tx.send(id, &LiveEvent::BatchEnd)?;
                    }
                    PublishEvent::LiveEvent(ev) => {
                        // The live event will only be published if the result is true.
                        for ev in handle_ev(ev, &mut depth, &mut position) {
                            bot_tx.send(TO_ALL, &ev)?;
                        }
                    }
                    PublishEvent::BatchStart(id) => {
                        bot_tx.send(id, &LiveEvent::BatchStart)?;
                    }
                    PublishEvent::BatchEnd(id) => {
                        bot_tx.send(id, &LiveEvent::BatchEnd)?;
                    }
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
fn handle_ev(
    ev: LiveEvent,
    depth: &mut HashMap<String, FusedHashMapMarketDepth>,
    position: &mut HashMap<String, Position>,
) -> Vec<LiveEvent> {
    match &ev {
        LiveEvent::Feed { symbol, event } => {
            if event.is(BUY_EVENT | DEPTH_EVENT) {
                let depth_ = {
                    match depth.get_mut(symbol) {
                        Some(d) => d,
                        None => return vec![],
                    }
                };
                return depth_
                    .update_bid_depth(event.clone())
                    .iter()
                    .map(|event| LiveEvent::Feed {
                        symbol: symbol.clone(),
                        event: event.clone(),
                    })
                    .collect();
            } else if event.is(SELL_EVENT | DEPTH_EVENT) {
                let depth_ = {
                    match depth.get_mut(symbol) {
                        Some(d) => d,
                        None => return vec![],
                    }
                };
                return depth_
                    .update_ask_depth(event.clone())
                    .iter()
                    .map(|event| LiveEvent::Feed {
                        symbol: symbol.clone(),
                        event: event.clone(),
                    })
                    .collect();
            } else if event.is(BUY_EVENT | DEPTH_BBO_EVENT) {
                let depth_ = {
                    match depth.get_mut(symbol) {
                        Some(d) => d,
                        None => return vec![],
                    }
                };
                return depth_
                    .update_best_bid(event.clone())
                    .iter()
                    .map(|event| LiveEvent::Feed {
                        symbol: symbol.clone(),
                        event: event.clone(),
                    })
                    .collect();
            } else if event.is(SELL_EVENT | DEPTH_BBO_EVENT) {
                let depth_ = {
                    match depth.get_mut(symbol) {
                        Some(d) => d,
                        None => return vec![],
                    }
                };
                return depth_
                    .update_best_ask(event.clone())
                    .iter()
                    .map(|event| LiveEvent::Feed {
                        symbol: symbol.clone(),
                        event: event.clone(),
                    })
                    .collect();
            } else if event.is(DEPTH_CLEAR_EVENT) {
                let depth_ = {
                    match depth.get_mut(symbol) {
                        Some(d) => d,
                        None => return vec![],
                    }
                };
                depth_.clear_depth(Side::None, 0.0, 0);
            }
        }
        LiveEvent::Position {
            symbol,
            qty,
            exch_ts,
        } => {
            if position.contains_key(symbol) {
                let position = position.get_mut(symbol).unwrap();
                return if *exch_ts >= position.exch_ts {
                    position.qty = *qty;
                    vec![ev]
                } else {
                    vec![]
                };
            } else {
                position.insert(
                    symbol.clone(),
                    Position {
                        qty: *qty,
                        exch_ts: *exch_ts,
                    },
                );
                return vec![ev];
            }
        }
        _ => {}
    }
    vec![ev]
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Name of the connector, used when connecting the bot to the connector.
    #[arg(short, long)]
    name: String,

    /// Connector
    /// * binancefutures: Binance USD-m Futures
    /// * binancepapi: Binance Portfolio Margin (PAPI)
    /// * binancespot: Binance Spot
    /// * bybit: Bybit Linear Futures (WebSocket Trade)
    /// * bybit_rest: Bybit Linear Futures (REST API for orders, compatible with testnet/demo)
    #[arg(short, long)]
    connector: String,

    /// Connector's configuration file path.
    #[arg(short = 'f', long)]
    config: String,

    /// IPC service name for recording events (enables position/income sync)
    #[cfg(feature = "recording")]
    #[arg(long)]
    recording_service: Option<String>,

    /// Position sync interval in seconds
    #[cfg(feature = "recording")]
    #[arg(long, default_value = "5")]
    position_sync_interval: u64,

    /// Income sync interval in seconds
    #[cfg(feature = "recording")]
    #[arg(long, default_value = "60")]
    income_sync_interval: u64,
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

    // Listen for shut down signal and notify publish task.
    let shutdown_signal = Arc::new(Notify::new());
    let shutdown_signal_ = shutdown_signal.clone();
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            // Wait for either SIGINT (CTRL+C) or SIGTERM on Unix.
            let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
                .expect("failed to install SIGTERM handler");
            select! {
                _ = signal::ctrl_c() => {},
                _ = sigterm.recv() => {},
            }
        }
        #[cfg(not(unix))]
        {
            // Non-Unix platforms only has SIGINT.
            if let Err(error) = signal::ctrl_c().await {
                error!(?error, "Couldn't listen for shutdown signal.");
            }
        }
        shutdown_signal_.notify_waiters();
    });

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
            connector.set_connector_name(&args.name);
            #[cfg(feature = "recording")]
            if let Some(ref service) = args.recording_service {
                connector.set_recording_service(service);
            }
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
        "bybit_rest" => {
            let mut connector = BybitRest::build_from(&config)
                .map_err(|error| {
                    error!(?error, "Couldn't build the BybitRest connector.");
                })
                .unwrap();
            connector.run(pub_tx.clone());
            Box::new(connector)
        }
        "binancespot" => {
            let mut connector = BinanceSpot::build_from(&config)
                .map_err(|error| {
                    error!(?error, "Couldn't build the BinanceSpot connector.");
                })
                .unwrap();
            connector.run(pub_tx.clone());
            Box::new(connector)
        }
        "binancepapi" => {
            let mut connector = BinancePapi::build_from(&config)
                .map_err(|error| {
                    error!(?error, "Couldn't build the BinancePapi connector.");
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

    // Clean up any stale IPC resources from previous runs
    cleanup_stale_ipc_resources();

    // Track registered symbols for cleanup on shutdown
    let registered_symbols: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    // Spawn position and income sync tasks if recording is enabled
    // Runs on a dedicated thread because iceoryx2 publishers aren't Send
    #[cfg(feature = "recording")]
    let _sync_handle = if let Some(ref recording_service) = args.recording_service {
        use std::collections::HashSet;
        use std::time::Duration as StdDuration;

        let sync_config = SyncConfig {
            recording_service: recording_service.clone(),
            connector_name: args.name.clone(),
            strategy_id: String::new(), // Will publish for all strategies
            position_interval: StdDuration::from_secs(args.position_sync_interval),
            income_interval: StdDuration::from_secs(args.income_sync_interval),
        };

        let connector_type = args.connector.clone();
        let config_str = config.clone();

        Some(thread::spawn(move || {
            let rt = Builder::new_current_thread().enable_all().build().unwrap();
            rt.block_on(async move {
                // Create a set to track symbols (will be empty, meaning sync all)
                let symbols: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));

                match connector_type.as_str() {
                    "binancefutures" => {
                        use connector::binancefutures::{Config as BfConfig, rest::BinanceFuturesClient};
                        use connector::sync::recording::binance::BinanceFuturesProvider;

                        let bf_config: BfConfig = toml::from_str(&config_str).unwrap();
                        let client = BinanceFuturesClient::new(
                            &bf_config.api_url,
                            &bf_config.api_key,
                            &bf_config.secret,
                        );
                        let provider = Arc::new(BinanceFuturesProvider::new(client));

                        match SyncTask::new(sync_config, symbols) {
                            Ok(sync_task) => {
                                info!("Started position and income sync for binancefutures");
                                // Run both syncs concurrently on this thread
                                let provider2 = provider.clone();
                                tokio::join!(
                                    sync_task.run_position_sync(provider),
                                    sync_task.run_income_sync(provider2)
                                );
                            }
                            Err(e) => {
                                error!("Failed to create sync task: {:?}", e);
                            }
                        }
                    }
                    "bybit" => {
                        use connector::bybit::{Config as BybitConfig, rest::BybitClient};
                        use connector::sync::recording::bybit::BybitProvider;

                        let bybit_config: BybitConfig = toml::from_str(&config_str).unwrap();
                        let client = BybitClient::new(
                            &bybit_config.api_url,
                            &bybit_config.api_key,
                            &bybit_config.secret,
                        );
                        let provider = Arc::new(BybitProvider::new(client, "linear"));

                        match SyncTask::new(sync_config, symbols) {
                            Ok(sync_task) => {
                                info!("Started position and income sync for bybit");
                                let provider2 = provider.clone();
                                tokio::join!(
                                    sync_task.run_position_sync(provider),
                                    sync_task.run_income_sync(provider2)
                                );
                            }
                            Err(e) => {
                                error!("Failed to create sync task: {:?}", e);
                            }
                        }
                    }
                    "bybit_rest" => {
                        use connector::bybit_rest::Config as BybitRestConfig;
                        use connector::bybit::rest::BybitClient;
                        use connector::sync::recording::bybit::BybitProvider;

                        let bybit_config: BybitRestConfig = toml::from_str(&config_str).unwrap();
                        let client = BybitClient::new(
                            &bybit_config.rest_url,
                            &bybit_config.api_key,
                            &bybit_config.secret,
                        );
                        let provider = Arc::new(BybitProvider::new(client, "linear"));

                        match SyncTask::new(sync_config, symbols) {
                            Ok(sync_task) => {
                                info!("Started position and income sync for bybit_rest");
                                let provider2 = provider.clone();
                                tokio::join!(
                                    sync_task.run_position_sync(provider),
                                    sync_task.run_income_sync(provider2)
                                );
                            }
                            Err(e) => {
                                error!("Failed to create sync task: {:?}", e);
                            }
                        }
                    }
                    "binancepapi" => {
                        use connector::binancepapi::{Config as PapiConfig};
                        use connector::binancepapi::rest::BinancePapiClient;
                        use connector::sync::recording::binancepapi::BinancePapiProvider;

                        let papi_config: PapiConfig = toml::from_str(&config_str).unwrap();
                        let client = BinancePapiClient::new(
                            &papi_config.api_url,
                            &papi_config.api_key,
                            &papi_config.secret,
                        );
                        let provider = Arc::new(BinancePapiProvider::new(client));

                        match SyncTask::new(sync_config, symbols) {
                            Ok(sync_task) => {
                                info!("Started position and income sync for binancepapi");
                                let provider2 = provider.clone();
                                tokio::join!(
                                    sync_task.run_position_sync(provider),
                                    sync_task.run_income_sync(provider2)
                                );
                            }
                            Err(e) => {
                                error!("Failed to create sync task: {:?}", e);
                            }
                        }
                    }
                    _ => {
                        info!("Position/income sync not implemented for connector: {}", connector_type);
                    }
                }
            });
        }))
    } else {
        None
    };

    let name = args.name.clone();
    let order_manager = connector.order_manager();
    let handle = thread::spawn(move || {
        let rt = Builder::new_current_thread().enable_all().build().unwrap();

        rt.block_on(async move {
            run_publish_task(&name, order_manager, pub_rx, shutdown_signal)
                .await
                .map_err(|error: ChannelError| {
                    error!(
                        ?error,
                        "An error occurred while sending a live event to the bots."
                    );
                })
                .unwrap();
        });
    });

    // Small delay to avoid race condition with iceoryx2 node creation.
    // When two threads create nodes simultaneously and one uses SignalHandlingMode::Disabled,
    // the node creation can fail with InternalError.
    thread::sleep(Duration::from_millis(50));

    let name = args.name;
    let symbols_for_cleanup = registered_symbols.clone();
    run_receive_task(&name, pub_tx, &mut connector, registered_symbols)
        .map_err(|error| {
            error!(
                ?error,
                "An error occurred while receiving a request from the bots."
            );
        })
        .unwrap();

    // Graceful shutdown: cancel all open orders
    info!("Shutting down connector...");
    let symbols = symbols_for_cleanup.lock().unwrap().clone();
    if !symbols.is_empty() {
        // Use block_in_place to allow blocking in async context
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                connector.shutdown(symbols).await;
            });
        });
    }

    // Clean up IPC resources
    cleanup_stale_ipc_resources();

    let _ = handle.join();
    info!("Connector shutdown complete");
}
