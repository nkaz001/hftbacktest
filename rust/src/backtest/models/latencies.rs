use std::mem;

use crate::{
    backtest::{
        reader::{Cache, Data, DataSource, NpyFile, Reader, POD},
        BacktestError,
    },
    types::Order,
};

/// Provides the order entry latency and the order response latency.
pub trait LatencyModel {
    /// Returns the order entry latency for the given timestamp and order.
    fn entry(&mut self, timestamp: i64, order: &Order) -> i64;

    /// Returns the order response latency for the given timestamp and order.
    fn response(&mut self, timestamp: i64, order: &Order) -> i64;
}

/// Provides constant order latency.
///
/// If latency has a negative value, it indicates an order rejection by the exchange and its
/// value represents the latency that the local experiences when receiving the rejection
/// notification.
#[derive(Clone)]
pub struct ConstantLatency {
    entry_latency: i64,
    response_latency: i64,
}

impl ConstantLatency {
    /// Constructs an instance of `ConstantLatency`.
    ///
    /// `entry_latency` and `response_latency` should match the time unit of the data's timestamps.
    /// Using nanoseconds across all datasets is recommended, since the live
    /// [Bot](crate::live::LiveBot) uses nanoseconds.
    pub fn new(entry_latency: i64, response_latency: i64) -> Self {
        Self {
            entry_latency,
            response_latency,
        }
    }
}

impl LatencyModel for ConstantLatency {
    fn entry(&mut self, _timestamp: i64, _order: &Order) -> i64 {
        self.entry_latency
    }

    fn response(&mut self, _timestamp: i64, _order: &Order) -> i64 {
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

unsafe impl POD for OrderLatencyRow {}

unsafe impl NpyFile for OrderLatencyRow {}

/// Provides order latency based on actual historical order latency data through interpolation.
///
/// However, if you don't have the actual order latency history, you can generate order latencies
/// artificially based on feed latency or using a custom model such as a regression model, which
/// incorporates factors like feed latency, trading volume, and the number of events.
///
/// In historical order latency data, negative latencies should not exist. This means that there
/// should be no instances where `exch_timestamp - req_timestamp < 0` or
/// `resp_timestamp - exch_timestamp < 0`. However, it's worth noting that exchanges may
/// inadequately handle or reject orders during overload situations or for technical reasons,
/// resulting in exchange timestamps being zero. In such cases, [entry()](Self::entry()) or
/// [response()](Self::response()) returns negative latency, indicating an order rejection by the
/// exchange, and its value represents the latency that the local experiences when receiving the
/// rejection notification.
///
/// **Example**
/// ```
/// use hftbacktest::backtest::{DataSource, models::IntpOrderLatency};
///
/// let latency_model = IntpOrderLatency::new(
///     vec![DataSource::File("latency_20240215.npz".to_string())]
/// );
/// ```
#[derive(Clone)]
pub struct IntpOrderLatency {
    entry_rn: usize,
    resp_rn: usize,
    reader: Reader<OrderLatencyRow>,
    data: Data<OrderLatencyRow>,
    next_data: Data<OrderLatencyRow>,
}

impl IntpOrderLatency {
    /// Constructs an instance of `IntpOrderLatency`.
    pub fn new(data: Vec<DataSource<OrderLatencyRow>>) -> Result<Self, BacktestError> {
        let mut reader = Reader::new(Cache::new());
        for file in data {
            match file {
                DataSource::File(file) => {
                    reader.add_file(file);
                }
                DataSource::Data(data) => {
                    reader.add_data(data);
                }
            }
        }
        let data = match reader.next() {
            Ok(data) => data,
            Err(BacktestError::EndOfData) => Data::empty(),
            Err(e) => return Err(e),
        };
        let next_data = match reader.next() {
            Ok(data) => data,
            Err(BacktestError::EndOfData) => Data::empty(),
            Err(e) => return Err(e),
        };
        Ok(Self {
            entry_rn: 0,
            resp_rn: 0,
            reader,
            data,
            next_data,
        })
    }

    fn intp(&self, x: i64, x1: i64, y1: i64, x2: i64, y2: i64) -> i64 {
        (((y2 - y1) as f64) / ((x2 - x1) as f64) * ((x - x1) as f64)) as i64 + y1
    }

    fn next(&mut self) -> Result<bool, BacktestError> {
        if self.next_data.len() > 0 {
            let next_data = match self.reader.next() {
                Ok(data) => data,
                Err(BacktestError::EndOfData) => Data::empty(),
                Err(e) => return Err(e),
            };
            let next_data = mem::replace(&mut self.next_data, next_data);
            let data = mem::replace(&mut self.data, next_data);
            self.reader.release(data);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl LatencyModel for IntpOrderLatency {
    fn entry(&mut self, timestamp: i64, _order: &Order) -> i64 {
        let first_row = &self.data[0];
        if timestamp < first_row.req_timestamp {
            return first_row.exch_timestamp - first_row.req_timestamp;
        }

        loop {
            let row = &self.data[self.entry_rn];
            let next_row = if self.entry_rn + 1 < self.data.len() {
                &self.data[self.entry_rn + 1]
            } else if self.next_data.len() > 0 {
                &self.next_data[0]
            } else {
                let last_row = &self.data[self.data.len() - 1];
                return last_row.exch_timestamp - last_row.req_timestamp;
            };

            let req_local_timestamp = row.req_timestamp;
            let next_req_local_timestamp = next_row.req_timestamp;

            if row.req_timestamp <= timestamp && timestamp < next_row.req_timestamp {
                let exch_timestamp = row.exch_timestamp;
                let next_exch_timestamp = next_row.exch_timestamp;

                // The exchange may reject an order request due to technical issues such
                // congestion, this is particularly common in crypto markets. A timestamp of
                // zero on the exchange represents the occurrence of those kinds of errors at
                // that time.
                if exch_timestamp <= 0 || next_exch_timestamp <= 0 {
                    let resp_timestamp = row.resp_timestamp;
                    let next_resp_timestamp = next_row.resp_timestamp;
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
            } else {
                if self.entry_rn == self.data.len() - 1 {
                    if self.next().unwrap() {
                        self.entry_rn = 0;
                    }
                } else {
                    self.entry_rn += 1;
                }
            }
        }
    }

    fn response(&mut self, timestamp: i64, _order: &Order) -> i64 {
        let first_row = &self.data[0];
        if timestamp < first_row.exch_timestamp {
            return first_row.resp_timestamp - first_row.exch_timestamp;
        }

        loop {
            let row = &self.data[self.resp_rn];
            let next_row = if self.resp_rn + 1 < self.data.len() {
                &self.data[self.resp_rn + 1]
            } else if self.next_data.len() > 0 {
                &self.next_data[0]
            } else {
                let last_row = &self.data[self.data.len() - 1];
                return last_row.resp_timestamp - last_row.exch_timestamp;
            };

            let exch_timestamp = row.exch_timestamp;
            let next_exch_timestamp = next_row.exch_timestamp;
            if exch_timestamp <= timestamp && timestamp < next_exch_timestamp {
                let resp_local_timestamp = row.resp_timestamp;
                let next_resp_local_timestamp = next_row.resp_timestamp;

                let lat1 = resp_local_timestamp - exch_timestamp;
                let lat2 = next_resp_local_timestamp - next_exch_timestamp;

                let lat = self.intp(timestamp, exch_timestamp, lat1, next_exch_timestamp, lat2);
                assert!(lat >= 0);
                return lat;
            } else {
                if self.resp_rn == self.data.len() - 1 {
                    if self.next().unwrap() {
                        self.resp_rn = 0;
                    }
                } else {
                    self.resp_rn += 1;
                }
            }
        }
    }
}
