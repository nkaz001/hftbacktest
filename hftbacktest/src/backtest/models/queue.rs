use std::{
    any::Any,
    collections::{hash_map::Entry, HashMap},
    marker::PhantomData,
};

use crate::{
    backtest::BacktestError,
    depth::MarketDepth,
    types::{AnyClone, Order, Side},
};

/// Provides an estimation of the order's queue position.
pub trait QueueModel<MD>
where
    MD: MarketDepth,
{
    /// Initialize the queue position and other necessary values for estimation.
    /// This function is called when the exchange model accepts the new order.
    fn new_order(&self, order: &mut Order, depth: &MD);

    /// Adjusts the estimation values when market trades occur at the same price.
    fn trade(&self, order: &mut Order, qty: f64, depth: &MD);

    /// Adjusts the estimation values when market depth changes at the same price.
    fn depth(&self, order: &mut Order, prev_qty: f64, new_qty: f64, depth: &MD);

    fn is_filled(&self, order: &Order, depth: &MD) -> f64;
}

/// Provides a conservative queue position model, where your order's queue position advances only
/// when trades occur at the same price level.
pub struct RiskAdverseQueueModel<MD>(PhantomData<MD>);

impl AnyClone for f64 {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl<MD> RiskAdverseQueueModel<MD> {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<MD> QueueModel<MD> for RiskAdverseQueueModel<MD>
where
    MD: MarketDepth,
{
    fn new_order(&self, order: &mut Order, depth: &MD) {
        let front_q_qty = if order.side == Side::Buy {
            depth.bid_qty_at_tick(order.price_tick)
        } else {
            depth.ask_qty_at_tick(order.price_tick)
        };
        order.q = Box::new(front_q_qty);
    }

    fn trade(&self, order: &mut Order, qty: f64, _depth: &MD) {
        let front_q_qty = order.q.as_any_mut().downcast_mut::<f64>().unwrap();
        *front_q_qty -= qty;
    }

    fn depth(&self, order: &mut Order, _prev_qty: f64, new_qty: f64, _depth: &MD) {
        let front_q_qty = order.q.as_any_mut().downcast_mut::<f64>().unwrap();
        *front_q_qty = front_q_qty.min(new_qty);
    }

    fn is_filled(&self, order: &Order, depth: &MD) -> f64 {
        let front_q_qty = order.q.as_any().downcast_ref::<f64>().unwrap();
        if (front_q_qty / depth.lot_size()).round() < 0.0 {
            (-front_q_qty / depth.lot_size()).floor() * depth.lot_size()
        } else {
            0.0
        }
    }
}

/// Stores the values needed for queue position estimation and adjustment for [`ProbQueueModel`].
#[derive(Clone)]
pub struct QueuePos {
    front_q_qty: f64,
    cum_trade_qty: f64,
}

impl AnyClone for QueuePos {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Default for QueuePos {
    fn default() -> Self {
        Self {
            front_q_qty: 0.0,
            cum_trade_qty: 0.0,
        }
    }
}

/// Provides the probability of a decrease behind the order's queue position.
pub trait Probability {
    /// Returns the probability based on the quantity ahead and behind the order.
    fn prob(&self, front: f64, back: f64) -> f64;
}

/// Provides a probability-based queue position model as described in
/// * `<https://quant.stackexchange.com/questions/3782/how-do-we-estimate-position-of-our-order-in-order-book>`
/// * `<https://rigtorp.se/2013/06/08/estimating-order-queue-position.html>`
///
/// Your order's queue position advances when a trade occurs at the same price level or the quantity
/// at the level decreases. The advancement in queue position depends on the probability based on
/// the relative queue position. To avoid double counting the quantity decrease caused by trades,
/// all trade quantities occurring at the level before the book quantity changes will be subtracted
/// from the book quantity changes.
pub struct ProbQueueModel<P, MD>
where
    P: Probability,
{
    prob: P,
    _md_marker: PhantomData<MD>,
}

impl<P, MD> ProbQueueModel<P, MD>
where
    P: Probability,
{
    /// Constructs an instance of `ProbQueueModel` with a [`Probability`] model.
    pub fn new(prob: P) -> Self {
        Self {
            prob,
            _md_marker: Default::default(),
        }
    }
}

impl<P, MD> QueueModel<MD> for ProbQueueModel<P, MD>
where
    P: Probability,
    MD: MarketDepth,
{
    fn new_order(&self, order: &mut Order, depth: &MD) {
        let mut q = QueuePos::default();
        if order.side == Side::Buy {
            q.front_q_qty = depth.bid_qty_at_tick(order.price_tick);
        } else {
            q.front_q_qty = depth.ask_qty_at_tick(order.price_tick);
        }
        order.q = Box::new(q);
    }

    fn trade(&self, order: &mut Order, qty: f64, _depth: &MD) {
        let q = order.q.as_any_mut().downcast_mut::<QueuePos>().unwrap();
        q.front_q_qty -= qty;
        q.cum_trade_qty += qty;
    }

    fn depth(&self, order: &mut Order, prev_qty: f64, new_qty: f64, _depth: &MD) {
        let mut chg = prev_qty - new_qty;
        // In order to avoid duplicate order queue position adjustment, subtract queue position
        // change by trades.
        let q = order.q.as_any_mut().downcast_mut::<QueuePos>().unwrap();
        chg -= q.cum_trade_qty;
        // Reset, as quantity change by trade should be already reflected in qty.
        q.cum_trade_qty = 0.0;
        // For an increase of the quantity, front queue doesn't change by the quantity change.
        if chg < 0.0 {
            q.front_q_qty = q.front_q_qty.min(new_qty);
            return;
        }

        let front = q.front_q_qty;
        let back = prev_qty - front;

        let mut prob = self.prob.prob(front, back);
        if prob.is_infinite() {
            prob = 1.0;
        }

        let est_front = front - (1.0 - prob) * chg + (back - prob * chg).min(0.0);
        q.front_q_qty = est_front.min(new_qty);
    }

    fn is_filled(&self, order: &Order, depth: &MD) -> f64 {
        let q = order.q.as_any().downcast_ref::<QueuePos>().unwrap();
        if (q.front_q_qty / depth.lot_size()).round() < 0.0 {
            (-q.front_q_qty / depth.lot_size()).floor() * depth.lot_size()
        } else {
            0.0
        }
    }
}

/// This probability model uses a power function `f(x) = x ** n` to adjust the probability which is
/// calculated as `f(back) / (f(back) + f(front))`.
pub struct PowerProbQueueFunc {
    n: f64,
}

impl PowerProbQueueFunc {
    /// Constructs an instance of `PowerProbQueueFunc`.
    pub fn new(n: f64) -> Self {
        Self { n }
    }

    fn f(&self, x: f64) -> f64 {
        x.powf(self.n)
    }
}

impl Probability for PowerProbQueueFunc {
    fn prob(&self, front: f64, back: f64) -> f64 {
        self.f(back) / (self.f(back) + self.f(front))
    }
}

/// This probability model uses a logarithmic function `f(x) = log(1 + x)` to adjust the
/// probability which is calculated as `f(back) / (f(back) + f(front))`.
#[derive(Default)]
pub struct LogProbQueueFunc(());

impl LogProbQueueFunc {
    /// Constructs an instance of `LogProbQueueFunc`.
    pub fn new() -> Self {
        Default::default()
    }

    fn f(&self, x: f64) -> f64 {
        (1.0 + x).ln()
    }
}

impl Probability for LogProbQueueFunc {
    fn prob(&self, front: f64, back: f64) -> f64 {
        self.f(back) / (self.f(back) + self.f(front))
    }
}

/// This probability model uses a logarithmic function `f(x) = log(1 + x)` to adjust the
/// probability which is calculated as `f(back) / f(back + front)`.
#[derive(Default)]
pub struct LogProbQueueFunc2(());

impl LogProbQueueFunc2 {
    /// Constructs an instance of `LogProbQueueFunc2`.
    pub fn new() -> Self {
        Default::default()
    }

    fn f(&self, x: f64) -> f64 {
        (1.0 + x).ln()
    }
}

impl Probability for LogProbQueueFunc2 {
    fn prob(&self, front: f64, back: f64) -> f64 {
        self.f(back) / self.f(back + front)
    }
}

/// This probability model uses a power function `f(x) = x ** n` to adjust the probability which is
/// calculated as `f(back) / f(back + front)`.
pub struct PowerProbQueueFunc2 {
    n: f64,
}

impl PowerProbQueueFunc2 {
    /// Constructs an instance of `PowerProbQueueFunc2`.
    pub fn new(n: f64) -> Self {
        Self { n }
    }

    fn f(&self, x: f64) -> f64 {
        x.powf(self.n)
    }
}

impl Probability for PowerProbQueueFunc2 {
    fn prob(&self, front: f64, back: f64) -> f64 {
        self.f(back) / self.f(back + front)
    }
}

/// This probability model uses a power function `f(x) = x ** n` to adjust the probability which is
/// calculated as `1 - f(front / (front + back))`.
pub struct PowerProbQueueFunc3 {
    n: f64,
}

impl PowerProbQueueFunc3 {
    /// Constructs an instance of `PowerProbQueueFunc3`.
    pub fn new(n: f64) -> Self {
        Self { n }
    }

    fn f(&self, x: f64) -> f64 {
        x.powf(self.n)
    }
}

impl Probability for PowerProbQueueFunc3 {
    fn prob(&self, front: f64, back: f64) -> f64 {
        1.0 - self.f(front / (front + back))
    }
}

/// Represents the order source for the Level 3 Market-By-Order queue model, which is stored in
/// [`order.q`](crate::types::Order::q)
#[derive(Copy, Clone, Eq, PartialEq)]
pub enum L3OrderSource {
    /// Represents an order originating from the market feed.
    Market,
    /// Represents an order originating from the backtest.
    Backtest,
}

impl AnyClone for L3OrderSource {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Order ID with the order source for Level 3 Market-By-Order.
#[derive(Hash, Eq, PartialEq)]
pub enum L3OrderId {
    /// Represents an order ID originating from the market feed.
    Market(u64),
    /// Represents an order ID originating from the backtest.
    Backtest(u64),
}

impl L3OrderId {
    pub fn is(&self, order: &Order) -> bool {
        let order_source = order.q.as_any().downcast_ref::<L3OrderSource>().unwrap();
        match self {
            L3OrderId::Market(order_id) => {
                order.order_id == *order_id && *order_source == L3OrderSource::Market
            }
            L3OrderId::Backtest(order_id) => {
                order.order_id == *order_id && *order_source == L3OrderSource::Backtest
            }
        }
    }
}

/// Provides an estimation of the order's queue position for Level 3 Market-By-Order feed.
pub trait L3QueueModel {
    type Error;

    /// This function is called when an order is added.
    fn add_order(&mut self, order_id: L3OrderId, order: Order) -> Result<(), Self::Error>;

    /// This function is called when an order is canceled.
    /// It does not necessarily mean that the order is canceled by the person who submitted it. It
    /// may simply mean that the order has been deleted in the market.
    fn cancel_order(&mut self, order_id: L3OrderId) -> Result<Order, Self::Error>;

    /// This function is called when an order is modified.
    fn modify_order(&mut self, order_id: L3OrderId, order: Order) -> Result<(), Self::Error>;

    /// This function is called when an order is filled.
    /// According to the exchange, the market feed may send fill and delete order events separately.
    /// This means that after a fill event is received, a delete order event can be received
    /// subsequently. The `delete` argument is used to indicate whether the order should be deleted
    /// immediately or if it should be deleted upon receiving a delete order event, which is handled
    /// by [`cancel_order`](L3QueueModel::cancel_order).
    fn fill(&mut self, order_id: L3OrderId, delete: bool) -> Result<Vec<Order>, Self::Error>;
}

/// This provides a Level 3 Market-By-Order queue model for backtesting in a FIFO manner. This means
/// that all orders, including backtest orders, are managed in a FIFO queue based on price-time
/// priority and executed in the FIFO order. Backtest orders are assumed to be executed in the queue
/// when the market order, order from the market feed, behind the backtest order is executed.
/// Exchanges may have different matching algorithms, such as Pro-Rata, and may have exotic order
/// types that aren't executed in a FIFO manner. Therefore, you should carefully choose the queue
/// model, even when dealing with a Level 3 Market-By-Order feed.
#[derive(Default)]
pub struct L3FIFOQueueModel {
    // Stores the location of the queue that holds the order by (side, price in ticks).
    pub orders: HashMap<L3OrderId, (Side, i64)>,
    // Since LinkedList's cursor is still unstable, there is no efficient way to delete an item in a
    // linked list, so it is better to use a vector.
    pub bid_queue: HashMap<i64, Vec<Order>>,
    pub ask_queue: HashMap<i64, Vec<Order>>,
}

impl L3FIFOQueueModel {
    /// Constructs an instance of `L3FIFOQueueModel`.
    pub fn new() -> Self {
        Default::default()
    }
}

impl L3QueueModel for L3FIFOQueueModel {
    type Error = BacktestError;

    fn add_order(&mut self, order_id: L3OrderId, order: Order) -> Result<(), Self::Error> {
        let order_price_tick = order.price_tick;
        let side = order.side;

        let priority = match side {
            Side::Buy => self.bid_queue.entry(order_price_tick).or_default(),
            Side::Sell => self.ask_queue.entry(order_price_tick).or_default(),
            Side::None | Side::Unsupported => unreachable!(),
        };

        priority.push(order);
        match self.orders.entry(order_id) {
            Entry::Occupied(_) => Err(BacktestError::OrderIdExist),
            Entry::Vacant(entry) => {
                entry.insert((side, order_price_tick));
                Ok(())
            }
        }
    }

    fn cancel_order(&mut self, order_id: L3OrderId) -> Result<Order, Self::Error> {
        let (side, order_price_tick) = self
            .orders
            .remove(&order_id)
            .ok_or(BacktestError::OrderNotFound)?;

        match side {
            Side::Buy => {
                let queue = self.bid_queue.get_mut(&order_price_tick).unwrap();
                let mut pos = None;
                for (i, order_in_q) in queue.iter().enumerate() {
                    if order_id.is(order_in_q) {
                        pos = Some(i);
                        break;
                    }
                }

                let order = queue.remove(pos.ok_or(BacktestError::OrderNotFound)?);
                // if priority.len() == 0 {
                //     self.bid_priority.remove(&order_price_tick);
                // }
                Ok(order)
            }
            Side::Sell => {
                let queue = self.ask_queue.get_mut(&order_price_tick).unwrap();
                let mut pos = None;
                for (i, order_in_q) in queue.iter().enumerate() {
                    if order_id.is(order_in_q) {
                        pos = Some(i);
                        break;
                    }
                }

                let order = queue.remove(pos.ok_or(BacktestError::OrderNotFound)?);
                // if priority.len() == 0 {
                //     self.ask_priority.remove(&order_price_tick);
                // }
                Ok(order)
            }
            Side::None | Side::Unsupported => unreachable!(),
        }
    }

    fn modify_order(&mut self, order_id: L3OrderId, order: Order) -> Result<(), Self::Error> {
        let (side, order_price_tick) = self
            .orders
            .get(&order_id)
            .ok_or(BacktestError::OrderNotFound)?;

        match side {
            Side::Buy => {
                let queue = self.bid_queue.get_mut(order_price_tick).unwrap();
                let mut processed = false;
                let mut pos = None;
                for (i, order_in_q) in queue.iter_mut().enumerate() {
                    if order_id.is(order_in_q) {
                        if (order_in_q.price_tick != order.price_tick)
                            || (order_in_q.leaves_qty < order.leaves_qty)
                        {
                            pos = Some(i);
                        } else {
                            order_in_q.leaves_qty = order.leaves_qty;
                            order_in_q.qty = order.qty;
                        }
                        processed = true;
                        break;
                    }
                }

                if !processed {
                    return Err(BacktestError::OrderNotFound);
                }

                if let Some(pos) = pos {
                    let prev_order = queue.remove(pos);
                    // if priority.len() == 0 {
                    //     self.bid_priority.remove(&order_price_tick);
                    // }
                    if prev_order.price_tick != order.price_tick {
                        let queue_ = self.bid_queue.get_mut(&order.price_tick).unwrap();
                        queue_.push(order);
                    } else {
                        queue.push(order);
                    }
                }
            }
            Side::Sell => {
                let queue = self.ask_queue.get_mut(order_price_tick).unwrap();
                let mut processed = false;
                let mut pos = None;
                for (i, order_in_q) in queue.iter_mut().enumerate() {
                    if order_id.is(order_in_q) {
                        if (order_in_q.price_tick != order.price_tick)
                            || (order_in_q.leaves_qty < order.leaves_qty)
                        {
                            pos = Some(i);
                        } else {
                            order_in_q.leaves_qty = order.leaves_qty;
                            order_in_q.qty = order.qty;
                        }
                        processed = true;
                        break;
                    }
                }

                if !processed {
                    return Err(BacktestError::OrderNotFound);
                }

                if let Some(pos) = pos {
                    let prev_order = queue.remove(pos);
                    // if priority.len() == 0 {
                    //     self.bid_priority.remove(&order_price_tick);
                    // }
                    if prev_order.price_tick != order.price_tick {
                        let queue_ = self.ask_queue.get_mut(&order.price_tick).unwrap();
                        queue_.push(order);
                    } else {
                        queue.push(order);
                    }
                }
            }
            Side::None | Side::Unsupported => unreachable!(),
        }

        Ok(())
    }

    fn fill(&mut self, order_id: L3OrderId, delete: bool) -> Result<Vec<Order>, Self::Error> {
        let (side, order_price_tick) = self
            .orders
            .remove(&order_id)
            .ok_or(BacktestError::OrderNotFound)?;

        match side {
            Side::Buy => {
                let queue = self.bid_queue.get_mut(&order_price_tick).unwrap();
                let mut filled = Vec::new();

                let mut pos = None;
                let mut i = 0;
                while i < queue.len() {
                    let order_in_q = queue.get(i).unwrap();
                    let order_source = order_in_q
                        .q
                        .as_any()
                        .downcast_ref::<L3OrderSource>()
                        .unwrap();
                    if order_id.is(order_in_q) {
                        pos = Some(i);
                        break;
                    } else if *order_source == L3OrderSource::Backtest {
                        let order = queue.remove(i);
                        filled.push(order);
                    } else {
                        i += 1;
                    }
                }
                let pos = pos.ok_or(BacktestError::OrderNotFound)?;
                if delete {
                    queue.remove(pos);
                    // if priority.len() == 0 {
                    //     self.ask_priority.remove(&order_price_tick);
                    // }
                }
                Ok(filled)
            }
            Side::Sell => {
                let queue = self.ask_queue.get_mut(&order_price_tick).unwrap();
                let mut filled = Vec::new();

                let mut pos = None;
                let mut i = 0;
                while i < queue.len() {
                    let order_in_q = queue.get(i).unwrap();
                    let order_source = order_in_q
                        .q
                        .as_any()
                        .downcast_ref::<L3OrderSource>()
                        .unwrap();
                    if order_id.is(order_in_q) {
                        pos = Some(i);
                        break;
                    } else if *order_source == L3OrderSource::Backtest {
                        let order = queue.remove(i);
                        filled.push(order);
                    } else {
                        i += 1;
                    }
                }
                let pos = pos.ok_or(BacktestError::OrderNotFound)?;
                if delete {
                    queue.remove(pos);
                    // if priority.len() == 0 {
                    //     self.ask_priority.remove(&order_price_tick);
                    // }
                }
                Ok(filled)
            }
            Side::None | Side::Unsupported => unreachable!(),
        }
    }
}
