from numba import float64, int64

from ..reader import COL_LOCAL_TIMESTAMP, COL_EXCH_TIMESTAMP


class ConstantLatency:
    r"""
    Provides constant order latency. The units of the arguments should match the timestamp units of your
    data.

    Args:
        entry_latency: Order entry latency.
        response_latency: Order response latency.
    """

    entry_latency: float64
    response_latency: float64

    def __init__(self, entry_latency, response_latency):
        self.entry_latency = entry_latency
        self.response_latency = response_latency

    def entry(self, timestamp, order, proc):
        return self.entry_latency

    def response(self, timestamp, order, proc):
        return self.response_latency

    def reset(self):
        pass


class FeedLatency:
    r"""
    Provides order latency based on feed latency. The units of the arguments should match the timestamp units of your
    data.

    Order latency is computed as follows:

    * feed_latency is calculated as the average latency between the latest feed's latency and the subsequent feed's
      latency(by forward-looking).

    * If either of these values is unavailable, the available value is used as the sole feed latency.

    .. code-block::

        entry_latency = feed_latency * entry_latency_mul + entry_latency
        response_latency = feed_latency * resp_latency_mul + response_latency

    Args:
        entry_latency_mul: Multiplier for feed latency to compute order entry latency.
        resp_latency_mul: Multiplier for feed latency to compute order response latency.
        entry_latency: Offset for order entry latency.
        response_latency: Offset for order response latency.
    """

    entry_latency_mul: float64
    resp_latency_mul: float64
    entry_latency: float64
    response_latency: float64

    def __init__(
            self,
            entry_latency_mul=1,
            resp_latency_mul=1,
            entry_latency=0,
            response_latency=0
    ):
        self.entry_latency_mul = entry_latency_mul
        self.resp_latency_mul = resp_latency_mul
        self.entry_latency = entry_latency
        self.response_latency = response_latency

    def __latency(self, proc):
        lat1 = -1
        for row_num in range(proc.row_num, -1, -1):
            local_timestamp = proc.data[row_num, COL_LOCAL_TIMESTAMP]
            exch_timestamp = proc.data[row_num, COL_EXCH_TIMESTAMP]
            if local_timestamp != -1 and exch_timestamp != -1:
                lat1 = local_timestamp - exch_timestamp
                break

        lat2 = -1
        for row_num in range(proc.next_row_num, len(proc.next_data)):
            next_local_timestamp = proc.next_data[row_num, COL_LOCAL_TIMESTAMP]
            next_exch_timestamp = proc.next_data[row_num, COL_EXCH_TIMESTAMP]
            if next_local_timestamp != -1 and next_exch_timestamp != -1:
                lat2 = next_local_timestamp - next_exch_timestamp
                break

        if lat1 != -1 and lat2 != -1:
            return (lat1 + lat2) / 2.0
        elif lat1 != -1:
            return lat1
        elif lat2 != -1:
            return lat2
        else:
            raise ValueError

    def entry(self, timestamp, order, proc):
        return self.entry_latency + self.entry_latency_mul * self.__latency(proc)

    def response(self, timestamp, order, proc):
        return self.response_latency + self.resp_latency_mul * self.__latency(proc)

    def reset(self):
        pass


class ForwardFeedLatency:
    r"""
    Provides order latency based on feed latency. The units of the arguments should match the timestamp units of your
    data.

    Order latency is computed as follows:

    * the subsequent feed's latency(by forward-looking) is used as the feed latency.

    .. code-block::

        entry_latency = feed_latency * entry_latency_mul + entry_latency
        response_latency = feed_latency * resp_latency_mul + response_latency

    Args:
        entry_latency_mul: Multiplier for feed latency to compute order entry latency.
        resp_latency_mul: Multiplier for feed latency to compute order response latency.
        entry_latency: Offset for order entry latency.
        response_latency: Offset for order response latency.
    """

    entry_latency_mul: float64
    resp_latency_mul: float64
    entry_latency: float64
    response_latency: float64

    def __init__(
            self,
            entry_latency_mul=1,
            resp_latency_mul=1,
            entry_latency=0,
            response_latency=0
    ):
        self.entry_latency_mul = entry_latency_mul
        self.resp_latency_mul = resp_latency_mul
        self.entry_latency = entry_latency
        self.response_latency = response_latency

    def __latency(self, proc):
        for row_num in range(proc.next_row_num, len(proc.next_data)):
            next_local_timestamp = proc.next_data[row_num, COL_LOCAL_TIMESTAMP]
            next_exch_timestamp = proc.next_data[row_num, COL_EXCH_TIMESTAMP]
            if next_local_timestamp != -1 and next_exch_timestamp != -1:
                return next_local_timestamp - next_exch_timestamp
        return ValueError

    def entry(self, timestamp, order, proc):
        return self.entry_latency + self.entry_latency_mul * self.__latency(proc)

    def response(self, timestamp, order, proc):
        return self.response_latency + self.resp_latency_mul * self.__latency(proc)

    def reset(self):
        pass


class BackwardFeedLatency:
    r"""
    Provides order latency based on feed latency. The units of the arguments should match the timestamp units of your
    data.

    Order latency is computed as follows:

    * the latest feed's latency is used as the feed latency.

    .. code-block::

        entry_latency = feed_latency * entry_latency_mul + entry_latency
        response_latency = feed_latency * resp_latency_mul + response_latency

    Args:
        entry_latency_mul: Multiplier for feed latency to compute order entry latency.
        resp_latency_mul: Multiplier for feed latency to compute order response latency.
        entry_latency: Offset for order entry latency.
        response_latency: Offset for order response latency.
    """

    entry_latency_mul: float64
    resp_latency_mul: float64
    entry_latency: float64
    response_latency: float64

    def __init__(
            self,
            entry_latency_mul=1,
            resp_latency_mul=1,
            entry_latency=0,
            response_latency=0
    ):
        self.entry_latency_mul = entry_latency_mul
        self.resp_latency_mul = resp_latency_mul
        self.entry_latency = entry_latency
        self.response_latency = response_latency

    def __latency(self, proc):
        for row_num in range(proc.row_num, -1, -1):
            local_timestamp = proc.data[row_num, COL_LOCAL_TIMESTAMP]
            exch_timestamp = proc.data[row_num, COL_EXCH_TIMESTAMP]
            if local_timestamp != -1 and exch_timestamp != -1:
                return local_timestamp - exch_timestamp
            return ValueError

    def entry(self, timestamp, order, proc):
        return self.entry_latency + self.entry_latency_mul * self.__latency(proc)

    def response(self, timestamp, order, proc):
        return self.response_latency + self.resp_latency_mul * self.__latency(proc)

    def reset(self):
        pass


class IntpOrderLatency:
    r"""
    Provides order latency by interpolating the actual historical order latency. This model provides the most accurate
    results. The units of the historical latency data should match the timestamp units of your feed data.

    Args:
        data (array): An (n, 3) array consisting of three columns: local timestamp when the request was made, exchange
            timestamp, and local timestamp when the response was received.
    """

    entry_rn: int64
    resp_rn: int64
    data: float64[:, :]
    
    def __init__(self, data):
        self.entry_rn = 0
        self.resp_rn = 0
        self.data = data

    def __intp(self, x, x1, y1, x2, y2):
        return (y2 - y1) / (x2 - x1) * (x - x1) + y1

    def entry(self, timestamp, order, proc):
        if timestamp < self.data[0, 0]:
            # Finds a valid latency.
            for row_num in range(len(self.data)):
                if self.data[row_num, 1] > 0 and self.data[row_num, 0] > 0:
                    return self.data[row_num, 1] - self.data[row_num, 0]
            raise ValueError
        if timestamp >= self.data[-1, 0]:
            # Finds a valid latency.
            for row_num in range(len(self.data) - 1, -1, -1):
                if self.data[row_num, 1] > 0 and self.data[row_num, 0] > 0:
                    return self.data[row_num, 1] - self.data[row_num, 0]
            raise ValueError
        for row_num in range(self.entry_rn, len(self.data) - 1):
            req_local_timestamp = self.data[row_num, 0]
            next_req_local_timestamp = self.data[row_num + 1, 0]
            if req_local_timestamp <= timestamp < next_req_local_timestamp:
                self.entry_rn = row_num

                exch_timestamp = self.data[row_num, 1]
                next_exch_timestamp = self.data[row_num + 1, 1]

                # The exchange may reject an order request due to technical issues such congestion, this is particularly
                # common in crypto markets. A timestamp of zero on the exchange represents the occurrence of those kinds
                # of errors at that time.
                if exch_timestamp <= 0 or next_exch_timestamp <= 0:
                    resp_timestamp = self.data[row_num, 2]
                    next_resp_timestamp = self.data[row_num + 1, 2]
                    lat1 = resp_timestamp - req_local_timestamp
                    lat2 = next_resp_timestamp - next_req_local_timestamp
                    # Negative latency indicates that the order is rejected for technical reasons, and its value
                    # represents the latency that the local experiences when receiving the rejection notification
                    return -self.__intp(timestamp, req_local_timestamp, lat1, next_req_local_timestamp, lat2)

                lat1 = exch_timestamp - req_local_timestamp
                lat2 = next_exch_timestamp - next_req_local_timestamp
                return self.__intp(timestamp, req_local_timestamp, lat1, next_req_local_timestamp, lat2)
        raise ValueError

    def response(self, timestamp, order, proc):
        if timestamp < self.data[0, 1]:
            # Finds a valid latency.
            for row_num in range(len(self.data)):
                if self.data[row_num, 2] > 0 and self.data[row_num, 1] > 0:
                    return self.data[row_num, 2] - self.data[row_num, 1]
            raise ValueError
        if timestamp >= self.data[-1, 1]:
            # Finds a valid latency.
            for row_num in range(len(self.data) -1, -1, -1):
                if self.data[row_num, 2] > 0 and self.data[row_num, 1] > 0:
                    return self.data[row_num, 2] - self.data[row_num, 1]
            raise ValueError
        for row_num in range(self.resp_rn, len(self.data) - 1):
            exch_timestamp = self.data[row_num, 1]
            next_exch_timestamp = self.data[row_num + 1, 1]
            if exch_timestamp <= timestamp < next_exch_timestamp:
                self.resp_rn = row_num

                resp_local_timestamp = self.data[row_num, 2]
                next_resp_local_timestamp = self.data[row_num + 1, 2]

                lat1 = resp_local_timestamp - exch_timestamp
                lat2 = next_resp_local_timestamp - next_exch_timestamp

                if exch_timestamp <= 0 and next_exch_timestamp <= 0:
                    raise ValueError
                elif exch_timestamp <= 0:
                    return lat2
                elif next_exch_timestamp <= 0:
                    return lat1

                lat = self.__intp(timestamp, exch_timestamp, lat1, next_exch_timestamp, lat2)
                if lat < 0:
                    raise ValueError('Response latency cannot be negative.')
                return lat
        raise ValueError

    def reset(self):
        self.entry_rn = 0
        self.resp_rn = 0
