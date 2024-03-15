use crate::{
    depth::hashmapmarketdepth::HashMapMarketDepth,
    ty::{Order, Side},
};

pub trait QueueModel<Q>
where
    Q: Clone,
{
    fn new_order(&self, order: &mut Order<Q>, depth: &HashMapMarketDepth);
    fn trade(&self, order: &mut Order<Q>, qty: f32, depth: &HashMapMarketDepth);
    fn depth(&self, order: &mut Order<Q>, prev_qty: f32, new_qty: f32, depth: &HashMapMarketDepth);
    fn is_filled(&self, order: &Order<Q>, depth: &HashMapMarketDepth) -> bool;
}

pub struct RiskAdverseQueueModel(());

impl RiskAdverseQueueModel {
    pub fn new() -> Self {
        Self(())
    }
}

impl QueueModel<f32> for RiskAdverseQueueModel {
    fn new_order(&self, order: &mut Order<f32>, depth: &HashMapMarketDepth) {
        if order.side == Side::Buy {
            order.q = *depth.bid_depth.get(&order.price_tick).unwrap_or(&0.0);
        } else {
            order.q = *depth.ask_depth.get(&order.price_tick).unwrap_or(&0.0);
        }
    }

    fn trade(&self, order: &mut Order<f32>, qty: f32, _depth: &HashMapMarketDepth) {
        order.q -= qty;
    }

    fn depth(
        &self,
        order: &mut Order<f32>,
        _prev_qty: f32,
        new_qty: f32,
        _depth: &HashMapMarketDepth,
    ) {
        order.q = order.q.min(new_qty);
    }

    fn is_filled(&self, order: &Order<f32>, depth: &HashMapMarketDepth) -> bool {
        (order.q / depth.lot_size).round() < 0.0
    }
}

#[derive(Clone)]
pub struct QueuePos {
    front: f32,
    cum_trade_qty: f32,
}

impl Default for QueuePos {
    fn default() -> Self {
        Self {
            front: 0.0,
            cum_trade_qty: 0.0,
        }
    }
}

pub trait Probability {
    fn prob(&self, front: f32, back: f32) -> f32;
}

pub struct ProbQueueModel<P>
where
    P: Probability,
{
    prob: P,
}

impl<P> ProbQueueModel<P>
where
    P: Probability,
{
    pub fn new(prob: P) -> Self {
        Self { prob }
    }
}

/// Provides a probability-based queue position model as described in
/// https://quant.stackexchange.com/questions/3782/how-do-we-estimate-position-of-our-order-in-order-book.
///
/// Your order's queue position advances when a trade occurs at the same price level or the quantity at the level
/// decreases. The advancement in queue position depends on the probability based on the relative queue position. To
/// avoid double counting the quantity decrease caused by trades, all trade quantities occurring at the level before
/// the book quantity changes will be subtracted from the book quantity changes.
impl<P> QueueModel<QueuePos> for ProbQueueModel<P>
where
    P: Probability,
{
    fn new_order(&self, order: &mut Order<QueuePos>, depth: &HashMapMarketDepth) {
        if order.side == Side::Buy {
            order.q.front = *depth.bid_depth.get(&order.price_tick).unwrap_or(&0.0);
        } else {
            order.q.front = *depth.ask_depth.get(&order.price_tick).unwrap_or(&0.0);
        }
    }

    fn trade(&self, order: &mut Order<QueuePos>, qty: f32, _depth: &HashMapMarketDepth) {
        order.q.front -= qty;
        order.q.cum_trade_qty += qty;
    }

    fn depth(
        &self,
        order: &mut Order<QueuePos>,
        prev_qty: f32,
        new_qty: f32,
        _depth: &HashMapMarketDepth,
    ) {
        let mut chg = prev_qty - new_qty;
        // In order to avoid duplicate order queue position adjustment, subtract queue position
        // change by trades.
        chg = chg - order.q.cum_trade_qty;
        // Reset, as quantity change by trade should be already reflected in qty.
        order.q.cum_trade_qty = 0.0;
        // For an increase of the quantity, front queue doesn't change by the quantity change.
        if chg < 0.0 {
            order.q.front = order.q.front.min(new_qty);
            return;
        }

        let front = order.q.front;
        let back = prev_qty - front;

        let mut prob = self.prob.prob(front, back);
        if prob.is_infinite() {
            prob = 1.0;
        }

        let est_front = front - (1.0 - prob) * chg + (back - prob * chg).min(0.0);
        order.q.front = est_front.min(new_qty);
    }

    fn is_filled(&self, order: &Order<QueuePos>, depth: &HashMapMarketDepth) -> bool {
        (order.q.front / depth.lot_size).round() < 0.0
    }
}

pub struct PowerProbQueueFunc {
    n: f32,
}

impl PowerProbQueueFunc {
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

pub struct LogProbQueueFunc(());

impl LogProbQueueFunc {
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

pub struct LogProbQueueFunc2(());

impl LogProbQueueFunc2 {
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

pub struct PowerProbQueueFunc2 {
    n: f32,
}

impl PowerProbQueueFunc2 {
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

pub struct PowerProbQueueFunc3 {
    n: f32,
}

impl PowerProbQueueFunc3 {
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
