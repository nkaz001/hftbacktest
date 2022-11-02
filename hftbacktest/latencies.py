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
    ('latency_coeff', float64),
])
class FeedLatency:
    def __init__(self, latency_coeff=1):
        self.latency_coeff = latency_coeff

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
        return self.latency_coeff * (lat1 + lat2) / 2.0

    def entry(self, hbt):
        return self.__latency(hbt)

    def response(self, hbt):
        return self.__latency(hbt)
