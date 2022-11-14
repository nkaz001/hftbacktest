from numba import float64
from numba.experimental import jitclass

from .backtest import COL_LOCAL_TIMESTAMP, COL_EXCH_TIMESTAMP


@jitclass([
    ('entry_latency', float64),
    ('response_latency', float64)
])
class ConstantLatency:
    def __init__(self, entry_latency, response_latency):
        self.entry_latency = entry_latency
        self.response_latency = response_latency

    def entry(self, hbt):
        return self.entry_latency

    def response(self, hbt):
        return self.response_latency


@jitclass([
    ('entry_latency_mul', float64),
    ('resp_latency_mul', float64),
    ('entry_latency', float64),
    ('response_latency', float64),
])
class FeedLatency:
    def __init__(self, entry_latency_mul=1, resp_latency_mul=1, entry_latency=0, response_latency=0):
        self.entry_latency_mul = entry_latency_mul
        self.resp_latency_mul = resp_latency_mul
        self.entry_latency = entry_latency
        self.response_latency = response_latency

    def __latency(self, hbt):
        if hbt.row_num + 1 < len(hbt.data):
            next_local_timestamp = hbt.data[hbt.row_num + 1, COL_LOCAL_TIMESTAMP]
            next_exch_timestamp = hbt.data[hbt.row_num + 1, COL_EXCH_TIMESTAMP]
        else:
            next_local_timestamp = hbt.data[hbt.row_num, COL_LOCAL_TIMESTAMP]
            next_exch_timestamp = hbt.data[hbt.row_num, COL_EXCH_TIMESTAMP]
        local_timestamp = hbt.data[hbt.row_num, COL_LOCAL_TIMESTAMP]
        exch_timestamp = hbt.data[hbt.row_num, COL_EXCH_TIMESTAMP]
        lat1 = local_timestamp - exch_timestamp
        lat2 = next_local_timestamp - next_exch_timestamp
        return (lat1 + lat2) / 2.0

    def entry(self, hbt):
        return self.entry_latency + self.entry_latency_mul * self.__latency(hbt)

    def response(self, hbt):
        return self.response_latency + self.resp_latency_mul * self.__latency(hbt)
