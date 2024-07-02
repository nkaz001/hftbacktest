//! Latency and queue position models
//!
//! Please find more details in the documents below.
//! * [Latency Models](https://hftbacktest.readthedocs.io/en/latest/latency_models.html)
//! * [Order Fill](https://hftbacktest.readthedocs.io/en/latest/order_fill.html)
mod latencies;
mod queue;

pub use latencies::{ConstantLatency, IntpOrderLatency, OrderLatencyRow, LatencyModel};
#[cfg(any(feature = "unstable_l3", doc))]
pub use queue::{L3FIFOQueueModel, L3OrderId, L3OrderSource, L3QueueModel};
pub use queue::{
    LogProbQueueFunc,
    LogProbQueueFunc2,
    PowerProbQueueFunc,
    PowerProbQueueFunc2,
    PowerProbQueueFunc3,
    ProbQueueModel,
    QueueModel,
    QueuePos,
    RiskAdverseQueueModel,
};
