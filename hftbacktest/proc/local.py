from numba.experimental import jitclass

from .proc import Proc, proc_spec
from ..order import BUY, SELL, NEW, CANCELED, FILLED, EXPIRED, NONE, Order
from ..reader import COL_EVENT, COL_LOCAL_TIMESTAMP, COL_SIDE, COL_PRICE, COL_QTY, DEPTH_CLEAR_EVENT, DEPTH_EVENT, \
    DEPTH_SNAPSHOT_EVENT


class Local_(Proc):
    def __init__(
            self,
            reader,
            orders_to_exch,
            orders_from_exch,
            depth,
            state,
            order_latency
    ):
        self._proc_init(
            reader,
            orders_to_exch,
            orders_from_exch,
            depth,
            state,
            order_latency
        )

    def _next_data_timestamp(self):
        if self.row_num + 1 < len(self.data):
            return self.data[self.row_num + 1, COL_LOCAL_TIMESTAMP]
        else:
            if len(self.next_data) == 0:
                return -2
            return self.next_data[0, COL_LOCAL_TIMESTAMP]

    def _process_recv_order(self, order, recv_timestamp, wait_resp, next_timestamp):
        # Apply the received order response to the local orders.
        self.orders[order.order_id] = order
        if order.status == FILLED:
            self.state.apply_fill(order)

        # Bypass next_timestamp
        return next_timestamp

    def _process_data(self, row):
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
        reader,
        orders_to_exch,
        orders_from_exch,
        depth,
        state,
        order_latency
):
    jitted = jitclass(
        spec=proc_spec(reader, state, order_latency)
    )(Local_)
    return jitted(
        reader,
        orders_to_exch,
        orders_from_exch,
        depth,
        state,
        order_latency
    )
