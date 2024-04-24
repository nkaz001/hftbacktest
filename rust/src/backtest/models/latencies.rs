use crate::{backtest::reader::Data, types::Order};

/// Provides the order entry latency and the order response latency.
pub trait LatencyModel {
    /// Returns the order entry latency for the given timestamp and order.
    fn entry<Q: Clone>(&mut self, timestamp: i64, order: &Order<Q>) -> i64;

    /// Returns the order response latency for the given timestamp and order.
    fn response<Q: Clone>(&mut self, timestamp: i64, order: &Order<Q>) -> i64;
}

/// Provides constant order latency.
#[derive(Clone)]
pub struct ConstantLatency {
    entry_latency: i64,
    response_latency: i64,
}

impl ConstantLatency {
    /// Constructs [`ConstantLatency`].
    ///
    /// `entry_latency` and `response_latency` should match the time unit of the data's timestamps.
    /// Using nanoseconds across all datasets is recommended, since the live [`crate::live::Bot`]
    /// uses nanoseconds.
    ///
    /// If latency has a negative value, it indicates an order rejection by the exchange and its
    /// value represents the latency that the local experiences when receiving the rejection
    /// notification.
    pub fn new(entry_latency: i64, response_latency: i64) -> Self {
        Self {
            entry_latency,
            response_latency,
        }
    }
}

impl LatencyModel for ConstantLatency {
    fn entry<Q: Clone>(&mut self, _timestamp: i64, _order: &Order<Q>) -> i64 {
        self.entry_latency
    }

    fn response<Q: Clone>(&mut self, _timestamp: i64, _order: &Order<Q>) -> i64 {
        self.response_latency
    }
}

/// The historical order latency data
#[derive(Clone, Debug)]
#[repr(C, align(32))]
pub struct OrderLatencyRow {
    /// Timestamp at which the request occurs.
    pub req_timestamp: i64,
    /// Timestamp at which the exchange processes the request.
    pub exch_timestamp: i64,
    /// Timestamp at which the response is received.
    pub resp_timestamp: i64,
    /// For the alignment.
    pub _reserved: i64,
}

/// Provides order latency based on actual historical order latency data through interpolation.
///
/// However, if you don't actual order latency history, you can generate order latencies
/// artificially based on feed latency or using a custom model such as a regression model, which
/// incorporates factors like feed latency, trading volume, and the number of events.
#[derive(Clone)]
pub struct IntpOrderLatency {
    entry_rn: usize,
    resp_rn: usize,
    data: Data<OrderLatencyRow>,
}

impl IntpOrderLatency {
    /// Constructs [`IntpOrderLatency`].
    ///
    /// In historical order latency data, negative latencies should not exist. This means that there
    /// should be no instances where `exch_timestamp - req_timestamp < 0` or
    /// `resp_timestamp - exch_timestamp < 0`. However, it's worth noting that exchanges may
    /// inadequately handle or reject orders during overload situations or for technical reasons,
    /// resulting in exchange timestamps being zero. In such cases, [`Self::entry()`] or
    /// [`Self::response()`] returns negative latency, indicating an order rejection by the
    /// exchange, and its value represents the latency that the local experiences when receiving the
    /// rejection notification.
    ///
    /// ```
    /// use hftbacktest::backtest::{reader::read_npz, models::IntpOrderLatency};
    ///
    /// let latency_model = IntpOrderLatency::new(
    ///     read_npz("latency_20240215.npz").unwrap()
    /// );
    /// ```
    pub fn new(data: Data<OrderLatencyRow>) -> Self {
        if data.len() == 0 {
            panic!();
        }
        Self {
            entry_rn: 0,
            resp_rn: 0,
            data,
        }
    }

    fn intp(&self, x: i64, x1: i64, y1: i64, x2: i64, y2: i64) -> i64 {
        (((y2 - y1) as f64) / ((x2 - x1) as f64) * ((x - x1) as f64)) as i64 + y1
    }
}

impl LatencyModel for IntpOrderLatency {
    fn entry<Q: Clone>(&mut self, timestamp: i64, _order: &Order<Q>) -> i64 {
        let first_row = &self.data[0];
        if timestamp < first_row.req_timestamp {
            return first_row.exch_timestamp - first_row.req_timestamp;
        }

        let last_row = &self.data[self.data.len() - 1];
        if timestamp >= last_row.req_timestamp {
            return last_row.exch_timestamp - last_row.req_timestamp;
        }

        for row_num in self.entry_rn..(self.data.len() - 1) {
            let req_local_timestamp = self.data[row_num].req_timestamp;
            let next_req_local_timestamp = self.data[row_num + 1].req_timestamp;
            if req_local_timestamp <= timestamp && timestamp < next_req_local_timestamp {
                self.entry_rn = row_num;

                let exch_timestamp = self.data[row_num].exch_timestamp;
                let next_exch_timestamp = self.data[row_num + 1].exch_timestamp;

                // The exchange may reject an order request due to technical issues such
                // congestion, this is particularly common in crypto markets. A timestamp of
                // zero on the exchange represents the occurrence of those kinds of errors at
                // that time.
                if exch_timestamp <= 0 || next_exch_timestamp <= 0 {
                    let resp_timestamp = self.data[row_num].resp_timestamp;
                    let next_resp_timestamp = self.data[row_num + 1].resp_timestamp;
                    let lat1 = resp_timestamp - req_local_timestamp;
                    let lat2 = next_resp_timestamp - next_req_local_timestamp;

                    // Negative latency indicates that the order is rejected for technical
                    // reasons, and its value represents the latency that the local experiences
                    // when receiving the rejection notification
                    return -self.intp(
                        timestamp,
                        req_local_timestamp,
                        lat1,
                        next_req_local_timestamp,
                        lat2,
                    );
                }

                let lat1 = exch_timestamp - req_local_timestamp;
                let lat2 = next_exch_timestamp - next_req_local_timestamp;
                return self.intp(
                    timestamp,
                    req_local_timestamp,
                    lat1,
                    next_req_local_timestamp,
                    lat2,
                );
            }
        }
        return -1;
    }

    fn response<Q: Clone>(&mut self, timestamp: i64, _order: &Order<Q>) -> i64 {
        let first_row = &self.data[0];
        if timestamp < first_row.exch_timestamp {
            return first_row.resp_timestamp - first_row.exch_timestamp;
        }

        let last_row = &self.data[self.data.len() - 1];
        if timestamp >= last_row.exch_timestamp {
            return last_row.resp_timestamp - last_row.exch_timestamp;
        }

        for row_num in self.resp_rn..(self.data.len() - 1) {
            let exch_timestamp = self.data[row_num].exch_timestamp;
            let next_exch_timestamp = self.data[row_num + 1].exch_timestamp;
            if exch_timestamp <= timestamp && timestamp < next_exch_timestamp {
                self.resp_rn = row_num;

                let resp_local_timestamp = self.data[row_num].resp_timestamp;
                let next_resp_local_timestamp = self.data[row_num + 1].resp_timestamp;

                let lat1 = resp_local_timestamp - exch_timestamp;
                let lat2 = next_resp_local_timestamp - next_exch_timestamp;

                let lat = self.intp(timestamp, exch_timestamp, lat1, next_exch_timestamp, lat2);
                if lat < 0 {
                    return -1;
                }
                return lat;
            }
        }
        return -1;
    }
}
