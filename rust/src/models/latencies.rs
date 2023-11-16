use crate::order::Order;
use crate::reader::Data;

pub trait LatencyModel {
    fn entry<Q: Clone>(&mut self, timestamp: i64, order: &Order<Q>) -> i64;
    fn response<Q: Clone>(&mut self, timestamp: i64, order: &Order<Q>) -> i64;
}

pub struct ConstantLatency {
    entry_latency: i64,
    response_latency: i64
}

impl ConstantLatency {
    pub fn new() -> Self {
        Self {
            entry_latency: 0,
            response_latency: 0
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

#[repr(C)]
#[derive(Clone, Debug)]
pub struct OrderLatencyRow {
    pub req_timestamp: i64,
    pub exch_timestamp: i64,
    pub resp_timestamp: i64,
}

pub struct IntpOrderLatency {
    entry_rn: usize,
    resp_rn: usize,
    data: Data<OrderLatencyRow>
}

impl IntpOrderLatency {
    pub fn new(data: Data<OrderLatencyRow>) -> Self {
        if data.len() == 0 {
            panic!();
        }
        Self {
            entry_rn: 0,
            resp_rn: 0,
            data
        }
    }

    fn intp(&self, x: i64, x1: i64, y1: i64, x2: i64, y2: i64) -> i64 {
        // todo: check div.
        (y2 - y1) / (x2 - x1) * (x - x1) + y1
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

        unsafe {
            for row_num in self.entry_rn..(self.data.len() - 1) {
                let req_local_timestamp = self.data[row_num].req_timestamp;
                let next_req_local_timestamp = self.data[row_num + 1].req_timestamp;
                if req_local_timestamp <= timestamp && timestamp < next_req_local_timestamp {
                    self.entry_rn = row_num;

                    let exch_timestamp = self.data[row_num].exch_timestamp;
                    let next_exch_timestamp = self.data[row_num + 1].exch_timestamp;

                    // The exchange may reject an order request due to technical issues such congestion, this is
                    // particularly common in crypto markets. A timestamp of zero on the exchange represents the occurrence
                    // of those kinds of errors at that time.
                    if exch_timestamp <= 0 || next_exch_timestamp <= 0 {
                        let resp_timestamp = self.data[row_num].resp_timestamp;
                        let next_resp_timestamp = self.data[row_num + 1].resp_timestamp;
                        let lat1 = resp_timestamp - req_local_timestamp;
                        let lat2 = next_resp_timestamp - next_req_local_timestamp;

                        // Negative latency indicates that the order is rejected for technical reasons, and its value
                        // represents the latency that the local experiences when receiving the rejection notification
                        return -self.intp(timestamp, req_local_timestamp, lat1, next_req_local_timestamp, lat2)
                    }

                    let lat1 = exch_timestamp - req_local_timestamp;
                    let lat2 = next_exch_timestamp - next_req_local_timestamp;
                    return self.intp(timestamp, req_local_timestamp, lat1, next_req_local_timestamp, lat2)
                }
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

        unsafe {
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
        }
        return -1;
    }
}