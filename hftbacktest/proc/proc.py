from numba import typeof, float64, int64
from numba.typed import Dict

from ..marketdepth import MarketDepth
from ..order import order_ladder_ty, order_ty, OrderBus


class Proc:
    def __init__(self):
        pass

    def _proc_init(self, reader, orders_to, orders_from, depth, state, order_latency):
        self.reader = reader
        self.data = reader.next()
        self.next_data = reader.next()
        self.row_num = -1

        self.orders = Dict.empty(int64, order_ty)

        self.orders_to = orders_to
        self.orders_from = orders_from

        self.depth = depth
        self.state = state
        self.order_latency = order_latency

    def next_timestamp(self):
        next_data_timestamp = self._next_data_timestamp()
        next_recv_order_timestamp = self.orders_from.frontmost_timestamp

        if (0 < next_recv_order_timestamp < next_data_timestamp) \
                or (next_data_timestamp <= 0 < next_recv_order_timestamp):
            return next_recv_order_timestamp
        else:
            return next_data_timestamp

    def process(self, wait_resp):
        next_data_timestamp = self._next_data_timestamp()
        next_recv_order_timestamp = self.orders_from.frontmost_timestamp

        if (0 < next_recv_order_timestamp < next_data_timestamp) \
                or (next_data_timestamp <= 0 < next_recv_order_timestamp):
            # Process the order part.
            next_timestamp = 0
            next_frontmost_timestamp = 0
            i = 0
            while i < self.orders_from.__len__():
                order, recv_timestamp = self.orders_from[i]
                if self.orders_from.frontmost_timestamp == recv_timestamp:
                    self.orders_from.__delitem__(i)

                    next_timestamp = self._process_recv_order(order, recv_timestamp, wait_resp, next_timestamp)
                else:
                    i += 1
                    # Find the next frontmost timestamp
                    if next_frontmost_timestamp <= 0:
                        next_frontmost_timestamp = recv_timestamp
                    else:
                        next_frontmost_timestamp = min(next_frontmost_timestamp, recv_timestamp)
            self.orders_from.frontmost_timestamp = next_frontmost_timestamp
            return next_timestamp
        else:
            # Process the data part.
            # Move to the next row.
            self.row_num += 1
            if self.row_num == len(self.data):
                self.reader.release(self.data)
                self.data = self.next_data
                self.next_data = self.reader.next()
                self.row_num = 0

            row = self.data[self.row_num]
            return self._process_data(row)

    @property
    def tick_size(self):
        return self.depth.tick_size

    @property
    def lot_size(self):
        return self.depth.lot_size

    @property
    def bid_depth(self):
        return self.depth.bid_depth

    @property
    def ask_depth(self):
        return self.depth.ask_depth


def proc_spec(reader, state, order_latency):
    return [
        ('reader', typeof(reader)),
        ('data', float64[:, :]),
        ('next_data', float64[:, :]),
        ('row_num', int64),

        ('orders', order_ladder_ty),

        ('orders_to', OrderBus.class_type.instance_type),
        ('orders_from', OrderBus.class_type.instance_type),

        ('depth', MarketDepth.class_type.instance_type),
        ('state', typeof(state)),
        ('order_latency', typeof(order_latency)),
    ]
