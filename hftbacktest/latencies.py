from numba import float64, int64
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

    def entry(self, order, hbt):
        return self.entry_latency

    def response(self, order, hbt):
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
        local_timestamp = -1
        exch_timestamp = -1
        for row_num in range(hbt.row_num, -1, -1):
            local_timestamp = hbt.data[row_num + 1, COL_LOCAL_TIMESTAMP]
            exch_timestamp = hbt.data[row_num + 1, COL_EXCH_TIMESTAMP]
            if local_timestamp != -1 and exch_timestamp != -1:
                break

        next_local_timestamp = -1
        next_exch_timestamp = -1
        for row_num in range(hbt.row_num + 1, len(hbt.data)):
            next_local_timestamp = hbt.data[row_num + 1, COL_LOCAL_TIMESTAMP]
            next_exch_timestamp = hbt.data[row_num + 1, COL_EXCH_TIMESTAMP]
            if next_local_timestamp != -1 and next_exch_timestamp != -1:
                break

        lat1 = -1
        lat2 = -1
        if local_timestamp != -1 and exch_timestamp != -1:
            lat1 = local_timestamp - exch_timestamp
        if next_local_timestamp != -1 and next_exch_timestamp != -1:
            lat2 = next_local_timestamp - next_exch_timestamp

        if lat1 != -1 and lat2 != -1:
            return (lat1 + lat2) / 2.0
        elif lat1 != -1:
            return lat1
        elif lat2 != -1:
            return lat2
        else:
            raise ValueError

    def entry(self, order, hbt):
        return self.entry_latency + self.entry_latency_mul * self.__latency(hbt)

    def response(self, order, hbt):
        return self.response_latency + self.resp_latency_mul * self.__latency(hbt)


@jitclass([
    ('entry_latency_mul', float64),
    ('resp_latency_mul', float64),
    ('entry_latency', float64),
    ('response_latency', float64),
])
class ForwardFeedLatency:
    def __init__(self, entry_latency_mul=1, resp_latency_mul=1, entry_latency=0, response_latency=0):
        self.entry_latency_mul = entry_latency_mul
        self.resp_latency_mul = resp_latency_mul
        self.entry_latency = entry_latency
        self.response_latency = response_latency

    def __latency(self, hbt):
        for row_num in range(hbt.row_num + 1, len(hbt.data)):
            next_local_timestamp = hbt.data[row_num + 1, COL_LOCAL_TIMESTAMP]
            next_exch_timestamp = hbt.data[row_num + 1, COL_EXCH_TIMESTAMP]
            if next_local_timestamp != -1 and next_exch_timestamp != -1:
                return next_local_timestamp - next_exch_timestamp
        return ValueError

    def entry(self, order, hbt):
        return self.entry_latency + self.entry_latency_mul * self.__latency(hbt)

    def response(self, order, hbt):
        return self.response_latency + self.resp_latency_mul * self.__latency(hbt)


@jitclass([
    ('entry_latency_mul', float64),
    ('resp_latency_mul', float64),
    ('entry_latency', float64),
    ('response_latency', float64),
])
class BackwardFeedLatency:
    def __init__(self, entry_latency_mul=1, resp_latency_mul=1, entry_latency=0, response_latency=0):
        self.entry_latency_mul = entry_latency_mul
        self.resp_latency_mul = resp_latency_mul
        self.entry_latency = entry_latency
        self.response_latency = response_latency

    def __latency(self, hbt):
        for row_num in range(hbt.row_num, -1, -1):
            local_timestamp = hbt.data[row_num + 1, COL_LOCAL_TIMESTAMP]
            exch_timestamp = hbt.data[row_num + 1, COL_EXCH_TIMESTAMP]
            if local_timestamp != -1 and exch_timestamp != -1:
                return local_timestamp - exch_timestamp
            return ValueError

    def entry(self, order, hbt):
        return self.entry_latency + self.entry_latency_mul * self.__latency(hbt)

    def response(self, order, hbt):
        return self.response_latency + self.resp_latency_mul * self.__latency(hbt)


@jitclass([
    ('entry_rn', int64),
    ('resp_rn', int64),
    ('data', float64[:]),
])
class IntpOrderLatency:
    def __init__(self, data):
        self.entry_rn = 0
        self.resp_rn = 0
        # req_local_timestamp: local timestamp at requesting (submit, cancel)
        # exch_timestamp: exchange timestamp in the order response
        # resp_local_timestamp: local timestamp at receiving the response.
        #
        # data is numpy array (n x 3)
        # req_local_timestamp, exch_timestamp, resp_local_timestamp
        # ..
        # ..
        self.data = data

    def __intp(self, x, x1, y1, x2, y2):
        return (y2 - y1) / (x2 - x1) * (x - x1) + y1

    def entry(self, order, hbt):
        if hbt.local_timestamp < self.data[0, 0]:
            return self.data[0, 1] - self.data[0, 0]
        if hbt.local_timestamp >= self.data[-1, 0]:
            return self.data[-1, 1] - self.data[-1, 0]
        for row_num in range(self.entry_rn, len(self.data) - 1):
            req_local_timestamp = self.data[row_num, 0]
            next_req_local_timestamp = self.data[row_num + 1, 0]
            if req_local_timestamp <= hbt.local_timestamp < next_req_local_timestamp:
                self.entry_rn = row_num

                exch_timestamp = self.data[row_num, 1]
                next_exch_timestamp = self.data[row_num + 1, 1]

                lat1 = exch_timestamp - req_local_timestamp
                lat2 = next_exch_timestamp - next_req_local_timestamp
                return self.__intp(hbt.local_timestamp, req_local_timestamp, lat1, next_req_local_timestamp, lat2)
        raise ValueError

    def response(self, order, hbt):
        if order.exch_timestamp < self.data[0, 1]:
            return self.data[0, 2] - self.data[0, 1]
        if order.exch_timestamp >= self.data[-1, 1]:
            return self.data[-1, 2] - self.data[-1, 1]
        for row_num in range(self.resp_rn, len(self.data) - 1):
            exch_timestamp = self.data[row_num, 1]
            next_exch_timestamp = self.data[row_num + 1, 1]
            if exch_timestamp <= order.exch_timestamp < next_exch_timestamp:
                self.resp_rn = row_num

                resp_local_timestamp = self.data[row_num, 2]
                next_resp_local_timestamp = self.data[row_num + 1, 2]

                lat1 = resp_local_timestamp - exch_timestamp
                lat2 = next_resp_local_timestamp - next_exch_timestamp
                return self.__intp(order.exch_timestamp, exch_timestamp, lat1, next_exch_timestamp, lat2)
        raise ValueError
