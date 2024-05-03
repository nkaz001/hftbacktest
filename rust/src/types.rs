use std::{
    any::Any,
    collections::HashMap,
    fmt::{Debug, Formatter},
    sync::Arc,
};

use thiserror::Error;

/// Error type assigned to [`Error`].
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(i64)]
pub enum ErrorKind {
    ConnectionInterrupted = 0,
    CriticalConnectionError = 1,
    OrderError = 2,
    Custom(i64),
}

/// Error conveyed through [`LiveEvent`].
#[derive(Clone, Debug)]
pub struct Error {
    pub kind: ErrorKind,
    pub value: Option<Arc<Box<dyn Any + Send + Sync>>>,
}

impl Error {
    pub fn new(kind: ErrorKind) -> Error {
        Self { kind, value: None }
    }

    pub fn with<T>(kind: ErrorKind, value: T) -> Error
    where
        T: Send + Sync + 'static,
    {
        Self {
            kind,
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

/// Events occurring in a live bot sent by a [`crate::connector::Connector`].
#[derive(Clone, Debug)]
pub enum LiveEvent {
    Depth(Depth),
    Trade(Trade),
    Order(OrderResponse),
    Position(Position),
    Error(Error),
}

/// Indicates a buy, with specific meaning that can vary depending on the situation. For example,
/// when combined with a depth event, it means a bid-side event, while when combined with a trade
/// event, it means that the trade initiator is a buyer.
pub const BUY: i64 = 1 << 29;

/// Indicates a sell, with specific meaning that can vary depending on the situation. For example,
/// when combined with a depth event, it means an ask-side event, while when combined with a trade
/// event, it means that the trade initiator is a seller.
pub const SELL: i64 = 1 << 28;

/// Indicates that the market depth is changed.
pub const DEPTH_EVENT: i64 = 1;

/// Indicates that a trade occurs in the market.
pub const TRADE_EVENT: i64 = 2;

/// Indicates that the market depth is cleared.
pub const DEPTH_CLEAR_EVENT: i64 = 3;

/// Indicates that the market depth snapshot is received.
pub const DEPTH_SNAPSHOT_EVENT: i64 = 4;

pub trait AsStr {
    fn as_str(&self) -> &'static str;
}

/// Exchange event data.
#[derive(Clone, PartialEq, Debug)]
#[repr(C, align(32))]
pub struct Event {
    pub ev: i64,
    pub exch_ts: i64,
    pub local_ts: i64,
    pub px: f32,
    pub qty: f32,
}

impl Event {
    #[inline]
    pub fn is(&self, ev: i64) -> bool {
        (self.ev & ev) == ev
    }
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
    pub front_q_qty: f32,
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
            front_q_qty: 0.0,
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
        self.front_q_qty = order.front_q_qty;
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
            .field("front_q_qty", &self.front_q_qty)
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

#[derive(Debug)]
pub struct StateValues {
    pub position: f64,
    pub balance: f64,
    pub fee: f64,
    pub trade_num: i32,
    pub trade_qty: f64,
    pub trade_amount: f64,
}

#[derive(Error, Debug)]
pub enum BuildError {
    #[error("`{0}` is required")]
    BuilderIncomplete(&'static str),
    #[error("{0}")]
    InvalidArgument(&'static str),
    #[error("`{0}/{1}` already exists")]
    Duplicate(String, String),
    #[error("`{0}` is not found")]
    ConnectorNotFound(String),
    #[error("{0:?}")]
    Error(#[from] anyhow::Error),
}

/// Provides an interface for a backtester or a bot.
pub trait Interface<Q, MD>
where
    Q: Sized + Clone,
{
    type Error;

    /// In backtesting, this timestamp reflects the time at which the backtesting is conducted
    /// within the provided data. In a live bot, it's literally the current local timestamp.
    fn current_timestamp(&self) -> i64;

    /// Returns the position you currently hold.
    ///
    /// * `asset_no` - Asset number from which the position will be retrieved.
    fn position(&self, asset_no: usize) -> f64;

    /// Returns the state's values such as balance, fee, and so on.
    fn state_values(&self, asset_no: usize) -> StateValues;

    /// Returns the [MarketDepth](crate::depth::MarketDepth).
    ///
    /// * `asset_no` - Asset number from which the market depth will be retrieved.
    fn depth(&self, asset_no: usize) -> &MD;

    /// Returns the last market trades.
    ///
    /// * `asset_no` - Asset number from which the last market trades will be retrieved.
    fn trade(&self, asset_no: usize) -> &Vec<Event>;

    /// Clears the last market trades from the buffer.
    ///
    /// * `asset_no` - Asset number at which this command will be executed. If `None`, all last
    ///                trades in any assets will be cleared.
    fn clear_last_trades(&mut self, asset_no: Option<usize>);

    /// Returns a hash map of order IDs and their corresponding [`Order`]s.
    ///
    /// * `asset_no` - Asset number from which orders will be retrieved.
    fn orders(&self, asset_no: usize) -> &HashMap<i64, Order<Q>>;

    /// Places a buy order.
    ///
    /// * `asset_no` - Asset number at which this command will be executed.
    /// * `order_id` - The unique order ID; there should not be any existing order with the same ID
    ///                on both local and exchange sides.
    /// * `price` - Order price.
    /// * `qty` - Quantity to buy.
    /// * `time_in_force` - Available [`TimeInForce`] options vary depending on the exchange model.
    ///                     See to the exchange model for details.
    ///
    ///  * `order_type` - Available [`OrdType`] options vary depending on the exchange model. See to
    ///                   the exchange model for details.
    ///
    ///  * `wait` - If true, wait until the order placement response is received.
    fn submit_buy_order(
        &mut self,
        asset_no: usize,
        order_id: i64,
        price: f32,
        qty: f32,
        time_in_force: TimeInForce,
        order_type: OrdType,
        wait: bool,
    ) -> Result<bool, Self::Error>;

    /// Places a sell order.
    ///
    /// * `asset_no` - Asset number at which this command will be executed.
    /// * `order_id` - The unique order ID; there should not be any existing order with the same ID
    ///                on both local and exchange sides.
    /// * `price` - Order price.
    /// * `qty` - Quantity to buy.
    /// * `time_in_force` - Available [`TimeInForce`] options vary depending on the exchange model.
    ///                     See to the exchange model for details.
    ///
    ///  * `order_type` - Available [`OrdType`] options vary depending on the exchange model. See to
    ///                   the exchange model for details.
    ///
    ///  * `wait` - If true, wait until the order placement response is received.
    fn submit_sell_order(
        &mut self,
        asset_no: usize,
        order_id: i64,
        price: f32,
        qty: f32,
        time_in_force: TimeInForce,
        order_type: OrdType,
        wait: bool,
    ) -> Result<bool, Self::Error>;

    /// Cancels the specified order.
    ///
    /// * `asset_no` - Asset number at which this command will be executed.
    /// * `order_id` - Order ID to cancel.
    /// * `wait` - If true, wait until the order placement response is received.
    fn cancel(&mut self, asset_no: usize, order_id: i64, wait: bool) -> Result<bool, Self::Error>;

    /// Clears inactive orders from the local orders whose status is neither [`Status::New`] nor
    /// [`Status::PartiallyFilled`].
    fn clear_inactive_orders(&mut self, asset_no: Option<usize>);

    /// Elapses the specified duration.
    ///
    /// Args:
    /// * `duration` - Duration to elapse. Nanoseconds is the default unit. However, unit should be
    ///                the same as the data's timestamp unit.
    ///
    /// Returns:
    ///   `Ok(true)` if the method reaches the specified timestamp within the data. If the end of
    ///   the data is reached before the specified timestamp, it returns `Ok(false)`.
    fn elapse(&mut self, duration: i64) -> Result<bool, Self::Error>;

    /// Elapses time only in backtesting. In live mode, it is ignored.
    ///
    /// The [elapse()](Self::elapse()) method exclusively manages time during backtesting, meaning
    /// that factors such as computing time are not properly accounted for. So, this method can be
    /// utilized to simulate such processing times.
    ///
    /// Args:
    /// * `duration` - Duration to elapse. Nanoseconds is the default unit. However, unit should be
    ///                the same as the data's timestamp unit.
    ///
    /// Returns:
    ///   `Ok(true)` if the method reaches the specified timestamp within the data. If the end of
    ///   the data is reached before the specified timestamp, it returns `Ok(false)`.
    fn elapse_bt(&mut self, duration: i64) -> Result<bool, Self::Error>;

    /// Closes this backtester or bot.
    fn close(&mut self) -> Result<(), Self::Error>;
}
