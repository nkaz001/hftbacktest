use std::{
    any::Any,
    fmt::{Debug, Formatter},
    sync::Arc,
};

/// Error type which is assigned to [`Error`].
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(i64)]
pub enum ErrorType {
    ConnectionInterrupted = 0,
    CriticalConnectionError = 1,
    OrderError = 2,
    Custom(i64),
}

/// Error conveyed by [`Event`].
#[derive(Clone, Debug)]
pub struct Error {
    pub ty: ErrorType,
    pub value: Option<Arc<Box<dyn Any + Send + Sync>>>,
}

impl Error {
    pub fn new(ty: ErrorType) -> Error {
        Self { ty, value: None }
    }

    pub fn with<T>(ty: ErrorType, value: T) -> Error
    where
        T: Send + Sync + 'static,
    {
        Self {
            ty,
            value: Some(Arc::new(Box::new(value))),
        }
    }

    pub fn value_downcast_ref<T>(&self) -> Option<&T>
    where
        T: 'static,
    {
        self.value
            .as_ref()
            .map(|value| value.downcast_ref())
            .flatten()
    }
}

/// Events that occur in a live bot sent by a connector.
#[derive(Clone, Debug)]
pub enum Event {
    Depth(Depth),
    Trade(Trade),
    Order(OrderResponse),
    Position(Position),
    Error(Error),
}

pub const BUY: i64 = 1 << 29;
pub const SELL: i64 = 1 << 28;

pub const DEPTH_EVENT: i64 = 1;
pub const TRADE_EVENT: i64 = 2;
pub const DEPTH_CLEAR_EVENT: i64 = 3;
pub const DEPTH_SNAPSHOT_EVENT: i64 = 4;
pub const USER_DEFINED_EVENT: i64 = 100;

pub trait AsStr {
    fn as_str(&self) -> &'static str;
}

/// Exchange event data.
#[derive(Clone, PartialEq, Debug)]
#[repr(C, align(32))]
pub struct Row {
    pub ev: i64,
    pub exch_ts: i64,
    pub local_ts: i64,
    pub px: f32,
    pub qty: f32,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Depth {
    pub asset_no: usize,
    pub exch_ts: i64,
    pub local_ts: i64,
    pub bids: Vec<(f32, f32)>,
    pub asks: Vec<(f32, f32)>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Trade {
    pub asset_no: usize,
    pub exch_ts: i64,
    pub local_ts: i64,
    pub side: i8,
    pub price: f32,
    pub qty: f32,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Position {
    pub asset_no: usize,
    pub symbol: String,
    pub qty: f64,
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(i8)]
pub enum Side {
    Buy = 1,
    Sell = -1,
    Unsupported = 127,
}

impl Side {
    pub fn as_f64(&self) -> f64 {
        match self {
            Side::Buy => 1f64,
            Side::Sell => -1f64,
            Side::Unsupported => panic!("Side::Unsupported"),
        }
    }

    pub fn as_f32(&self) -> f32 {
        match self {
            Side::Buy => 1f32,
            Side::Sell => -1f32,
            Side::Unsupported => panic!("Side::Unsupported"),
        }
    }
}

impl AsStr for Side {
    fn as_str(&self) -> &'static str {
        match self {
            Side::Buy => "BUY",
            Side::Sell => "SELL",
            Side::Unsupported => panic!("Side::Unsupported"),
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u8)]
pub enum Status {
    None = 0,
    New = 1,
    Expired = 2,
    Filled = 3,
    Canceled = 4,
    PartiallyFilled = 5,
    Unsupported = 255,
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u8)]
pub enum TimeInForce {
    GTC = 0,
    GTX = 1,
    FOK = 2,
    IOC = 3,
    Unsupported = 255,
}

impl AsStr for TimeInForce {
    fn as_str(&self) -> &'static str {
        match self {
            TimeInForce::GTC => "GTC",
            TimeInForce::GTX => "GTX",
            TimeInForce::FOK => "FOK",
            TimeInForce::IOC => "IOC",
            TimeInForce::Unsupported => panic!("TimeInForce::Unsupported"),
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u8)]
pub enum OrdType {
    Limit = 0,
    Market = 1,
    Unsupported = 255,
}

impl AsStr for OrdType {
    fn as_str(&self) -> &'static str {
        match self {
            OrdType::Limit => "LIMIT",
            OrdType::Market => "MARKET",
            OrdType::Unsupported => panic!("OrdType::Unsupported"),
        }
    }
}

#[derive(Clone)]
pub struct Order<Q>
where
    Q: Sized + Clone,
{
    pub qty: f32,
    pub leaves_qty: f32,
    pub price_tick: i32,
    pub tick_size: f32,
    pub side: Side,
    pub time_in_force: TimeInForce,
    pub exch_timestamp: i64,
    pub status: Status,
    pub local_timestamp: i64,
    pub req: Status,
    pub exec_price_tick: i32,
    pub exec_qty: f32,
    pub order_id: i64,
    pub q: Q,
    pub maker: bool,
    pub order_type: OrdType,
}

impl<Q> Order<Q>
where
    Q: Sized + Clone,
{
    pub fn new(
        order_id: i64,
        price_tick: i32,
        tick_size: f32,
        qty: f32,
        side: Side,
        order_type: OrdType,
        time_in_force: TimeInForce,
    ) -> Self
    where
        Q: Default,
    {
        Self {
            qty,
            leaves_qty: qty,
            price_tick,
            tick_size,
            side,
            time_in_force,
            exch_timestamp: 0,
            status: Status::None,
            local_timestamp: 0,
            req: Status::None,
            exec_price_tick: 0,
            exec_qty: 0.0,
            order_id,
            q: Q::default(),
            maker: false,
            order_type,
        }
    }

    pub fn price(&self) -> f32 {
        self.price_tick as f32 * self.tick_size
    }

    pub fn exec_price(&self) -> f32 {
        self.exec_price_tick as f32 * self.tick_size
    }

    pub fn cancellable(&self) -> bool {
        (self.status == Status::New || self.status == Status::PartiallyFilled)
            && self.req == Status::None
    }

    pub fn active(&self) -> bool {
        self.status == Status::New || self.status == Status::PartiallyFilled
    }

    pub fn pending(&self) -> bool {
        self.req != Status::None
    }

    pub fn update(&mut self, order: &Order<Q>) {
        self.qty = order.qty;
        self.leaves_qty = order.leaves_qty;
        self.price_tick = order.price_tick;
        self.tick_size = order.tick_size;
        self.side = order.side;
        self.time_in_force = order.time_in_force;

        assert!(order.exch_timestamp >= self.exch_timestamp);
        if order.exch_timestamp > 0 {
            self.exch_timestamp = order.exch_timestamp;
        }
        self.status = order.status;
        if order.local_timestamp > 0 {
            self.local_timestamp = order.local_timestamp;
        }
        self.req = order.req;
        self.exec_price_tick = order.exec_price_tick;
        self.exec_qty = order.exec_qty;
        self.order_id = order.order_id;
        self.q = order.q.clone();
        self.maker = order.maker;
        self.order_type = order.order_type;
    }
}

impl<Q> Debug for Order<Q>
where
    Q: Sized + Clone,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Order")
            .field("qty", &self.qty)
            .field("leaves_qty", &self.leaves_qty)
            .field("price_tick", &self.price_tick)
            .field("tick_size", &self.tick_size)
            .field("side", &self.side)
            .field("time_in_force", &self.time_in_force)
            .field("exch_timestamp", &self.exch_timestamp)
            .field("status", &self.status)
            .field("local_timestamp", &self.local_timestamp)
            .field("req", &self.req)
            .field("exec_price_tick", &self.exec_price_tick)
            .field("exec_qty", &self.exec_qty)
            .field("order_id", &self.order_id)
            .field("maker", &self.maker)
            .field("order_type", &self.order_type)
            .finish()
    }
}

#[derive(Clone, Debug)]
pub enum Request {
    Order((usize, Order<()>)),
}

#[derive(Clone, Debug)]
pub struct OrderResponse {
    pub asset_no: usize,
    pub order: Order<()>,
}
