//! Latency and queue position models
//!
//! Please find more details in the documents below.
//! * [Latency Models](https://hftbacktest.readthedocs.io/en/latest/latency_models.html)
//! * [Order Fill](https://hftbacktest.readthedocs.io/en/latest/order_fill.html)
mod fee;
mod latency;
mod queue;

pub use fee::{
    CommonFees,
    DirectionalFees,
    FeeModel,
    FlatPerTradeFeeModel,
    TradingQtyFeeModel,
    TradingValueFeeModel,
};
pub use latency::{ConstantLatency, IntpOrderLatency, LatencyModel, OrderLatencyRow};
#[cfg(any(feature = "unstable_l3", doc))]
pub use queue::{L3FIFOQueueModel, L3OrderId, L3OrderSource, L3QueueModel};
pub use queue::{
    LogProbQueueFunc,
    LogProbQueueFunc2,
    PowerProbQueueFunc,
    PowerProbQueueFunc2,
    PowerProbQueueFunc3,
    ProbQueueModel,
    Probability,
    QueueModel,
    QueuePos,
    RiskAdverseQueueModel,
};
