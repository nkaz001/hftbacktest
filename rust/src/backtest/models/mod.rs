mod latencies;
mod queue;

pub use latencies::{
    LatencyModel,
    ConstantLatency,
    IntpOrderLatency,
};
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
