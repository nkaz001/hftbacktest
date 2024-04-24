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
    backtest::state::StateValues,
    connector::Connector,
    depth::{HashMapMarketDepth, MarketDepth},
    live::AssetInfo,
    types::{
        Error as ErrorEvent,
        Error,
        Event,
        LiveEvent,
        OrdType,
        Order,
        Request,
        Side,
        Status,
        TimeInForce,
        BUY,
        SELL,
    },
    BuildError,
    Interface,
};

#[derive(Error, Eq, PartialEq, Clone, Debug)]
pub enum BotError {
    #[error("asset not found")]
    AssetNotFound,
    #[error("order not found")]
    OrderNotFound,
    #[error("order id already exists")]
    DuplicateOrderId,
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
    mapping: Vec<(String, AssetInfo)>,
) {
    conns
        .iter_mut()
        .for_each(|(_, conn)| conn.run(ev_tx.clone()).unwrap());
    loop {
        select! {
            req = req_rx.recv() => {
                match req {
                    Some(Request::Order((asset_no, order))) => {
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
pub type OrderRecvHook = Box<dyn Fn(&Order<()>, &Order<()>) -> Result<(), BotError>>;

/// Live [`Bot`] builder.
pub struct BotBuilder<MD> {
    conns: HashMap<String, Box<dyn Connector + Send + 'static>>,
    assets: Vec<(String, AssetInfo)>,
    error_handler: Option<ErrorHandler>,
    order_hook: Option<OrderRecvHook>,
    depth_builder: Option<Box<dyn FnMut(&AssetInfo) -> MD>>,
}

impl<MD> BotBuilder<MD> {
    /// Registers a [`Connector`] with a specified name.
    /// The specified name for this connector is used when using [`BotBuilder::add`] to add an
    /// asset for trading through this connector.
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
    /// * `name` - Name of the [`Connector`], which is registered by [`BotBuilder::register`],
    ///            through which this asset will be traded.
    /// * `symbol` - Symbol of the asset. You need to check with the [`Connector`] which symbology
    ///              is used.
    /// * `tick_size` - The minimum price fluctuation.
    /// * `lot_size` -  The minimum trade size.
    pub fn add(self, name: &str, symbol: &str, tick_size: f32, lot_size: f32) -> Self {
        Self {
            assets: {
                let asset_no = self.assets.len();
                let mut assets = self.assets;
                assets.push((
                    name.to_string(),
                    AssetInfo {
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
        Handler: Fn(Error) -> Result<(), BotError> + 'static,
    {
        Self {
            error_handler: Some(Box::new(handler)),
            ..self
        }
    }

    /// Registers the order response receive hook.
    pub fn order_recv_hook<Hook>(self, hook: Hook) -> Self
    where
        Hook: Fn(&Order<()>, &Order<()>) -> Result<(), BotError> + 'static,
    {
        Self {
            order_hook: Some(Box::new(hook)),
            ..self
        }
    }

    /// Sets [`MarketDepth`] build function.
    pub fn depth<Builder>(self, builder: Builder) -> Self
    where
        Builder: Fn(&AssetInfo) -> MD + 'static,
    {
        Self {
            depth_builder: Some(Box::new(builder)),
            ..self
        }
    }

    /// Builds a live [`Bot`] based on the registered connectors and assets.
    pub fn build(self) -> Result<Bot<MD>, BuildError> {
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
        let position = self.assets.iter().map(|_| 0.0).collect();
        let trade = self.assets.iter().map(|_| Vec::new()).collect();

        Ok(Bot {
            ev_tx: Some(ev_tx),
            ev_rx,
            req_rx: Some(req_rx),
            req_tx,
            depth,
            orders,
            position,
            conns: Some(conns),
            assets: self.assets,
            trade,
            error_handler: self.error_handler,
            order_hook: self.order_hook,
        })
    }
}

/// A live trading bot.
///
/// Provides the same interface as the backtesters in [`crate::backtest`].
///
/// ```
/// let mut hbt = Bot::builder()
///     .register("connector_name", connector)
///     .add("connector_name", "symbol", tick_size, lot_size)
///     .depth(|asset_info| HashMapMarketDepth::new(asset_info.tick_size, asset_info.lot_size))
///     .build()
///     .unwrap();
/// ```
pub struct Bot<MD> {
    req_tx: UnboundedSender<Request>,
    req_rx: Option<UnboundedReceiver<Request>>,
    ev_tx: Option<Sender<LiveEvent>>,
    ev_rx: Receiver<LiveEvent>,
    depth: Vec<MD>,
    orders: Vec<HashMap<i64, Order<()>>>,
    position: Vec<f64>,
    trade: Vec<Vec<Event>>,
    conns: Option<HashMap<String, Box<dyn Connector + Send + 'static>>>,
    assets: Vec<(String, AssetInfo)>,
    error_handler: Option<ErrorHandler>,
    order_hook: Option<OrderRecvHook>,
}

impl<MD> Bot<MD>
where
    MD: MarketDepth,
{
    /// Builder to construct [`Bot`] instances.
    pub fn builder() -> BotBuilder<MD> {
        BotBuilder {
            conns: HashMap::new(),
            assets: Vec::new(),
            error_handler: None,
            order_hook: None,
            depth_builder: None,
        }
    }

    /// Runs the [`Bot`]. Spawns a thread to run [`Connector`]s and to handle sending [`Request`]
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

    fn elapse_(&mut self, duration: i64) -> Result<bool, BotError> {
        let now = Instant::now();
        let mut remaining_duration = duration;
        loop {
            let timeout = Duration::from_nanos(remaining_duration as u64);
            match self.ev_rx.recv_timeout(timeout) {
                Ok(LiveEvent::Depth(data)) => {
                    // fixme: updates the depth only if exch_ts is greater than that of the existing
                    //        level.
                    let depth = unsafe { self.depth.get_unchecked_mut(data.asset_no) };
                    // depth.timestamp = data.exch_ts;
                    for (px, qty) in data.bids {
                        depth.update_bid_depth(px, qty, 0);
                    }
                    for (px, qty) in data.asks {
                        depth.update_ask_depth(px, qty, 0);
                    }
                }
                Ok(LiveEvent::Trade(data)) => {
                    let trade = unsafe { self.trade.get_unchecked_mut(data.asset_no) };
                    trade.push(Event {
                        exch_ts: data.exch_ts,
                        local_ts: data.local_ts,
                        ev: {
                            if data.side == 1 {
                                BUY
                            } else if data.side == -1 {
                                SELL
                            } else {
                                0
                            }
                        },
                        px: data.price,
                        qty: data.qty,
                    });
                }
                Ok(LiveEvent::Order(data)) => {
                    debug!(?data, "Event::Order");
                    match self
                        .orders
                        .get_mut(data.asset_no)
                        .ok_or(BotError::AssetNotFound)?
                        .entry(data.order.order_id)
                    {
                        Entry::Occupied(mut entry) => {
                            let ex_order = entry.get_mut();
                            if let Some(hook) = self.order_hook.as_mut() {
                                hook(ex_order, &data.order)?;
                            }
                            if data.order.exch_timestamp >= ex_order.exch_timestamp {
                                if ex_order.status == Status::Canceled
                                    || ex_order.status == Status::Expired
                                    || ex_order.status == Status::Filled
                                {
                                    // Ignores the update since the current status is the final status.
                                } else {
                                    ex_order.update(&data.order);
                                }
                            }
                        }
                        Entry::Vacant(entry) => {
                            error!(
                                ?data,
                                "Bot received an unmanaged order. \
                                This should be handled by a Connector."
                            );
                            entry.insert(data.order);
                        }
                    }
                }
                Ok(LiveEvent::Position(data)) => {
                    *(unsafe { self.position.get_unchecked_mut(data.asset_no) }) = data.qty;
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

    fn submit_order(
        &mut self,
        asset_no: usize,
        order_id: i64,
        price: f32,
        qty: f32,
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
            return Err(BotError::DuplicateOrderId);
        }
        let tick_size = self.assets.get(asset_no).unwrap().1.tick_size;
        let order = Order {
            order_id,
            front_q_qty: 0.0,
            q: (),
            price_tick: (price / tick_size).round() as i32,
            qty,
            leaves_qty: 0.0,
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
            maker: false,
        };
        orders.insert(order.order_id, order.clone());
        self.req_tx.send(Request::Order((asset_no, order))).unwrap();
        Ok(true)
    }
}

impl Interface<(), HashMapMarketDepth> for Bot<HashMapMarketDepth> {
    type Error = BotError;

    #[inline]
    fn current_timestamp(&self) -> i64 {
        Utc::now().timestamp_nanos_opt().unwrap()
    }

    #[inline]
    fn position(&self, asset_no: usize) -> f64 {
        *self.position.get(asset_no).unwrap_or(&0.0)
    }

    #[inline]
    fn state_values(&self, asset_no: usize) -> StateValues {
        StateValues {
            position: *self.position.get(asset_no).unwrap_or(&0.0),
            balance: 0.0,
            fee: 0.0,
            trade_num: 0,
            trade_qty: 0.0,
            trade_amount: 0.0,
        }
    }

    #[inline]
    fn depth(&self, asset_no: usize) -> &HashMapMarketDepth {
        self.depth.get(asset_no).unwrap()
    }

    #[inline]
    fn trade(&self, asset_no: usize) -> &Vec<Event> {
        self.trade.get(asset_no).unwrap()
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
    fn orders(&self, asset_no: usize) -> &HashMap<i64, Order<()>> {
        self.orders.get(asset_no).unwrap()
    }

    #[inline]
    fn submit_buy_order(
        &mut self,
        asset_no: usize,
        order_id: i64,
        price: f32,
        qty: f32,
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
        order_id: i64,
        price: f32,
        qty: f32,
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

    #[inline]
    fn cancel(&mut self, asset_no: usize, order_id: i64, wait: bool) -> Result<bool, Self::Error> {
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
            .send(Request::Order((asset_no, order.clone())))
            .unwrap();
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
    fn elapse(&mut self, duration: i64) -> Result<bool, Self::Error> {
        self.elapse_(duration)
    }

    #[inline]
    fn elapse_bt(&mut self, _duration: i64) -> Result<bool, Self::Error> {
        Ok(true)
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}
