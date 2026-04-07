use std::time::{Duration, Instant};

use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use hftbacktest::{live::ipc::TO_ALL, prelude::*};
use tokio::{
    select,
    sync::{
        broadcast::{Receiver, error::RecvError},
        mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    },
    time,
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use tracing::{debug, error, info, warn};

use crate::{
    binancefutures::{
        BinanceFuturesError,
        msg::{
            rest, stream,
            stream::{EventStream, Stream},
        },
        orderbookmanager::{DepthEvent, OrderBookManager},
        rest::BinanceFuturesClient,
    },
    connector::PublishEvent,
    utils::{generate_rand_string, parse_depth, parse_px_qty_tup},
};

pub struct MarketDataStream {
    client: BinanceFuturesClient,
    ev_tx: UnboundedSender<PublishEvent>,
    symbol_rx: Receiver<String>,
    /// Order book manager for proper synchronization
    order_book_manager: OrderBookManager,
    /// Channel for receiving REST snapshot responses
    rest_tx: UnboundedSender<(String, rest::Depth)>,
    rest_rx: UnboundedReceiver<(String, rest::Depth)>,
    /// Channel for receiving snapshot failure notifications
    rest_fail_tx: UnboundedSender<String>,
    rest_fail_rx: UnboundedReceiver<String>,
}

impl MarketDataStream {
    pub fn new(
        client: BinanceFuturesClient,
        ev_tx: UnboundedSender<PublishEvent>,
        symbol_rx: Receiver<String>,
    ) -> Self {
        let (rest_tx, rest_rx) = unbounded_channel::<(String, rest::Depth)>();
        let (rest_fail_tx, rest_fail_rx) = unbounded_channel::<String>();
        Self {
            client,
            ev_tx,
            symbol_rx,
            order_book_manager: OrderBookManager::new(),
            rest_tx,
            rest_rx,
            rest_fail_tx,
            rest_fail_rx,
        }
    }

    /// Request depth snapshot for a symbol with retry logic.
    /// Retries up to 3 times with exponential backoff (1s, 2s, 4s).
    /// On final failure, sends symbol to rest_fail_tx for orderbookmanager notification.
    fn request_snapshot(&mut self, symbol: String) {
        self.order_book_manager.mark_snapshot_requested(&symbol);

        let client = self.client.clone();
        let rest_tx = self.rest_tx.clone();
        let rest_fail_tx = self.rest_fail_tx.clone();

        tokio::spawn(async move {
            const MAX_RETRIES: u32 = 3;

            for attempt in 0..MAX_RETRIES {
                if attempt > 0 {
                    let delay = Duration::from_secs(1 << attempt); // 2s, 4s
                    warn!(symbol = %symbol, attempt = attempt + 1, delay_secs = delay.as_secs(), "Retrying snapshot request");
                    tokio::time::sleep(delay).await;
                }

                debug!(symbol = %symbol, attempt = attempt + 1, "Requesting depth snapshot via REST");
                match client.get_depth(&symbol).await {
                    Ok(depth) => {
                        if let Err(e) = rest_tx.send((symbol.clone(), depth)) {
                            error!(symbol = %symbol, error = ?e, "Failed to send snapshot to channel");
                        }
                        return; // Success
                    }
                    Err(error) => {
                        error!(?error, %symbol, attempt = attempt + 1, "Couldn't get depth snapshot via REST");
                    }
                }
            }

            // All retries exhausted — notify failure
            error!(symbol = %symbol, "Depth snapshot request failed after {MAX_RETRIES} attempts");
            let _ = rest_fail_tx.send(symbol);
        });
    }

    /// Process a WebSocket depth update event
    fn process_depth_update(&mut self, data: stream::Depth) {
        let symbol = data.symbol.clone();

        // Use order book manager to handle synchronization
        let (should_process, should_request_snapshot) =
            self.order_book_manager.on_depth_update(&data);

        if should_request_snapshot {
            self.request_snapshot(symbol.clone());
        }

        if should_process {
            // Process the depth update normally
            self.send_depth_events(&symbol, data.transaction_time, &data.bids, &data.asks);
        }
    }

    /// Process a REST snapshot response
    fn process_snapshot(&mut self, symbol: String, snapshot: rest::Depth) {
        // Get events to process from order book manager
        let events = self.order_book_manager.on_snapshot(&symbol, &snapshot);

        if events.is_empty() {
            // Need to request snapshot again (gap detected)
            warn!(symbol = %symbol, "Gap detected during snapshot processing, requesting new snapshot");
            self.request_snapshot(symbol);
            return;
        }

        // Process all events
        for event in events {
            match event {
                DepthEvent::Snapshot {
                    symbol,
                    transaction_time,
                    bids,
                    asks,
                    last_update_id,
                } => {
                    info!(
                        symbol = %symbol,
                        last_update_id = last_update_id,
                        "Applying depth snapshot"
                    );
                    // For snapshot, we should clear existing depth first
                    // Send DEPTH_CLEAR_EVENT before the snapshot data
                    self.ev_tx.send(PublishEvent::BatchStart(TO_ALL)).unwrap();

                    // Send clear event for both sides
                    self.ev_tx
                        .send(PublishEvent::LiveEvent(LiveEvent::Feed {
                            symbol: symbol.clone(),
                            event: Event {
                                ev: LOCAL_DEPTH_CLEAR_EVENT,
                                exch_ts: transaction_time * 1_000_000,
                                local_ts: Utc::now().timestamp_nanos_opt().unwrap(),
                                order_id: 0,
                                px: 0.0,
                                qty: 0.0,
                                ival: 0,
                                fval: 0.0,
                            },
                        }))
                        .unwrap();

                    self.ev_tx.send(PublishEvent::BatchEnd(TO_ALL)).unwrap();

                    // Now send the snapshot data
                    self.send_depth_events(&symbol, transaction_time, &bids, &asks);
                }
                DepthEvent::Update {
                    symbol,
                    transaction_time,
                    bids,
                    asks,
                    first_update_id: _,
                    last_update_id: _,
                } => {
                    self.send_depth_events(&symbol, transaction_time, &bids, &asks);
                }
            }
        }

        // Check if we need another snapshot (resync marked during processing)
        if self.order_book_manager.needs_snapshot(&symbol) {
            warn!(symbol = %symbol, "Resync needed after processing, requesting new snapshot");
            self.request_snapshot(symbol);
        }
    }

    /// Send depth events to the event channel
    fn send_depth_events(
        &self,
        symbol: &str,
        transaction_time: i64,
        bids: &[(String, String)],
        asks: &[(String, String)],
    ) {
        match parse_depth(bids.to_vec(), asks.to_vec()) {
            Ok((parsed_bids, parsed_asks)) => {
                self.ev_tx.send(PublishEvent::BatchStart(TO_ALL)).unwrap();

                for (px, qty) in parsed_bids {
                    self.ev_tx
                        .send(PublishEvent::LiveEvent(LiveEvent::Feed {
                            symbol: symbol.to_string(),
                            event: Event {
                                ev: LOCAL_BID_DEPTH_EVENT,
                                exch_ts: transaction_time * 1_000_000,
                                local_ts: Utc::now().timestamp_nanos_opt().unwrap(),
                                order_id: 0,
                                px,
                                qty,
                                ival: 0,
                                fval: 0.0,
                            },
                        }))
                        .unwrap();
                }

                for (px, qty) in parsed_asks {
                    self.ev_tx
                        .send(PublishEvent::LiveEvent(LiveEvent::Feed {
                            symbol: symbol.to_string(),
                            event: Event {
                                ev: LOCAL_ASK_DEPTH_EVENT,
                                exch_ts: transaction_time * 1_000_000,
                                local_ts: Utc::now().timestamp_nanos_opt().unwrap(),
                                order_id: 0,
                                px,
                                qty,
                                ival: 0,
                                fval: 0.0,
                            },
                        }))
                        .unwrap();
                }

                self.ev_tx.send(PublishEvent::BatchEnd(TO_ALL)).unwrap();
            }
            Err(error) => {
                error!(?error, symbol = %symbol, "Couldn't parse depth data.");
            }
        }
    }

    /// Process a trade event
    fn process_trade(&self, data: stream::Trade) {
        match parse_px_qty_tup(data.price, data.qty) {
            Ok((px, qty)) => {
                if data.type_ != "MARKET" {
                    return;
                }
                self.ev_tx
                    .send(PublishEvent::LiveEvent(LiveEvent::Feed {
                        symbol: data.symbol,
                        event: Event {
                            ev: {
                                if data.is_the_buyer_the_market_maker {
                                    LOCAL_SELL_TRADE_EVENT
                                } else {
                                    LOCAL_BUY_TRADE_EVENT
                                }
                            },
                            exch_ts: data.transaction_time * 1_000_000,
                            local_ts: Utc::now().timestamp_nanos_opt().unwrap(),
                            order_id: 0,
                            px,
                            qty,
                            ival: 0,
                            fval: 0.0,
                        },
                    }))
                    .unwrap();
            }
            Err(e) => {
                error!(error = ?e, "Couldn't parse trade stream.");
            }
        }
    }

    /// Process a bookTicker event (real-time BBO updates)
    fn process_book_ticker(&self, data: stream::BookTicker) {
        // Skip BBO events for symbols that haven't completed depth synchronization.
        // During init/resync, emitting BBO before the snapshot would leave stale state
        // that gets overwritten once the snapshot arrives.
        if self.order_book_manager.needs_snapshot(&data.symbol) {
            return;
        }
        match parse_px_qty_tup(data.bid_price, data.bid_qty) {
            Ok((bid_px, bid_qty)) => match parse_px_qty_tup(data.ask_price, data.ask_qty) {
                Ok((ask_px, ask_qty)) => {
                    let exch_ts = data.transaction_time * 1_000_000;
                    let local_ts = Utc::now().timestamp_nanos_opt().unwrap();

                    self.ev_tx.send(PublishEvent::BatchStart(TO_ALL)).unwrap();

                    self.ev_tx
                        .send(PublishEvent::LiveEvent(LiveEvent::Feed {
                            symbol: data.symbol.clone(),
                            event: Event {
                                ev: LOCAL_BID_DEPTH_BBO_EVENT,
                                exch_ts,
                                local_ts,
                                order_id: 0,
                                px: bid_px,
                                qty: bid_qty,
                                ival: 0,
                                fval: 0.0,
                            },
                        }))
                        .unwrap();

                    self.ev_tx
                        .send(PublishEvent::LiveEvent(LiveEvent::Feed {
                            symbol: data.symbol,
                            event: Event {
                                ev: LOCAL_ASK_DEPTH_BBO_EVENT,
                                exch_ts,
                                local_ts,
                                order_id: 0,
                                px: ask_px,
                                qty: ask_qty,
                                ival: 0,
                                fval: 0.0,
                            },
                        }))
                        .unwrap();

                    self.ev_tx.send(PublishEvent::BatchEnd(TO_ALL)).unwrap();
                }
                Err(e) => {
                    error!(error = ?e, "Couldn't parse bookTicker ask.");
                }
            },
            Err(e) => {
                error!(error = ?e, "Couldn't parse bookTicker bid.");
            }
        }
    }

    /// Process incoming WebSocket message
    fn process_message(&mut self, stream: EventStream) {
        match stream {
            EventStream::DepthUpdate(data) => {
                self.process_depth_update(data);
            }
            EventStream::Trade(data) => {
                self.process_trade(data);
            }
            EventStream::BookTicker(data) => {
                self.process_book_ticker(data);
            }
            _ => unreachable!(),
        }
    }

    pub async fn connect(&mut self, url: &str) -> Result<(), BinanceFuturesError> {
        let request = url.into_client_request()?;
        let (ws_stream, _) = connect_async(request).await?;
        let (mut write, mut read) = ws_stream.split();
        let mut ping_checker = time::interval(Duration::from_secs(10));
        let mut health_checker = time::interval(Duration::from_secs(10));
        let mut last_ping = Instant::now();

        info!("Connected to market data stream: {}", url);

        // Mark all order books for resync on new connection
        self.order_book_manager.mark_all_resync();

        loop {
            select! {
                // Handle REST snapshot responses
                Some((symbol, data)) = self.rest_rx.recv() => {
                    self.process_snapshot(symbol, data);
                }
                // Handle REST snapshot failure notifications
                Some(symbol) = self.rest_fail_rx.recv() => {
                    self.order_book_manager.mark_snapshot_failed(&symbol);
                }
                // Periodic health check: retry snapshots for stuck symbols
                _ = health_checker.tick() => {
                    let symbols = self.order_book_manager.symbols_needing_snapshot_retry();
                    for symbol in symbols {
                        warn!(symbol = %symbol, "Health check: retrying snapshot for stuck symbol");
                        self.request_snapshot(symbol);
                    }
                }
                // Ping timeout check
                _ = ping_checker.tick() => {
                    if last_ping.elapsed() > Duration::from_secs(300) {
                        warn!("Ping timeout.");
                        return Err(BinanceFuturesError::ConnectionInterrupted);
                    }
                }
                // Handle new symbol subscriptions
                msg = self.symbol_rx.recv() => match msg {
                    Ok(symbol) => {
                        // Initialize order book for this symbol
                        self.order_book_manager.start_init(&symbol);

                        let id = generate_rand_string(16);
                        write.send(Message::Text(format!(r#"{{
                            "method": "SUBSCRIBE",
                            "params": [
                                "{symbol}@trade",
                                "{symbol}@depth@0ms",
                                "{symbol}@bookTicker"
                            ],
                            "id": "{id}"
                        }}"#).into())).await?;

                        info!(symbol = %symbol, "Subscribed to trade, depth and bookTicker streams");
                    }
                    Err(RecvError::Closed) => {
                        info!("Symbol subscription channel closed");
                        return Ok(());
                    }
                    Err(RecvError::Lagged(num)) => {
                        error!("{num} subscription requests were missed.");
                    }
                },
                // Handle WebSocket messages
                message = read.next() => match message {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<Stream>(&text) {
                            Ok(Stream::EventStream(stream)) => {
                                self.process_message(stream);
                            }
                            Ok(Stream::Result(result)) => {
                                debug!(?result, "Subscription request response is received.");
                            }
                            Err(error) => {
                                error!(?error, %text, "Couldn't parse Stream.");
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        write.send(Message::Pong(data)).await?;
                        last_ping = Instant::now();
                    }
                    Some(Ok(Message::Close(close_frame))) => {
                        warn!(?close_frame, "WebSocket connection closed by server");
                        return Err(BinanceFuturesError::ConnectionAbort(
                            close_frame.map(|f| f.to_string()).unwrap_or(String::new())
                        ));
                    }
                    Some(Ok(Message::Binary(_)))
                    | Some(Ok(Message::Frame(_)))
                    | Some(Ok(Message::Pong(_))) => {}
                    Some(Err(error)) => {
                        error!(?error, "WebSocket error");
                        return Err(BinanceFuturesError::from(error));
                    }
                    None => {
                        warn!("WebSocket stream ended");
                        return Err(BinanceFuturesError::ConnectionInterrupted);
                    }
                }
            }
        }
    }
}
