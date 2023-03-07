from numba import typeof, int64, float64
from numba.experimental import jitclass
from numba.typed.typeddict import Dict

from .reader import COL_EVENT, COL_LOCAL_TIMESTAMP, COL_SIDE, COL_PRICE, COL_QTY, DEPTH_CLEAR_EVENT, DEPTH_EVENT, \
    DEPTH_SNAPSHOT_EVENT
from .marketdepth import MarketDepth
from .order import BUY, SELL, NEW, CANCELED, FILLED, EXPIRED, NONE, Order, order_ty, OrderBus, order_ladder_ty


class Local_:
    def __init__(
            self,
            data_reader,
            orders_to_exch,
            orders_from_exch,
            depth,
            state,
            order_latency
    ):
        self.data_reader = data_reader
        self.data = data_reader.next()
        self.next_data = data_reader.next()
        self.row_num = -1

        self.orders = Dict.empty(int64, order_ty)

        self.orders_to = orders_to_exch
        self.orders_from = orders_from_exch

        self.depth = depth
        self.state = state
        self.order_latency = order_latency

    def next_timestamp(self):
        next_data_timestamp = self.__next_data_timestamp()
        next_recv_order_timestamp = self.orders_from.frontmost_timestamp

        if (0 < next_recv_order_timestamp < next_data_timestamp) \
                or (next_data_timestamp <= 0 < next_recv_order_timestamp):
            return next_recv_order_timestamp
        else:
            return next_data_timestamp

    def __next_data_timestamp(self):
        if self.row_num + 1 < len(self.data):
            return self.data[self.row_num + 1, COL_LOCAL_TIMESTAMP]
        else:
            if len(self.next_data) == 0:
                return -2
            return self.next_data[0, COL_LOCAL_TIMESTAMP]

    def __process_recv_order(self, wait_resp):
        next_frontmost_timestamp = 0
        i = 0
        while i < self.orders_from.__len__():
            order, recv_timestamp = self.orders_from[i]
            if self.orders_from.frontmost_timestamp == recv_timestamp:
                self.orders_from.__delitem__(i)

                # Apply the received order response to the local orders.
                self.orders[order.order_id] = order
                if order.status == FILLED:
                    self.state.apply_fill(order)
            else:
                i += 1
                # Find the next frontmost timestamp
                if next_frontmost_timestamp <= 0:
                    next_frontmost_timestamp = recv_timestamp
                else:
                    next_frontmost_timestamp = min(next_frontmost_timestamp, recv_timestamp)
        self.orders_from.frontmost_timestamp = next_frontmost_timestamp
        return 0

    def __process_data(self, wait_resp):
        # Move to the next row.
        self.row_num += 1
        if self.row_num == len(self.data):
            self.data_reader.release(self.data)
            self.data = self.next_data
            self.next_data = self.data_reader.next()
            self.row_num = 0

        row = self.data[self.row_num]

        # Process a depth event
        if row[COL_EVENT] == DEPTH_CLEAR_EVENT:
            self.depth.clear_depth(row[COL_SIDE], row[COL_PRICE])
        elif row[COL_EVENT] == DEPTH_EVENT or row[COL_EVENT] == DEPTH_SNAPSHOT_EVENT:
            if row[COL_SIDE] == BUY:
                self.depth.update_bid_depth(
                    row[COL_PRICE],
                    row[COL_QTY],
                    row[COL_LOCAL_TIMESTAMP]
                )
            else:
                self.depth.update_ask_depth(
                    row[COL_PRICE],
                    row[COL_QTY],
                    row[COL_LOCAL_TIMESTAMP]
                )

        return 0

    def process(self, wait_resp):
        next_data_timestamp = self.__next_data_timestamp()
        next_recv_order_timestamp = self.orders_from.frontmost_timestamp

        if (0 < next_recv_order_timestamp < next_data_timestamp) \
                or (next_data_timestamp <= 0 < next_recv_order_timestamp):
            return self.__process_recv_order(wait_resp)
        else:
            return self.__process_data(wait_resp)

    def submit_buy_order(self, order_id, price, qty, time_in_force, current_timestamp):
        price_tick = round(price / self.depth.tick_size)
        order = Order(order_id, price_tick, self.depth.tick_size, qty, BUY, time_in_force)
        order.req = NEW
        exch_recv_timestamp = current_timestamp + self.order_latency.entry(order, self)

        self.orders[order.order_id] = order
        self.orders_to.append(order.copy(), exch_recv_timestamp)

    def submit_sell_order(self, order_id, price, qty, time_in_force, current_timestamp):
        price_tick = round(price / self.depth.tick_size)
        order = Order(order_id, price_tick, self.depth.tick_size, qty, SELL, time_in_force)
        order.req = NEW
        exch_recv_timestamp = current_timestamp + self.order_latency.entry(order, self)

        self.orders[order.order_id] = order
        self.orders_to.append(order.copy(), exch_recv_timestamp)

    def cancel(self, order_id, current_timestamp):
        order = self.orders.get(order_id)

        if order is None:
            raise KeyError('the given order_id does not exist.')
        if order.req != NONE:
            raise ValueError('the given order cannot be cancelled because there is a ongoing request.')

        order.req = CANCELED
        exch_recv_timestamp = current_timestamp + self.order_latency.entry(order, self)

        self.orders_to.append(order.copy(), exch_recv_timestamp)

    def clear_inactive_orders(self):
        for order in list(self.orders.values()):
            if order.status == EXPIRED \
                    or order.status == FILLED \
                    or order.status == CANCELED:
                del self.orders[order.order_id]


def Local(
        data_reader,
        orders_to_exch,
        orders_from_exch,
        depth,
        state,
        order_latency
):
    jitted = jitclass(spec=[
        ('data_reader', typeof(data_reader)),
        ('data', float64[:, :]),
        ('next_data', float64[:, :]),
        ('row_num', int64),

        ('orders', order_ladder_ty),

        ('orders_to', OrderBus.class_type.instance_type),
        ('orders_from', OrderBus.class_type.instance_type),

        ('depth', MarketDepth.class_type.instance_type),
        ('state', typeof(state)),
        ('order_latency', typeof(order_latency)),
    ])(Local_)
    return jitted(
        data_reader,
        orders_to_exch,
        orders_from_exch,
        depth,
        state,
        order_latency
    )
