use std::{
    collections::{hash_map::Entry, HashMap},
    sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender},
    thread,
    time::{Duration, Instant},
};

use chrono::Utc;
use tokio::{
    select,
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
};
use tracing::{debug, error, info, warn};

use crate::{
    backtest::{
        state::{State, StateValues},
        Error,
    },
    connector::Connector,
    depth::{btreebook::BTreeMapMarketDepth, MarketDepth},
    live::{AssetInfo, LiveBuilder},
    ty::{Event, OrdType, Order, Request, Row, Side, Status, TimeInForce, BUY, SELL},
    Interface,
};

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum BotError {
    AssetNotFound,
    OrderNotFound,
    DuplicateOrderId,
    InvalidOrderStatus,
}

#[tokio::main(worker_threads = 2)]
async fn thread_main(
    ev_tx: Sender<Event>,
    mut req_rx: UnboundedReceiver<Request>,
    mut conns: HashMap<String, Box<dyn Connector + Send + 'static>>,
    mapping: Vec<(String, AssetInfo)>,
) {
    for (_, conn) in conns.iter_mut() {
        conn.run(ev_tx.clone());
    }
    loop {
        select! {
            req = req_rx.recv() => {
                match req {
                    Some(Request::Order((an, order))) => {
                        if let Some((connector_name, _)) = mapping.get(an) {
                            let conn_ = conns.get_mut(connector_name).unwrap();
                            let ev_tx_ = ev_tx.clone();
                            match order.req{
                                Status::New => {
                                    if let Err(error) = conn_.submit(an, order, ev_tx_) {
                                        error!(?error, "submit error");
                                    }
                                }
                                Status::Canceled => {
                                    if let Err(error) = conn_.cancel(an, order, ev_tx_) {
                                        error!(?error, "cancel error");
                                    }
                                }
                                req => {
                                    error!(?req, "invalid request.");
                                }
                            }
                        }
                    }
                    None => {

                    }
                }
            }
        }
    }
}

pub struct Bot {
    req_tx: UnboundedSender<Request>,
    req_rx: Option<UnboundedReceiver<Request>>,
    ev_tx: Option<Sender<Event>>,
    ev_rx: Receiver<Event>,
    pub depth: Vec<BTreeMapMarketDepth>,
    pub orders: Vec<HashMap<i64, Order<()>>>,
    pub position: Vec<f64>,
    trade: Vec<Vec<Row>>,
    conns: Option<HashMap<String, Box<dyn Connector + Send + 'static>>>,
    assets: Vec<(String, AssetInfo)>,
}

impl Bot {
    pub fn new(
        conns: HashMap<String, Box<dyn Connector + Send + 'static>>,
        assets: Vec<(String, AssetInfo)>,
    ) -> Self {
        let (ev_tx, ev_rx) = channel();
        let (req_tx, req_rx) = unbounded_channel();

        let depth = assets
            .iter()
            .map(|(_, asset_info)| {
                BTreeMapMarketDepth::new(asset_info.tick_size, asset_info.lot_size)
            })
            .collect();

        let orders = assets.iter().map(|_| HashMap::new()).collect();
        let position = assets.iter().map(|_| 0.0).collect();
        let trade = assets.iter().map(|_| Vec::new()).collect();

        Self {
            ev_tx: Some(ev_tx),
            ev_rx,
            req_rx: Some(req_rx),
            req_tx,
            depth,
            orders,
            position,
            conns: Some(conns),
            assets,
            trade,
        }
    }

    pub fn run(&mut self) {
        let ev_tx = self.ev_tx.take().unwrap();
        let req_rx = self.req_rx.take().unwrap();
        let conns = self.conns.take().unwrap();
        let assets = self.assets.clone();
        let _ = thread::spawn(move || {
            thread_main(ev_tx, req_rx, conns, assets);
        });
    }

    fn elapse_(&mut self, duration: i64) -> Result<bool, BotError> {
        let now = Instant::now();
        let mut remaining_duration = duration;
        loop {
            let timeout = Duration::from_nanos(remaining_duration as u64);
            match self.ev_rx.recv_timeout(timeout) {
                Ok(Event::Depth(data)) => {
                    let depth = unsafe { self.depth.get_unchecked_mut(data.asset_no) };
                    depth.timestamp = data.exch_ts;
                    for (px, qty) in data.bids {
                        depth.update_bid_depth(px, qty, 0);
                    }
                    for (px, qty) in data.asks {
                        depth.update_ask_depth(px, qty, 0);
                    }
                }
                Ok(Event::Trade(data)) => {
                    let trade = unsafe { self.trade.get_unchecked_mut(data.asset_no) };
                    trade.push(Row {
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
                Ok(Event::Order(data)) => {
                    debug!(?data, "Event::Order");
                    match self
                        .orders
                        .get_mut(data.asset_no)
                        .ok_or(BotError::AssetNotFound)?
                        .entry(data.order.order_id)
                    {
                        Entry::Occupied(mut entry) => {
                            let ex_order = entry.get_mut();
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
                            warn!(?data, "Received an unmanaged order.");
                            entry.insert(data.order);
                        }
                    }
                }
                Ok(Event::Position(data)) => {
                    *(unsafe { self.position.get_unchecked_mut(data.asset_no) }) = data.qty;
                }
                Ok(Event::Error(code, _)) => {}
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

impl Interface<(), BTreeMapMarketDepth> for Bot {
    type Error = BotError;

    fn current_timestamp(&self) -> i64 {
        Utc::now().timestamp_nanos_opt().unwrap()
    }

    fn position(&self, asset_no: usize) -> f64 {
        *self.position.get(asset_no).unwrap_or(&0.0)
    }

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

    fn depth(&self, asset_no: usize) -> &BTreeMapMarketDepth {
        self.depth.get(asset_no).unwrap()
    }

    fn trade(&self, asset_no: usize) -> &Vec<Row> {
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

    fn orders(&self, asset_no: usize) -> &HashMap<i64, Order<()>> {
        self.orders.get(asset_no).unwrap()
    }

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

    fn clear_inactive_orders(&mut self, an: Option<usize>) {
        match an {
            Some(an) => {
                if let Some(orders) = self.orders.get_mut(an) {
                    orders.retain(|order_id, order| order.active());
                }
            }
            None => {
                for orders in self.orders.iter_mut() {
                    orders.retain(|order_id, order| order.active());
                }
            }
        }
    }

    fn elapse(&mut self, duration: i64) -> Result<bool, Self::Error> {
        self.elapse_(duration)
    }

    fn elapse_bt(&mut self, _duration: i64) -> Result<bool, Self::Error> {
        Ok(true)
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}
