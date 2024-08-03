use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender},
    thread,
    time::{Duration, Instant},
};

use chrono::Utc;
use thiserror::Error;
use tokio::{
    select,
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
};
use tracing::{debug, error};

use crate::{
    connector::Connector,
    depth::{L2MarketDepth, MarketDepth},
    live::Asset,
    prelude::{OrderId, OrderRequest, WaitOrderResponse},
    types::{
        Bot,
        BuildError,
        Event,
        LiveError as ErrorEvent,
        LiveError,
        LiveEvent,
        OrdType,
        Order,
        Request,
        Side,
        StateValues,
        Status,
        TimeInForce,
        LOCAL_ASK_DEPTH_EVENT,
        LOCAL_BID_DEPTH_EVENT,
        LOCAL_BUY_TRADE_EVENT,
        LOCAL_SELL_TRADE_EVENT,
    },
};

#[derive(Error, Eq, PartialEq, Clone, Debug)]
pub enum BotError {
    #[error("order id already exists")]
    OrderIdExist,
    #[error("asset not found")]
    AssetNotFound,
    #[error("order not found")]
    OrderNotFound,
    #[error("order status is invalid")]
    InvalidOrderStatus,
    #[error("{0}")]
    Custom(String),
}

#[tokio::main]
async fn thread_main(
    ev_tx: Sender<LiveEvent>,
    mut req_rx: UnboundedReceiver<Request>,
    mut conns: HashMap<String, Box<dyn Connector + Send + 'static>>,
    mapping: Vec<(String, Asset)>,
) {
    conns
        .iter_mut()
        .for_each(|(_, conn)| conn.run(ev_tx.clone()).unwrap());
    loop {
        select! {
            req = req_rx.recv() => {
                match req {
                    Some(Request::Order { asset_no, order }) => {
                        if let Some((connector_name, _)) = mapping.get(asset_no) {
                            let conn_ = conns.get_mut(connector_name).unwrap();
                            let ev_tx_ = ev_tx.clone();
                            match order.req {
                                Status::New => {
                                    if let Err(error) = conn_.submit(asset_no, order, ev_tx_) {
                                        error!(
                                            %connector_name,
                                            ?error,
                                            "Unable to submit a new order due to an internal error in the connector."
                                        );
                                    }
                                }
                                Status::Canceled => {
                                    if let Err(error) = conn_.cancel(asset_no, order, ev_tx_) {
                                        error!(
                                            %connector_name,
                                            ?error,
                                            "Unable to cancel an open order due to an internal error in the connector."
                                        );
                                    }
                                }
                                req => {
                                    error!(%connector_name, ?req, "req_rx received an invalid request.");
                                }
                            }
                        }
                    }
                    None => {
                        debug!("req_rx channel is closed.");
                        break;
                    }
                }
            }
        }
    }
}

pub type ErrorHandler = Box<dyn Fn(ErrorEvent) -> Result<(), BotError>>;
pub type OrderRecvHook = Box<dyn Fn(&Order, &Order) -> Result<(), BotError>>;

pub type DepthBuilder<MD> = Box<dyn FnMut(&Asset) -> MD>;

/// Live [`LiveBot`] builder.
pub struct LiveBotBuilder<MD> {
    conns: HashMap<String, Box<dyn Connector + Send + 'static>>,
    assets: Vec<(String, Asset)>,
    error_handler: Option<ErrorHandler>,
    order_hook: Option<OrderRecvHook>,
    depth_builder: Option<DepthBuilder<MD>>,
    trade_len: usize,
}

impl<MD> LiveBotBuilder<MD> {
    /// Registers a [`Connector`] with a specified name.
    /// The specified name for this connector is used when using [`add()`](`LiveBotBuilder::add()`)
    /// to add an asset for trading through this connector.
    pub fn register<C>(self, name: &str, conn: C) -> Self
    where
        C: Connector + Send + 'static,
    {
        Self {
            conns: {
                let mut conns = self.conns;
                conns.insert(name.to_string(), Box::new(conn));
                conns
            },
            ..self
        }
    }

    /// Adds an asset.
    ///
    /// * `name` - Name of the [`Connector`], which is registered by
    ///            [`register()`](`LiveBotBuilder::register()`), through which this asset will be
    ///            traded.
    /// * `symbol` - Symbol of the asset. You need to check with the [`Connector`] which symbology
    ///              is used.
    /// * `tick_size` - The minimum price fluctuation.
    /// * `lot_size` -  The minimum trade size.
    pub fn add(self, name: &str, symbol: &str, tick_size: f64, lot_size: f64) -> Self {
        Self {
            assets: {
                let asset_no = self.assets.len();
                let mut assets = self.assets;
                assets.push((
                    name.to_string(),
                    Asset {
                        asset_no,
                        symbol: symbol.to_string(),
                        tick_size,
                        lot_size,
                    },
                ));
                assets
            },
            ..self
        }
    }

    /// Registers the error handler to deal with an error from connectors.
    pub fn error_handler<Handler>(self, handler: Handler) -> Self
    where
        Handler: Fn(LiveError) -> Result<(), BotError> + 'static,
    {
        Self {
            error_handler: Some(Box::new(handler)),
            ..self
        }
    }

    /// Registers the order response receive hook.
    pub fn order_recv_hook<Hook>(self, hook: Hook) -> Self
    where
        Hook: Fn(&Order, &Order) -> Result<(), BotError> + 'static,
    {
        Self {
            order_hook: Some(Box::new(hook)),
            ..self
        }
    }

    /// Sets [`MarketDepth`] build function.
    pub fn depth<Builder>(self, builder: Builder) -> Self
    where
        Builder: Fn(&Asset) -> MD + 'static,
    {
        Self {
            depth_builder: Some(Box::new(builder)),
            ..self
        }
    }

    /// Sets the length of market trades to be stored in the local processor. The default value is
    /// `0`.
    pub fn trade_len(self, trade_len: usize) -> Self {
        Self { trade_len, ..self }
    }

    /// Builds a live [`LiveBot`] based on the registered connectors and assets.
    pub fn build(self) -> Result<LiveBot<MD>, BuildError> {
        let mut dup = HashSet::new();
        let mut conns = self.conns;
        for (asset_no, (name, asset_info)) in self.assets.iter().enumerate() {
            if !dup.insert(format!("{}/{}", name, asset_info.symbol)) {
                Err(BuildError::Duplicate(
                    name.clone(),
                    asset_info.symbol.clone(),
                ))?;
            }
            let conn = conns
                .get_mut(name)
                .ok_or(BuildError::ConnectorNotFound(name.to_string()))?;
            conn.add(
                asset_no,
                asset_info.symbol.clone(),
                asset_info.tick_size,
                asset_info.lot_size,
            )?;
        }

        let (ev_tx, ev_rx) = channel();
        let (req_tx, req_rx) = unbounded_channel();

        let mut depth_builder = self
            .depth_builder
            .ok_or(BuildError::BuilderIncomplete("depth"))?;
        let depth = self
            .assets
            .iter()
            .map(|(_, asset_info)| depth_builder(asset_info))
            .collect();

        let orders = self.assets.iter().map(|_| HashMap::new()).collect();
        let state = self.assets.iter().map(|_| Default::default()).collect();
        let trade = self
            .assets
            .iter()
            .map(|_| Vec::with_capacity(self.trade_len))
            .collect();
        let last_feed_latency = self.assets.iter().map(|_| None).collect();
        let last_order_latency = self.assets.iter().map(|_| None).collect();

        Ok(LiveBot {
            ev_tx: Some(ev_tx),
            ev_rx,
            req_rx: Some(req_rx),
            req_tx,
            depth,
            orders,
            state,
            conns: Some(conns),
            assets: self.assets,
            trade,
            trade_len: self.trade_len,
            error_handler: self.error_handler,
            order_hook: self.order_hook,
            last_feed_latency,
            last_order_latency,
        })
    }
}

/// A live trading bot.
///
/// Provides the same interface as the backtesters in [`backtest`](`crate::backtest`).
///
/// ```
/// use hftbacktest::{live::LiveBot, prelude::HashMapMarketDepth};
///
/// let mut hbt = LiveBot::builder()
///     .register("connector_name", connector)
///     .add("connector_name", "symbol", tick_size, lot_size)
///     .depth(|asset| HashMapMarketDepth::new(asset.tick_size, asset.lot_size))
///     .build()
///     .unwrap();
/// ```
pub struct LiveBot<MD> {
    req_tx: UnboundedSender<Request>,
    req_rx: Option<UnboundedReceiver<Request>>,
    ev_tx: Option<Sender<LiveEvent>>,
    ev_rx: Receiver<LiveEvent>,
    depth: Vec<MD>,
    orders: Vec<HashMap<OrderId, Order>>,
    trade: Vec<Vec<Event>>,
    trade_len: usize,
    conns: Option<HashMap<String, Box<dyn Connector + Send + 'static>>>,
    assets: Vec<(String, Asset)>,
    error_handler: Option<ErrorHandler>,
    order_hook: Option<OrderRecvHook>,
    last_feed_latency: Vec<Option<(i64, i64)>>,
    last_order_latency: Vec<Option<(i64, i64, i64)>>,
    state: Vec<StateValues>,
}

impl<MD> LiveBot<MD>
where
    MD: MarketDepth + L2MarketDepth,
{
    /// Builder to construct [`LiveBot`] instances.
    pub fn builder() -> LiveBotBuilder<MD> {
        LiveBotBuilder {
            conns: HashMap::new(),
            assets: Vec::new(),
            error_handler: None,
            order_hook: None,
            depth_builder: None,
            trade_len: 0,
        }
    }

    /// Runs the [`LiveBot`]. Spawns a thread to run [`Connector`]s and to handle sending [`Request`]
    /// to [`Connector`]s without blocking.
    pub fn run(&mut self) -> Result<(), BotError> {
        let ev_tx = self.ev_tx.take().unwrap();
        let req_rx = self.req_rx.take().unwrap();
        let conns = self.conns.take().unwrap();
        let assets = self.assets.clone();
        let _ = thread::spawn(move || {
            thread_main(ev_tx, req_rx, conns, assets);
        });
        Ok(())
    }

    fn elapse_<const WAIT_NEXT_FEED: bool>(
        &mut self,
        duration: i64,
        wait_order_response: WaitOrderResponse,
    ) -> Result<bool, BotError> {
        let now = Instant::now();
        let mut remaining_duration = duration;
        loop {
            let timeout = Duration::from_nanos(remaining_duration as u64);
            match self.ev_rx.recv_timeout(timeout) {
                Ok(LiveEvent::FeedBatch { asset_no, events }) => {
                    for event in events {
                        *unsafe { self.last_feed_latency.get_unchecked_mut(asset_no) } =
                            Some((event.exch_ts, event.local_ts));
                        if event.is(LOCAL_BID_DEPTH_EVENT) {
                            let depth = unsafe { self.depth.get_unchecked_mut(asset_no) };
                            depth.update_bid_depth(event.px, event.qty, event.exch_ts);
                        } else if event.is(LOCAL_ASK_DEPTH_EVENT) {
                            let depth = unsafe { self.depth.get_unchecked_mut(asset_no) };
                            depth.update_ask_depth(event.px, event.qty, event.exch_ts);
                        } else if (event.is(LOCAL_BUY_TRADE_EVENT)
                            || event.is(LOCAL_SELL_TRADE_EVENT))
                            && self.trade_len > 0
                        {
                            let trade = unsafe { self.trade.get_unchecked_mut(asset_no) };
                            trade.push(event);
                        }
                    }
                    if WAIT_NEXT_FEED {
                        return Ok(true);
                    }
                }
                Ok(LiveEvent::Feed { asset_no, event }) => {
                    *unsafe { self.last_feed_latency.get_unchecked_mut(asset_no) } =
                        Some((event.exch_ts, event.local_ts));
                    if event.is(LOCAL_BID_DEPTH_EVENT) {
                        let depth = unsafe { self.depth.get_unchecked_mut(asset_no) };
                        depth.update_bid_depth(event.px, event.qty, event.exch_ts);
                    } else if event.is(LOCAL_ASK_DEPTH_EVENT) {
                        let depth = unsafe { self.depth.get_unchecked_mut(asset_no) };
                        depth.update_ask_depth(event.px, event.qty, event.exch_ts);
                    } else if (event.is(LOCAL_BUY_TRADE_EVENT) || event.is(LOCAL_SELL_TRADE_EVENT))
                        && self.trade_len > 0
                    {
                        let trade = unsafe { self.trade.get_unchecked_mut(asset_no) };
                        trade.push(event);
                    }
                }
                Ok(LiveEvent::Order { asset_no, order }) => {
                    debug!(%asset_no, ?order, "Event::Order");
                    let received_order_resp = match wait_order_response {
                        WaitOrderResponse::Any => true,
                        WaitOrderResponse::Specified {
                            asset_no: wait_order_asset_no,
                            order_id: wait_order_id,
                        } if wait_order_id == order.order_id && wait_order_asset_no == asset_no => {
                            true
                        }
                        _ => false,
                    };
                    *unsafe { self.last_order_latency.get_unchecked_mut(asset_no) } = Some((
                        order.local_timestamp,
                        order.exch_timestamp,
                        Utc::now().timestamp_nanos_opt().unwrap(),
                    ));
                    match self
                        .orders
                        .get_mut(asset_no)
                        .ok_or(BotError::AssetNotFound)?
                        .entry(order.order_id)
                    {
                        Entry::Occupied(mut entry) => {
                            let ex_order = entry.get_mut();
                            if let Some(hook) = self.order_hook.as_mut() {
                                hook(ex_order, &order)?;
                            }
                            if order.exch_timestamp >= ex_order.exch_timestamp {
                                if ex_order.status == Status::Canceled
                                    || ex_order.status == Status::Expired
                                    || ex_order.status == Status::Filled
                                {
                                    // Ignores the update since the current status is the final status.
                                } else {
                                    ex_order.update(&order);
                                }
                            }
                        }
                        Entry::Vacant(entry) => {
                            error!(
                                %asset_no,
                                ?order,
                                "Bot received an unmanaged order. \
                                This should be handled by a Connector."
                            );
                            entry.insert(order);
                        }
                    }
                    if received_order_resp {
                        return Ok(true);
                    }
                }
                Ok(LiveEvent::Position { asset_no, qty }) => {
                    unsafe { self.state.get_unchecked_mut(asset_no) }.position = qty;
                }
                Ok(LiveEvent::Error(error)) => {
                    if let Some(handler) = self.error_handler.as_mut() {
                        handler(error)?;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    return Ok(true);
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Ok(false);
                }
            }
            let elapsed = now.elapsed().as_nanos() as i64;
            if elapsed > duration {
                return Ok(true);
            }
            remaining_duration = duration - elapsed;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn submit_order(
        &mut self,
        asset_no: usize,
        order_id: u64,
        price: f64,
        qty: f64,
        time_in_force: TimeInForce,
        order_type: OrdType,
        wait: bool,
        side: Side,
    ) -> Result<bool, BotError> {
        let orders = self
            .orders
            .get_mut(asset_no)
            .ok_or(BotError::AssetNotFound)?;
        if orders.contains_key(&order_id) {
            return Err(BotError::OrderIdExist);
        }
        let tick_size = self.assets.get(asset_no).unwrap().1.tick_size;
        let order = Order {
            order_id,
            price_tick: (price / tick_size).round() as i64,
            qty,
            leaves_qty: qty,
            tick_size,
            side,
            time_in_force,
            order_type,
            status: Status::New,
            local_timestamp: Utc::now().timestamp_nanos_opt().unwrap(),
            req: Status::New,
            exec_price_tick: 0,
            exch_timestamp: 0,
            exec_qty: 0.0,
            // Invalid information
            q: Box::new(()),
            maker: false,
        };
        let order_id = order.order_id;
        orders.insert(order_id, order.clone());
        self.req_tx
            .send(Request::Order { asset_no, order })
            .unwrap();
        if wait {
            // fixme: timeout should be specified by the argument.
            return self.wait_order_response(asset_no, order_id, 60_000_000_000);
        }
        Ok(true)
    }
}

impl<MD> Bot<MD> for LiveBot<MD>
where
    MD: MarketDepth + L2MarketDepth,
{
    type Error = BotError;

    #[inline]
    fn current_timestamp(&self) -> i64 {
        Utc::now().timestamp_nanos_opt().unwrap()
    }

    #[inline]
    fn num_assets(&self) -> usize {
        self.state.len()
    }

    #[inline]
    fn position(&self, asset_no: usize) -> f64 {
        self.state_values(asset_no).position
    }

    #[inline]
    fn state_values(&self, asset_no: usize) -> &StateValues {
        // todo: implement the missing fields. Trade values need to be changed to a rolling manner,
        //       unlike the current Python implementation, to support live trading.
        self.state.get(asset_no).unwrap()
    }

    #[inline]
    fn depth(&self, asset_no: usize) -> &MD {
        self.depth.get(asset_no).unwrap()
    }

    #[inline]
    fn last_trades(&self, asset_no: usize) -> &[Event] {
        self.trade.get(asset_no).unwrap().as_slice()
    }

    fn clear_last_trades(&mut self, asset_no: Option<usize>) {
        match asset_no {
            Some(asset_no) => {
                self.trade.get_mut(asset_no).unwrap().clear();
            }
            None => {
                for asset_no in 0..self.trade.len() {
                    self.trade.get_mut(asset_no).unwrap().clear();
                }
            }
        }
    }

    #[inline]
    fn orders(&self, asset_no: usize) -> &HashMap<OrderId, Order> {
        self.orders.get(asset_no).unwrap()
    }

    #[inline]
    fn submit_buy_order(
        &mut self,
        asset_no: usize,
        order_id: OrderId,
        price: f64,
        qty: f64,
        time_in_force: TimeInForce,
        order_type: OrdType,
        wait: bool,
    ) -> Result<bool, Self::Error> {
        self.submit_order(
            asset_no,
            order_id,
            price,
            qty,
            time_in_force,
            order_type,
            wait,
            Side::Buy,
        )
    }

    #[inline]
    fn submit_sell_order(
        &mut self,
        asset_no: usize,
        order_id: OrderId,
        price: f64,
        qty: f64,
        time_in_force: TimeInForce,
        order_type: OrdType,
        wait: bool,
    ) -> Result<bool, Self::Error> {
        self.submit_order(
            asset_no,
            order_id,
            price,
            qty,
            time_in_force,
            order_type,
            wait,
            Side::Sell,
        )
    }

    fn submit_order(
        &mut self,
        asset_no: usize,
        order: OrderRequest,
        wait: bool,
    ) -> Result<bool, Self::Error> {
        self.submit_order(
            asset_no,
            order.order_id,
            order.price,
            order.qty,
            order.time_in_force,
            order.order_type,
            wait,
            order.side,
        )
    }

    #[inline]
    fn cancel(
        &mut self,
        asset_no: usize,
        order_id: OrderId,
        wait: bool,
    ) -> Result<bool, Self::Error> {
        let orders = self
            .orders
            .get_mut(asset_no)
            .ok_or(BotError::AssetNotFound)?;
        let order = orders.get_mut(&order_id).ok_or(BotError::OrderNotFound)?;
        if !order.cancellable() {
            return Err(BotError::InvalidOrderStatus);
        }
        order.req = Status::Canceled;
        order.local_timestamp = Utc::now().timestamp_nanos_opt().unwrap();
        self.req_tx
            .send(Request::Order {
                asset_no,
                order: order.clone(),
            })
            .unwrap();
        if wait {
            // fixme: timeout should be specified by the argument.
            return self.wait_order_response(asset_no, order_id, 60_000_000_000);
        }
        Ok(true)
    }

    #[inline]
    fn clear_inactive_orders(&mut self, asset_no: Option<usize>) {
        match asset_no {
            Some(an) => {
                if let Some(orders) = self.orders.get_mut(an) {
                    orders.retain(|_, order| order.active());
                }
            }
            None => {
                for orders in self.orders.iter_mut() {
                    orders.retain(|_, order| order.active());
                }
            }
        }
    }

    #[inline]
    fn wait_order_response(
        &mut self,
        asset_no: usize,
        order_id: OrderId,
        timeout: i64,
    ) -> Result<bool, Self::Error> {
        self.elapse_::<false>(timeout, WaitOrderResponse::Specified { asset_no, order_id })
    }

    #[inline]
    fn wait_next_feed(
        &mut self,
        include_order_resp: bool,
        timeout: i64,
    ) -> Result<bool, Self::Error> {
        if include_order_resp {
            self.elapse_::<true>(timeout, WaitOrderResponse::Any)
        } else {
            self.elapse_::<true>(timeout, WaitOrderResponse::None)
        }
    }

    #[inline]
    fn elapse(&mut self, duration: i64) -> Result<bool, Self::Error> {
        self.elapse_::<false>(duration, WaitOrderResponse::None)
    }

    #[inline]
    fn elapse_bt(&mut self, _duration: i64) -> Result<bool, Self::Error> {
        Ok(true)
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn feed_latency(&self, asset_no: usize) -> Option<(i64, i64)> {
        *self.last_feed_latency.get(asset_no).unwrap()
    }

    fn order_latency(&self, asset_no: usize) -> Option<(i64, i64, i64)> {
        *self.last_order_latency.get(asset_no).unwrap()
    }
}
