use std::{future::Future, sync::Arc};

use chrono::Utc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct Throttler {
    exec_ts: Arc<Mutex<Vec<i64>>>,
    rate_limit: usize,
}

impl Throttler {
    pub fn new(rate_limit: usize) -> Self {
        Self {
            exec_ts: Default::default(),
            rate_limit,
        }
    }

    pub async fn execute<Fut, T>(&mut self, fut: Fut) -> Option<T>
    where
        Fut: Future<Output = T>,
    {
        let cur_ts = Utc::now().timestamp_nanos_opt().unwrap();
        {
            let mut exec_ts_ = self.exec_ts.lock().await;
            exec_ts_.retain(|ts| *ts > cur_ts - 60_000_000_000);
            if exec_ts_.len() > self.rate_limit {
                return None;
            }
            exec_ts_.push(cur_ts);
        }
        Some(fut.await)
    }
}
