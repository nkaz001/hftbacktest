use std::marker::PhantomData;

use crate::{
    depth::MarketDepth,
    types::{Order, Side},
};

/// Provides an estimation of the order's queue position.
pub trait QueueModel<Q, MD>
where
    Q: Clone,
    MD: MarketDepth,
{
    /// Initialize the queue position and other necessary values for estimation.
    /// This function is called when the exchange model accepts the new order.
    fn new_order(&self, order: &mut Order<Q>, depth: &MD);

    /// Adjusts the estimation values when market trades occur at the same price.
    fn trade(&self, order: &mut Order<Q>, qty: f32, depth: &MD);

    /// Adjusts the estimation values when market depth changes at the same price.
    fn depth(&self, order: &mut Order<Q>, prev_qty: f32, new_qty: f32, depth: &MD);

    // todo: remove it, as there's no need for specialization.
    fn is_filled(&self, order: &Order<Q>, depth: &MD) -> bool;
}

/// Provides a conservative queue position model, where your order's queue position advances only
/// when trades occur at the same price level.
pub struct RiskAdverseQueueModel<MD>(PhantomData<MD>);

impl<MD> RiskAdverseQueueModel<MD> {
    pub fn new() -> Self {
        Self(Default::default())
    }
}

impl<MD> QueueModel<(), MD> for RiskAdverseQueueModel<MD>
where
    MD: MarketDepth,
{
    fn new_order(&self, order: &mut Order<()>, depth: &MD) {
        if order.side == Side::Buy {
            order.front_q_qty = depth.bid_qty_at_tick(order.price_tick);
        } else {
            order.front_q_qty = depth.ask_qty_at_tick(order.price_tick);
        }
    }

    fn trade(&self, order: &mut Order<()>, qty: f32, _depth: &MD) {
        order.front_q_qty -= qty;
    }

    fn depth(&self, order: &mut Order<()>, _prev_qty: f32, new_qty: f32, _depth: &MD) {
        order.front_q_qty = order.front_q_qty.min(new_qty);
    }

    fn is_filled(&self, order: &Order<()>, depth: &MD) -> bool {
        (order.front_q_qty / depth.lot_size()).round() < 0.0
    }
}

/// Stores the values needed for queue position estimation and adjustment for [`ProbQueueModel`].
#[derive(Clone)]
pub struct QueuePos {
    cum_trade_qty: f32,
}

impl Default for QueuePos {
    fn default() -> Self {
        Self { cum_trade_qty: 0.0 }
    }
}

/// Provides the probability of a decrease behind the order's queue position.
pub trait Probability {
    /// Returns the probability based on the quantity ahead and behind the order.
    fn prob(&self, front: f32, back: f32) -> f32;
}

/// Provides a probability-based queue position model as described in
/// https://quant.stackexchange.com/questions/3782/how-do-we-estimate-position-of-our-order-in-order-book
/// https://rigtorp.se/2013/06/08/estimating-order-queue-position.html
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
    /// Constructs [`ProbQueueModel`] with a [`Probability`] model.
    pub fn new(prob: P) -> Self {
        Self {
            prob,
            _md_marker: Default::default(),
        }
    }
}

impl<P, MD> QueueModel<QueuePos, MD> for ProbQueueModel<P, MD>
where
    P: Probability,
    MD: MarketDepth,
{
    fn new_order(&self, order: &mut Order<QueuePos>, depth: &MD) {
        if order.side == Side::Buy {
            order.front_q_qty = depth.bid_qty_at_tick(order.price_tick);
        } else {
            order.front_q_qty = depth.ask_qty_at_tick(order.price_tick);
        }
    }

    fn trade(&self, order: &mut Order<QueuePos>, qty: f32, _depth: &MD) {
        order.front_q_qty -= qty;
        order.q.cum_trade_qty += qty;
    }

    fn depth(&self, order: &mut Order<QueuePos>, prev_qty: f32, new_qty: f32, _depth: &MD) {
        let mut chg = prev_qty - new_qty;
        // In order to avoid duplicate order queue position adjustment, subtract queue position
        // change by trades.
        chg = chg - order.q.cum_trade_qty;
        // Reset, as quantity change by trade should be already reflected in qty.
        order.q.cum_trade_qty = 0.0;
        // For an increase of the quantity, front queue doesn't change by the quantity change.
        if chg < 0.0 {
            order.front_q_qty = order.front_q_qty.min(new_qty);
            return;
        }

        let front = order.front_q_qty;
        let back = prev_qty - front;

        let mut prob = self.prob.prob(front, back);
        if prob.is_infinite() {
            prob = 1.0;
        }

        let est_front = front - (1.0 - prob) * chg + (back - prob * chg).min(0.0);
        order.front_q_qty = est_front.min(new_qty);
    }

    fn is_filled(&self, order: &Order<QueuePos>, depth: &MD) -> bool {
        (order.front_q_qty / depth.lot_size()).round() < 0.0
    }
}

/// This probability model uses a power function `f(x) = x ** n` to adjust the probability which is
/// calculated as `f(back) / (f(back) + f(front))`.
pub struct PowerProbQueueFunc {
    n: f32,
}

impl PowerProbQueueFunc {
    /// Constructs [`PowerProbQueueFunc`].
    pub fn new(n: f32) -> Self {
        Self { n }
    }

    fn f(&self, x: f32) -> f32 {
        x.powf(self.n)
    }
}

impl Probability for PowerProbQueueFunc {
    fn prob(&self, front: f32, back: f32) -> f32 {
        self.f(back) / (self.f(back) + self.f(front))
    }
}

/// This probability model uses a logarithmic function `f(x) = log(1 + x)` to adjust the
/// probability which is calculated as `f(back) / (f(back) + f(front))`.
pub struct LogProbQueueFunc(());

impl LogProbQueueFunc {
    /// Constructs [`LogProbQueueFunc`].
    pub fn new() -> Self {
        Self(())
    }

    fn f(&self, x: f32) -> f32 {
        (1.0 + x).ln()
    }
}

impl Probability for LogProbQueueFunc {
    fn prob(&self, front: f32, back: f32) -> f32 {
        self.f(back) / (self.f(back) + self.f(front))
    }
}

/// This probability model uses a logarithmic function `f(x) = log(1 + x)` to adjust the
/// probability which is calculated as `f(back) / f(back + front)`.
pub struct LogProbQueueFunc2(());

impl LogProbQueueFunc2 {
    /// Constructs [`LogProbQueueFunc2`].
    pub fn new() -> Self {
        Self(())
    }

    fn f(&self, x: f32) -> f32 {
        (1.0 + x).ln()
    }
}

impl Probability for LogProbQueueFunc2 {
    fn prob(&self, front: f32, back: f32) -> f32 {
        self.f(back) / self.f(back + front)
    }
}

/// This probability model uses a power function `f(x) = x ** n` to adjust the probability which is
/// calculated as `f(back) / f(back + front)`.
pub struct PowerProbQueueFunc2 {
    n: f32,
}

impl PowerProbQueueFunc2 {
    /// Constructs [`PowerProbQueueFunc2`].
    pub fn new(n: f32) -> Self {
        Self { n }
    }

    fn f(&self, x: f32) -> f32 {
        x.powf(self.n)
    }
}

impl Probability for PowerProbQueueFunc2 {
    fn prob(&self, front: f32, back: f32) -> f32 {
        self.f(back) / self.f(back + front)
    }
}

/// This probability model uses a power function `f(x) = x ** n` to adjust the probability which is
/// calculated as `1 - f(front / (front + back))`.
pub struct PowerProbQueueFunc3 {
    n: f32,
}

impl PowerProbQueueFunc3 {
    /// Constructs [`PowerProbQueueFunc3`].
    pub fn new(n: f32) -> Self {
        Self { n }
    }

    fn f(&self, x: f32) -> f32 {
        x.powf(self.n)
    }
}

impl Probability for PowerProbQueueFunc3 {
    fn prob(&self, front: f32, back: f32) -> f32 {
        1.0 - self.f(front / (front + back))
    }
}
