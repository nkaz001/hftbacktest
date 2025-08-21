use std::{
    any::Any,
    collections::{HashMap, HashSet, VecDeque, hash_map::Entry},
    marker::PhantomData,
};

use crate::{
    backtest::BacktestError,
    depth::{INVALID_MAX, INVALID_MIN, MarketDepth},
    types::{
        AnyClone,
        BUY_EVENT,
        Event,
        OrdType,
        Order,
        OrderId,
        SELL_EVENT,
        Side,
        Status,
        TimeInForce,
    },
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

    fn is_filled(&self, order: &mut Order, depth: &MD) -> f64;
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

    fn is_filled(&self, order: &mut Order, depth: &MD) -> f64 {
        let front_q_qty = order.q.as_any_mut().downcast_mut::<f64>().unwrap();
        let exec = (-*front_q_qty / depth.lot_size()).round() as i64;
        if exec > 0 {
            *front_q_qty = 0.0;
            (exec as f64) * depth.lot_size()
        } else {
            0.0
        }
    }
}

/// Stores the values needed for queue position estimation and adjustment for [`ProbQueueModel`].
#[derive(Clone, Debug)]
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

    fn is_filled(&self, order: &mut Order, depth: &MD) -> f64 {
        let q = order.q.as_any_mut().downcast_mut::<QueuePos>().unwrap();
        let exec = (-q.front_q_qty / depth.lot_size()).round() as i64;
        if exec > 0 {
            q.front_q_qty = 0.0;
            (exec as f64) * depth.lot_size()
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
enum L3OrderSource {
    /// Represents an order originating from the market feed.
    MarketFeed,
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

trait L3Order {
    fn order_source(&self) -> L3OrderSource;

    fn is_backtest_order(&self) -> bool;

    fn is_market_feed_order(&self) -> bool;
}

impl L3Order for Order {
    fn order_source(&self) -> L3OrderSource {
        *self.q.as_any().downcast_ref::<L3OrderSource>().unwrap()
    }

    fn is_backtest_order(&self) -> bool {
        match self.order_source() {
            L3OrderSource::MarketFeed => false,
            L3OrderSource::Backtest => true,
        }
    }

    fn is_market_feed_order(&self) -> bool {
        match self.order_source() {
            L3OrderSource::MarketFeed => true,
            L3OrderSource::Backtest => false,
        }
    }
}

/// Provides a model to determine whether the backtest order is filled, accounting for the queue
/// position based on L3 Market-By-Order data.
pub trait L3QueueModel<MD> {
    /// Returns `true` if the queue contains a backtest order for the order ID.
    fn contains_backtest_order(&self, order_id: OrderId) -> bool;

    /// Invoked when the best bid is updated.
    /// Returns the ask backtest orders that are filled by crossing the best bid.
    fn on_best_bid_update(
        &mut self,
        prev_best_tick: i64,
        new_best_tick: i64,
    ) -> Result<Vec<Order>, BacktestError>;

    /// Invoked when the best ask is updated.
    /// Returns the bid backtest orders that are filled by crossing the best ask.
    fn on_best_ask_update(
        &mut self,
        prev_best_tick: i64,
        new_best_tick: i64,
    ) -> Result<Vec<Order>, BacktestError>;

    /// Invoked when a backtest order is added.
    fn add_backtest_order(&mut self, order: Order, depth: &MD) -> Result<(), BacktestError>;

    /// Invoked when an order is added from the market feed.
    fn add_market_feed_order(&mut self, order: &Event, depth: &MD) -> Result<(), BacktestError>;

    /// Invoked when a backtest order is canceled.
    fn cancel_backtest_order(
        &mut self,
        order_id: OrderId,
        depth: &MD,
    ) -> Result<Order, BacktestError>;

    /// Invoked when an order is canceled from the market feed.
    ///
    /// It does not necessarily mean that the order is canceled by the one who submitted it. It may
    /// simply mean that the order has been deleted in the market.
    fn cancel_market_feed_order(
        &mut self,
        order_id: OrderId,
        depth: &MD,
    ) -> Result<(), BacktestError>;

    /// Invoked when a backtest order is modified.
    fn modify_backtest_order(
        &mut self,
        order_id: OrderId,
        order: &mut Order,
        depth: &MD,
    ) -> Result<(), BacktestError>;

    /// Invoked when an order is modified from the market feed.
    fn modify_market_feed_order(
        &mut self,
        order_id: OrderId,
        order: &Event,
        depth: &MD,
    ) -> Result<(), BacktestError>;

    /// Invoked when an order is filled from the market feed.
    ///
    /// According to the exchange, the market feed may send fill and delete order events separately.
    /// This means that after a fill event is received, a delete order event can be received
    /// subsequently. The `DELETE` constant generic is used to indicate whether the order should be
    /// deleted immediately or if it should be deleted upon receiving a delete order event, which is
    /// handled by [`cancel_market_feed_order`](L3QueueModel::cancel_market_feed_order).
    fn fill_market_feed_order<const DELETE: bool>(
        &mut self,
        order_id: OrderId,
        order: &Event,
        depth: &MD,
    ) -> Result<Vec<Order>, BacktestError>;

    /// Invoked when a clear order message is received. Returns the expired orders due to the clear
    /// message.
    ///
    /// Such messages can occur in two scenarios:
    ///
    /// 1. The exchange clears orders for reasons such as session close.
    ///    In this case, backtest orders must also be cleared.
    /// 2. The exchange sends a clear message before sending a snapshot to properly maintain market
    ///    depth. Here, clearing backtest orders is not always necessary. However, estimating the
    ///    order queue position becomes difficult because clearing the market feed orders leads to
    ///    the loss of queue position information. Additionally, there's no guarantee that all
    ///    orders preceding the clear message will persist.
    ///
    /// Due to these challenges, HftBacktest opts to clear all backtest orders upon receiving a
    /// clear message, even though this may differ from the exchange's actual behavior.
    fn clear_orders(&mut self, side: Side) -> Vec<Order>;
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
    pub backtest_orders: HashMap<OrderId, (Side, i64)>,
    pub mkt_feed_orders: HashMap<OrderId, (Side, i64)>,
    // Since LinkedList's cursor is still unstable, there is no efficient way to delete an item in a
    // linked list, so it is better to use a vector.
    pub bid_queue: HashMap<i64, VecDeque<Order>>,
    pub ask_queue: HashMap<i64, VecDeque<Order>>,
}

impl L3FIFOQueueModel {
    /// Constructs an instance of `L3FIFOQueueModel`.
    pub fn new() -> Self {
        Default::default()
    }

    fn fill_bid_between<const INVALID_FROM: bool>(
        &mut self,
        from_tick: i64,
        to_tick: i64,
    ) -> Vec<Order> {
        assert!(to_tick <= from_tick);
        // Finds the shortest iteration.
        let mut filled = Vec::new();
        if INVALID_FROM || (self.backtest_orders.len() as i64) < from_tick - to_tick {
            let mut filled_tick = HashSet::new();
            self.backtest_orders.retain(|_, (side, order_price_tick)| {
                if *side == Side::Buy && *order_price_tick >= to_tick {
                    filled_tick.insert(*order_price_tick);
                    false
                } else {
                    true
                }
            });
            for order_price_tick in filled_tick {
                let queue = self.bid_queue.get_mut(&order_price_tick).unwrap();
                queue.retain(|order| {
                    if order.is_backtest_order() {
                        filled.push(order.clone());
                        false
                    } else {
                        true
                    }
                });
            }
        } else {
            for t in to_tick..(from_tick + 1) {
                if let Some(queue) = self.bid_queue.get_mut(&t) {
                    queue.retain(|order| {
                        if order.is_backtest_order() {
                            self.backtest_orders.remove(&order.order_id);
                            filled.push(order.clone());
                            false
                        } else {
                            true
                        }
                    });
                }
            }
        }
        filled
    }

    fn fill_ask_between<const INVALID_FROM: bool>(
        &mut self,
        from_tick: i64,
        to_tick: i64,
    ) -> Vec<Order> {
        assert!(from_tick <= to_tick);
        // Finds the shortest iteration.
        let mut filled = Vec::new();
        if INVALID_FROM || (self.backtest_orders.len() as i64) < to_tick - from_tick {
            let mut filled_tick = HashSet::new();
            self.backtest_orders.retain(|_, (side, order_price_tick)| {
                if *side == Side::Sell && *order_price_tick <= to_tick {
                    filled_tick.insert(*order_price_tick);
                    false
                } else {
                    true
                }
            });
            for order_price_tick in filled_tick {
                let queue = self.ask_queue.get_mut(&order_price_tick).unwrap();
                queue.retain(|order| {
                    if order.is_backtest_order() {
                        filled.push(order.clone());
                        false
                    } else {
                        true
                    }
                });
            }
        } else {
            for t in from_tick..(to_tick + 1) {
                if let Some(queue) = self.ask_queue.get_mut(&t) {
                    queue.retain(|order| {
                        if order.is_backtest_order() {
                            self.backtest_orders.remove(&order.order_id);
                            filled.push(order.clone());
                            false
                        } else {
                            true
                        }
                    });
                }
            }
        }
        filled
    }
}

impl<MD> L3QueueModel<MD> for L3FIFOQueueModel
where
    MD: MarketDepth,
{
    fn contains_backtest_order(&self, order_id: OrderId) -> bool {
        self.backtest_orders.contains_key(&order_id)
    }

    fn on_best_bid_update(
        &mut self,
        prev_best_tick: i64,
        new_best_tick: i64,
    ) -> Result<Vec<Order>, BacktestError> {
        if prev_best_tick == INVALID_MIN {
            Ok(self.fill_ask_between::<true>(prev_best_tick + 1, new_best_tick))
        } else {
            Ok(self.fill_ask_between::<false>(prev_best_tick + 1, new_best_tick))
        }
    }

    fn on_best_ask_update(
        &mut self,
        prev_best_tick: i64,
        new_best_tick: i64,
    ) -> Result<Vec<Order>, BacktestError> {
        if prev_best_tick == INVALID_MAX {
            Ok(self.fill_bid_between::<true>(prev_best_tick - 1, new_best_tick))
        } else {
            Ok(self.fill_bid_between::<false>(prev_best_tick - 1, new_best_tick))
        }
    }

    fn add_backtest_order(&mut self, mut order: Order, _depth: &MD) -> Result<(), BacktestError> {
        let order_price_tick = order.price_tick;
        let side = order.side;
        let order_id = order.order_id;

        order.q = Box::new(L3OrderSource::Backtest);

        let queue = match side {
            Side::Buy => self.bid_queue.entry(order_price_tick).or_default(),
            Side::Sell => self.ask_queue.entry(order_price_tick).or_default(),
            Side::None | Side::Unsupported => unreachable!(),
        };

        queue.push_back(order);

        match self.backtest_orders.entry(order_id) {
            Entry::Occupied(_) => Err(BacktestError::OrderIdExist),
            Entry::Vacant(entry) => {
                entry.insert((side, order_price_tick));
                Ok(())
            }
        }
    }

    fn add_market_feed_order(&mut self, order: &Event, depth: &MD) -> Result<(), BacktestError> {
        let tick_size = depth.tick_size();
        let order_price_tick = (order.px / tick_size).round() as i64;
        let side;
        let order_id = order.order_id;

        let queue = if order.is(BUY_EVENT) {
            side = Side::Buy;
            self.bid_queue.entry(order_price_tick).or_default()
        } else if order.is(SELL_EVENT) {
            side = Side::Sell;
            self.ask_queue.entry(order_price_tick).or_default()
        } else {
            unreachable!()
        };

        queue.push_back(Order {
            qty: order.qty,
            leaves_qty: order.qty,
            price_tick: order_price_tick,
            exch_timestamp: order.exch_ts,
            q: Box::new(L3OrderSource::MarketFeed),
            tick_size,
            order_id,
            side,
            // The information below is invalid.
            exec_qty: 0.0,
            exec_price_tick: 0,
            local_timestamp: 0,
            maker: false,
            order_type: OrdType::Limit,
            req: Status::None,
            status: Status::None,
            time_in_force: TimeInForce::GTC,
        });

        match self.mkt_feed_orders.entry(order_id) {
            Entry::Occupied(_) => Err(BacktestError::OrderIdExist),
            Entry::Vacant(entry) => {
                entry.insert((side, order_price_tick));
                Ok(())
            }
        }
    }

    fn cancel_backtest_order(
        &mut self,
        order_id: OrderId,
        _depth: &MD,
    ) -> Result<Order, BacktestError> {
        let (side, order_price_tick) = self
            .backtest_orders
            .remove(&order_id)
            .ok_or(BacktestError::OrderNotFound)?;

        match side {
            Side::Buy => {
                let queue = self.bid_queue.get_mut(&order_price_tick).unwrap();
                for i in 0..queue.len() {
                    if queue[i].is_backtest_order() && queue[i].order_id == order_id {
                        let order = queue.remove(i).unwrap();
                        // if queue.len() == 0 {
                        //     self.bid_queue.remove(&order_price_tick);
                        // }
                        return Ok(order);
                    }
                }
                unreachable!()
            }
            Side::Sell => {
                let queue = self.ask_queue.get_mut(&order_price_tick).unwrap();
                for i in 0..queue.len() {
                    if queue[i].is_backtest_order() && queue[i].order_id == order_id {
                        let order = queue.remove(i).unwrap();
                        // if queue.len() == 0 {
                        //     self.ask_queue.remove(&order_price_tick);
                        // }
                        return Ok(order);
                    }
                }
                unreachable!()
            }
            Side::None | Side::Unsupported => unreachable!(),
        }
    }

    fn cancel_market_feed_order(
        &mut self,
        order_id: OrderId,
        _depth: &MD,
    ) -> Result<(), BacktestError> {
        let (side, order_price_tick) = self
            .mkt_feed_orders
            .remove(&order_id)
            .ok_or(BacktestError::OrderNotFound)?;

        match side {
            Side::Buy => {
                let queue = self.bid_queue.get_mut(&order_price_tick).unwrap();
                for i in 0..queue.len() {
                    if queue[i].is_market_feed_order() && queue[i].order_id == order_id {
                        queue.remove(i);
                        // if queue.len() == 0 {
                        //     self.bid_queue.remove(&order_price_tick);
                        // }
                        return Ok(());
                    }
                }
                unreachable!()
            }
            Side::Sell => {
                let queue = self.ask_queue.get_mut(&order_price_tick).unwrap();
                for i in 0..queue.len() {
                    if queue[i].is_market_feed_order() && queue[i].order_id == order_id {
                        queue.remove(i);
                        // if queue.len() == 0 {
                        //     self.ask_queue.remove(&order_price_tick);
                        // }
                        return Ok(());
                    }
                }
                unreachable!()
            }
            Side::None | Side::Unsupported => unreachable!(),
        }
    }

    fn modify_backtest_order(
        &mut self,
        order_id: OrderId,
        order: &mut Order,
        _depth: &MD,
    ) -> Result<(), BacktestError> {
        order.q = Box::new(L3OrderSource::Backtest);

        let (side, order_price_tick) = self
            .backtest_orders
            .get_mut(&order_id)
            .ok_or(BacktestError::OrderNotFound)?;

        match side {
            Side::Buy => {
                let queue = self.bid_queue.get_mut(order_price_tick).unwrap();
                let mut processed = false;
                for i in 0..queue.len() {
                    let order_in_q = queue.get_mut(i).unwrap();
                    if order_in_q.is_backtest_order() && order_in_q.order_id == order_id {
                        if (order_in_q.price_tick != order.price_tick)
                            || (order_in_q.leaves_qty < order.qty)
                        {
                            let mut prev_order = queue.remove(i).unwrap();
                            let prev_order_price_tick = prev_order.price_tick;
                            prev_order.update(order);
                            prev_order.leaves_qty = prev_order.qty;
                            // if queue.len() == 0 {
                            //     self.bid_queue.remove(&order_price_tick);
                            // }
                            if prev_order_price_tick != order.price_tick {
                                *order_price_tick = order.price_tick;
                                let queue_ =
                                    self.bid_queue.entry(prev_order.price_tick).or_default();
                                queue_.push_back(prev_order);
                            } else {
                                queue.push_back(prev_order);
                            }
                        } else {
                            order_in_q.qty = order.qty;
                            order_in_q.leaves_qty = order.qty;
                            order_in_q.exch_timestamp = order.exch_timestamp;
                        }
                        processed = true;
                        break;
                    }
                }
                if !processed {
                    return Err(BacktestError::OrderNotFound);
                }
            }
            Side::Sell => {
                let queue = self.ask_queue.get_mut(order_price_tick).unwrap();
                let mut processed = false;
                for i in 0..queue.len() {
                    let order_in_q = queue.get_mut(i).unwrap();
                    if order_in_q.is_backtest_order() && order_in_q.order_id == order_id {
                        if (order_in_q.price_tick != order.price_tick)
                            || (order_in_q.leaves_qty < order.qty)
                        {
                            let mut prev_order = queue.remove(i).unwrap();
                            let prev_order_price_tick = prev_order.price_tick;
                            prev_order.update(order);
                            prev_order.leaves_qty = prev_order.qty;
                            // if queue.len() == 0 {
                            //     self.bid_queue.remove(&order_price_tick);
                            // }
                            if prev_order_price_tick != order.price_tick {
                                *order_price_tick = order.price_tick;
                                let queue_ =
                                    self.ask_queue.entry(prev_order.price_tick).or_default();
                                queue_.push_back(prev_order);
                            } else {
                                queue.push_back(prev_order);
                            }
                        } else {
                            order_in_q.qty = order.qty;
                            order_in_q.leaves_qty = order.qty;
                            order_in_q.exch_timestamp = order.exch_timestamp;
                        }
                        processed = true;
                        break;
                    }
                }
                if !processed {
                    return Err(BacktestError::OrderNotFound);
                }
            }
            Side::None | Side::Unsupported => unreachable!(),
        }

        Ok(())
    }

    fn modify_market_feed_order(
        &mut self,
        order_id: OrderId,
        order: &Event,
        depth: &MD,
    ) -> Result<(), BacktestError> {
        let (side, order_price_tick) = self
            .mkt_feed_orders
            .get_mut(&order_id)
            .ok_or(BacktestError::OrderNotFound)?;
        let new_price_tick = (order.px / depth.tick_size()).round() as i64;

        match side {
            Side::Buy => {
                let queue = self.bid_queue.get_mut(order_price_tick).unwrap();
                let mut processed = false;
                for i in 0..queue.len() {
                    let order_in_q = queue.get_mut(i).unwrap();
                    if order_in_q.is_market_feed_order() && order_in_q.order_id == order_id {
                        if (order_in_q.price_tick != new_price_tick)
                            || (order_in_q.leaves_qty < order.qty)
                        {
                            let mut prev_order = queue.remove(i).unwrap();
                            let prev_order_price_tick = prev_order.price_tick;
                            prev_order.price_tick = new_price_tick;
                            prev_order.leaves_qty = order.qty;
                            prev_order.qty = order.qty;
                            prev_order.exch_timestamp = order.exch_ts;
                            // if queue.len() == 0 {
                            //     self.bid_queue.remove(&order_price_tick);
                            // }
                            if prev_order_price_tick != new_price_tick {
                                *order_price_tick = new_price_tick;

                                let queue_ = self.bid_queue.entry(*order_price_tick).or_default();
                                queue_.push_back(prev_order);
                            } else {
                                queue.push_back(prev_order);
                            }
                        } else {
                            order_in_q.leaves_qty = order.qty;
                            order_in_q.qty = order.qty;
                            order_in_q.exch_timestamp = order.exch_ts;
                        }
                        processed = true;
                        break;
                    }
                }
                if !processed {
                    return Err(BacktestError::OrderNotFound);
                }
            }
            Side::Sell => {
                let queue = self.ask_queue.get_mut(order_price_tick).unwrap();
                let mut processed = false;
                for i in 0..queue.len() {
                    let order_in_q = queue.get_mut(i).unwrap();
                    if order_in_q.is_market_feed_order() && order_in_q.order_id == order_id {
                        if (order_in_q.price_tick != new_price_tick)
                            || (order_in_q.leaves_qty < order.qty)
                        {
                            let mut prev_order = queue.remove(i).unwrap();
                            let prev_order_price_tick = prev_order.price_tick;
                            prev_order.price_tick = new_price_tick;
                            prev_order.leaves_qty = order.qty;
                            prev_order.qty = order.qty;
                            prev_order.exch_timestamp = order.exch_ts;
                            // if queue.len() == 0 {
                            //     self.bid_queue.remove(&order_price_tick);
                            // }
                            if prev_order_price_tick != new_price_tick {
                                *order_price_tick = new_price_tick;

                                let queue_ = self.ask_queue.entry(*order_price_tick).or_default();
                                queue_.push_back(prev_order);
                            } else {
                                queue.push_back(prev_order);
                            }
                        } else {
                            order_in_q.leaves_qty = order.qty;
                            order_in_q.qty = order.qty;
                            order_in_q.exch_timestamp = order.exch_ts;
                        }
                        processed = true;
                        break;
                    }
                }
                if !processed {
                    return Err(BacktestError::OrderNotFound);
                }
            }
            Side::None | Side::Unsupported => unreachable!(),
        }

        Ok(())
    }

    fn fill_market_feed_order<const DELETE: bool>(
        &mut self,
        order_id: OrderId,
        order: &Event,
        depth: &MD,
    ) -> Result<Vec<Order>, BacktestError> {
        let (side, order_price_tick) = if DELETE {
            self.mkt_feed_orders
                .remove(&order_id)
                .ok_or(BacktestError::OrderNotFound)?
        } else {
            *self
                .mkt_feed_orders
                .get(&order_id)
                .ok_or(BacktestError::OrderNotFound)?
        };
        let exec_price_tick = (order.px / depth.tick_size()).round() as i64;

        match side {
            Side::Buy => {
                let mut filled = Vec::new();

                // The backtest bid orders above the price of the filled market-feed bid order are
                // filled.
                // The fill event should occur before the cancel event which may update the best
                // price.
                if exec_price_tick < depth.best_bid_tick() {
                    let mut f =
                        self.fill_bid_between::<false>(depth.best_bid_tick(), exec_price_tick + 1);
                    filled.append(&mut f);
                }

                // The backtest bid orders in the queue, placed before the filled market-feed bid
                // order, are filled.
                let queue = self.bid_queue.get_mut(&order_price_tick).unwrap();

                let mut i = 0;
                while i < queue.len() {
                    let order_in_q = queue.get(i).unwrap();
                    match order_in_q.order_source() {
                        L3OrderSource::MarketFeed if order_in_q.order_id == order_id => {
                            if DELETE {
                                queue.remove(i);
                            }
                            break;
                        }
                        L3OrderSource::MarketFeed => {
                            i += 1;
                        }
                        L3OrderSource::Backtest => {
                            let order = queue.remove(i).unwrap();
                            filled.push(order);
                        }
                    }
                }
                for order in &filled {
                    self.backtest_orders.remove(&order.order_id);
                }
                Ok(filled)
            }
            Side::Sell => {
                let mut filled = Vec::new();

                // The backtest ask orders below the price of the filled market-feed ask order are
                // filled.
                // The fill event should occur before the cancel event which may update the best
                // price.
                if exec_price_tick > depth.best_ask_tick() {
                    let mut f =
                        self.fill_ask_between::<false>(depth.best_ask_tick(), exec_price_tick - 1);
                    filled.append(&mut f);
                }

                // The backtest ask orders in the queue, placed before the filled market-feed ask
                // order, are filled.
                let queue = self.ask_queue.get_mut(&order_price_tick).unwrap();

                let mut i = 0;
                while i < queue.len() {
                    let order_in_q = queue.get(i).unwrap();
                    match order_in_q.order_source() {
                        L3OrderSource::MarketFeed if order_in_q.order_id == order_id => {
                            if DELETE {
                                queue.remove(i);
                            }
                            break;
                        }
                        L3OrderSource::MarketFeed => {
                            i += 1;
                        }
                        L3OrderSource::Backtest => {
                            let order = queue.remove(i).unwrap();
                            filled.push(order);
                        }
                    }
                }
                for order in &filled {
                    self.backtest_orders.remove(&order.order_id);
                }
                Ok(filled)
            }
            Side::None | Side::Unsupported => unreachable!(),
        }
    }

    fn clear_orders(&mut self, side: Side) -> Vec<Order> {
        match side {
            Side::Buy => {
                self.mkt_feed_orders
                    .retain(|_, (order_side, _)| *order_side != side);
                self.backtest_orders
                    .retain(|_, (order_side, _)| *order_side != side);

                self.bid_queue
                    .drain()
                    .flat_map(|(_, q)| q)
                    .filter(|order| order.is_backtest_order())
                    .collect()
            }
            Side::Sell => {
                self.mkt_feed_orders
                    .retain(|_, (order_side, _)| *order_side != side);
                self.backtest_orders
                    .retain(|_, (order_side, _)| *order_side != side);

                self.ask_queue
                    .drain()
                    .flat_map(|(_, q)| q)
                    .filter(|order| order.is_backtest_order())
                    .collect()
            }
            Side::None => {
                self.mkt_feed_orders.clear();
                self.backtest_orders.clear();

                let mut expired: Vec<_> = self
                    .bid_queue
                    .drain()
                    .flat_map(|(_, v)| v)
                    .filter(|order| order.is_backtest_order())
                    .collect();
                expired.extend(
                    self.ask_queue
                        .drain()
                        .flat_map(|(_, q)| q)
                        .filter(|order| order.is_backtest_order()),
                );
                expired
            }
            Side::Unsupported => {
                unreachable!()
            }
        }
    }
}

#[cfg(test)]
mod l3_tests {
    use crate::{
        backtest::{L3QueueModel, models::L3FIFOQueueModel},
        prelude::{
            Event,
            HashMapMarketDepth,
            L3MarketDepth,
            OrdType,
            Order,
            Side,
            Status,
            TimeInForce,
        },
        types::{ADD_ORDER_EVENT, BUY_EVENT, EXCH_EVENT, FILL_EVENT, SELL_EVENT},
    };

    #[test]
    fn fill_by_crossing() {
        let mut depth = HashMapMarketDepth::new(1.0, 1.0);
        let mut qm = L3FIFOQueueModel::new();

        let ev = Event {
            ev: EXCH_EVENT | BUY_EVENT | ADD_ORDER_EVENT,
            exch_ts: 0,
            local_ts: 0,
            px: 100.0,
            qty: 1.0,
            order_id: 1,
            ival: 0,
            fval: 0.0,
        };

        depth
            .add_buy_order(ev.order_id, ev.px, ev.qty, ev.exch_ts)
            .unwrap();
        qm.add_market_feed_order(&ev, &depth).unwrap();

        let ev = Event {
            ev: EXCH_EVENT | SELL_EVENT | ADD_ORDER_EVENT,
            exch_ts: 0,
            local_ts: 0,
            px: 101.0,
            qty: 1.0,
            order_id: 2,
            ival: 0,
            fval: 0.0,
        };

        depth
            .add_sell_order(ev.order_id, ev.px, ev.qty, ev.exch_ts)
            .unwrap();
        qm.add_market_feed_order(&ev, &depth).unwrap();

        qm.add_backtest_order(
            Order {
                qty: 1.0,
                leaves_qty: 0.0,
                exec_qty: 0.0,
                exec_price_tick: 0,
                price_tick: 100,
                tick_size: 1.0,
                exch_timestamp: 0,
                local_timestamp: 0,
                order_id: 1,
                q: Box::new(()),
                maker: false,
                order_type: OrdType::Limit,
                req: Status::None,
                status: Status::None,
                side: Side::Buy,
                time_in_force: TimeInForce::GTC,
            },
            &depth,
        )
        .unwrap();

        let filled = <L3FIFOQueueModel as L3QueueModel<HashMapMarketDepth>>::on_best_ask_update(
            &mut qm, 101, 100,
        )
        .unwrap();
        assert_eq!(filled.len(), 1);
        assert!(
            !<L3FIFOQueueModel as L3QueueModel<HashMapMarketDepth>>::contains_backtest_order(
                &qm, 1
            )
        );

        qm.add_backtest_order(
            Order {
                qty: 1.0,
                leaves_qty: 0.0,
                exec_qty: 0.0,
                exec_price_tick: 0,
                price_tick: 101,
                tick_size: 1.0,
                exch_timestamp: 0,
                local_timestamp: 0,
                order_id: 1,
                q: Box::new(()),
                maker: false,
                order_type: OrdType::Limit,
                req: Status::None,
                status: Status::None,
                side: Side::Sell,
                time_in_force: TimeInForce::GTC,
            },
            &depth,
        )
        .unwrap();

        let filled = <L3FIFOQueueModel as L3QueueModel<HashMapMarketDepth>>::on_best_bid_update(
            &mut qm, 100, 101,
        )
        .unwrap();
        assert_eq!(filled.len(), 1);
        assert!(
            !<L3FIFOQueueModel as L3QueueModel<HashMapMarketDepth>>::contains_backtest_order(
                &qm, 1
            )
        );
    }

    #[test]
    fn fill_in_queue() {
        let mut depth = HashMapMarketDepth::new(1.0, 1.0);
        let mut qm = L3FIFOQueueModel::new();

        let ev = Event {
            ev: EXCH_EVENT | BUY_EVENT | ADD_ORDER_EVENT,
            exch_ts: 0,
            local_ts: 0,
            px: 100.0,
            qty: 1.0,
            order_id: 1,
            ival: 0,
            fval: 0.0,
        };

        depth
            .add_buy_order(ev.order_id, ev.px, ev.qty, ev.exch_ts)
            .unwrap();
        qm.add_market_feed_order(&ev, &depth).unwrap();

        qm.add_backtest_order(
            Order {
                qty: 1.0,
                leaves_qty: 0.0,
                exec_qty: 0.0,
                exec_price_tick: 0,
                price_tick: 100,
                tick_size: 1.0,
                exch_timestamp: 0,
                local_timestamp: 0,
                order_id: 1,
                q: Box::new(()),
                maker: false,
                order_type: OrdType::Limit,
                req: Status::None,
                status: Status::None,
                side: Side::Buy,
                time_in_force: TimeInForce::GTC,
            },
            &depth,
        )
        .unwrap();

        let ev = Event {
            ev: EXCH_EVENT | BUY_EVENT | ADD_ORDER_EVENT,
            exch_ts: 0,
            local_ts: 0,
            px: 100.0,
            qty: 1.0,
            order_id: 2,
            ival: 0,
            fval: 0.0,
        };

        depth
            .add_buy_order(ev.order_id, ev.px, ev.qty, ev.exch_ts)
            .unwrap();
        qm.add_market_feed_order(&ev, &depth).unwrap();

        let ev = Event {
            ev: EXCH_EVENT | BUY_EVENT | ADD_ORDER_EVENT,
            exch_ts: 0,
            local_ts: 0,
            px: 100.0,
            qty: 1.0,
            order_id: 3,
            ival: 0,
            fval: 0.0,
        };

        depth
            .add_buy_order(ev.order_id, ev.px, ev.qty, ev.exch_ts)
            .unwrap();
        qm.add_market_feed_order(&ev, &depth).unwrap();

        depth.delete_order(1, 0).unwrap();
        //qm.cancel_market_feed_order(1, &depth).unwrap();

        let ev = Event {
            ev: EXCH_EVENT | BUY_EVENT | FILL_EVENT,
            exch_ts: 0,
            local_ts: 0,
            px: 100.0,
            qty: 1.0,
            order_id: 1,
            ival: 0,
            fval: 0.0,
        };

        let filled = qm.fill_market_feed_order::<false>(1, &ev, &depth).unwrap();
        assert_eq!(filled.len(), 0);

        let ev = Event {
            ev: EXCH_EVENT | BUY_EVENT | FILL_EVENT,
            exch_ts: 0,
            local_ts: 0,
            px: 100.0,
            qty: 1.0,
            order_id: 2,
            ival: 0,
            fval: 0.0,
        };

        depth.delete_order(2, 0).unwrap();
        let filled = qm.fill_market_feed_order::<false>(2, &ev, &depth).unwrap();
        assert_eq!(filled.len(), 1);
        assert!(
            !<L3FIFOQueueModel as L3QueueModel<HashMapMarketDepth>>::contains_backtest_order(
                &qm, 1
            )
        );
    }
}
