use std::{io::Error as IoError, mem};

use hftbacktest_derive::NpyDTyped;

use crate::{
    backtest::{
        BacktestError,
        data::{Data, DataPreprocess, DataSource, POD, Reader},
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
#[repr(C, align(32))]
#[derive(Clone, Debug, NpyDTyped)]
pub struct OrderLatencyRow {
    /// Timestamp at which the request occurs.
    pub req_ts: i64,
    /// Timestamp at which the exchange processes the request.
    pub exch_ts: i64,
    /// Timestamp at which the response is received.
    pub resp_ts: i64,
    /// For the alignment.
    pub _padding: i64,
}

unsafe impl POD for OrderLatencyRow {}

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
///     vec![DataSource::File("latency_20240215.npz".to_string())],
///     0
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
    /// Constructs an `IntpOrderLatency` with options.
    pub fn build(
        data: Vec<DataSource<OrderLatencyRow>>,
        parallel_load: bool,
        latency_offset: i64,
    ) -> Result<Self, BacktestError> {
        let mut reader = if latency_offset == 0 {
            Reader::builder()
                .parallel_load(parallel_load)
                .data(data)
                .build()?
        } else {
            Reader::builder()
                .parallel_load(parallel_load)
                .data(data)
                .preprocessor(OrderLatencyAdjustment::new(latency_offset))
                .build()?
        };
        let data = match reader.next_data() {
            Ok(data) => data,
            Err(BacktestError::EndOfData) => Data::empty(),
            Err(e) => return Err(e),
        };
        let next_data = match reader.next_data() {
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

    /// Constructs an `IntpOrderLatency` with default options.
    pub fn new(data: Vec<DataSource<OrderLatencyRow>>, latency_offset: i64) -> Self {
        Self::build(data, true, latency_offset).unwrap()
    }

    fn intp(&self, x: i64, x1: i64, y1: i64, x2: i64, y2: i64) -> i64 {
        (((y2 - y1) as f64) / ((x2 - x1) as f64) * ((x - x1) as f64)) as i64 + y1
    }

    fn next_data(&mut self) -> Result<bool, BacktestError> {
        if !self.next_data.is_empty() {
            let next_data = match self.reader.next_data() {
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
        if timestamp < first_row.req_ts {
            return first_row.exch_ts - first_row.req_ts;
        }

        loop {
            let row = &self.data[self.entry_rn];
            let next_row = if self.entry_rn + 1 < self.data.len() {
                &self.data[self.entry_rn + 1]
            } else if !self.next_data.is_empty() {
                &self.next_data[0]
            } else {
                let last_row = &self.data[self.data.len() - 1];
                return last_row.exch_ts - last_row.req_ts;
            };

            let req_local_timestamp = row.req_ts;
            let next_req_local_timestamp = next_row.req_ts;

            if row.req_ts <= timestamp && timestamp < next_row.req_ts {
                let exch_timestamp = row.exch_ts;
                let next_exch_timestamp = next_row.exch_ts;

                // The exchange may reject an order request due to technical issues such
                // congestion, this is particularly common in crypto markets. A timestamp of
                // zero on the exchange represents the occurrence of those kinds of errors at
                // that time.
                if exch_timestamp <= 0 || next_exch_timestamp <= 0 {
                    let resp_timestamp = row.resp_ts;
                    let next_resp_timestamp = next_row.resp_ts;
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
            } else if self.entry_rn == self.data.len() - 1 {
                if self.next_data().unwrap() {
                    self.entry_rn = 0;
                }
            } else {
                self.entry_rn += 1;
            }
        }
    }

    fn response(&mut self, timestamp: i64, _order: &Order) -> i64 {
        let first_row = &self.data[0];
        if timestamp < first_row.exch_ts {
            return first_row.resp_ts - first_row.exch_ts;
        }

        loop {
            let row = &self.data[self.resp_rn];
            let next_row = if self.resp_rn + 1 < self.data.len() {
                &self.data[self.resp_rn + 1]
            } else if !self.next_data.is_empty() {
                &self.next_data[0]
            } else {
                let last_row = &self.data[self.data.len() - 1];
                return last_row.resp_ts - last_row.exch_ts;
            };

            let exch_timestamp = row.exch_ts;
            let next_exch_timestamp = next_row.exch_ts;
            if exch_timestamp <= timestamp && timestamp < next_exch_timestamp {
                let resp_local_timestamp = row.resp_ts;
                let next_resp_local_timestamp = next_row.resp_ts;

                let lat1 = resp_local_timestamp - exch_timestamp;
                let lat2 = next_resp_local_timestamp - next_exch_timestamp;

                let lat = self.intp(timestamp, exch_timestamp, lat1, next_exch_timestamp, lat2);
                assert!(lat >= 0);
                return lat;
            } else if self.resp_rn == self.data.len() - 1 {
                if self.next_data().unwrap() {
                    self.resp_rn = 0;
                }
            } else {
                self.resp_rn += 1;
            }
        }
    }
}

#[derive(Clone)]
struct OrderLatencyAdjustment {
    latency_offset: i64,
}

impl OrderLatencyAdjustment {
    pub fn new(latency_offset: i64) -> Self {
        Self { latency_offset }
    }
}

impl DataPreprocess<OrderLatencyRow> for OrderLatencyAdjustment {
    fn preprocess(&self, data: &mut Data<OrderLatencyRow>) -> Result<(), IoError> {
        for i in 0..data.len() {
            data[i].exch_ts += self.latency_offset;
            data[i].resp_ts += self.latency_offset + self.latency_offset;
        }
        Ok(())
    }
}
