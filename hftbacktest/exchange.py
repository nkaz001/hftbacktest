from numba import typeof, int64, float64
from numba.experimental import jitclass
from numba.typed.typeddict import Dict
from numba.types import DictType

from .order import BUY, SELL, NEW, CANCELED, FILLED, EXPIRED, GTX, NONE, order_ty, OrderBus, order_ladder_ty
from .reader import COL_EVENT, COL_EXCH_TIMESTAMP, COL_SIDE, COL_PRICE, COL_QTY, DEPTH_CLEAR_EVENT, DEPTH_EVENT, \
    DEPTH_SNAPSHOT_EVENT, TRADE_EVENT
from .marketdepth import MarketDepth, INVALID_MAX, INVALID_MIN


class NoPartialFillExch_:
    def __init__(
            self,
            reader,
            orders_to_local,
            orders_from_local,
            depth,
            state,
            order_latency,
            queue_model
    ):
        self.reader = reader
        self.data = reader.next()
        self.next_data = reader.next()
        self.row_num = -1

        self.orders = Dict.empty(int64, order_ty)
        self.sell_orders = Dict.empty(int64, order_ladder_ty)
        self.buy_orders = Dict.empty(int64, order_ladder_ty)

        self.orders_to = orders_to_local
        self.orders_from = orders_from_local

        self.depth = depth
        self.state = state

        self.order_latency = order_latency
        self.queue_model = queue_model

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
            return self.data[self.row_num + 1, COL_EXCH_TIMESTAMP]
        else:
            if len(self.next_data) == 0:
                return -2
            return self.next_data[0, COL_EXCH_TIMESTAMP]

    def __process_recv_order(self, wait_resp):
        next_timestamp = 0
        next_frontmost_timestamp = 0
        i = 0
        while i < self.orders_from.__len__():
            order, recv_timestamp = self.orders_from[i]
            if self.orders_from.frontmost_timestamp == recv_timestamp:
                self.orders_from.__delitem__(i)

                # Process a new order.
                if order.req == NEW:
                    order.req = NONE
                    resp_timestamp = self.__ack_new(order, recv_timestamp)

                # Process a cancel order.
                elif order.req == CANCELED:
                    order.req = NONE
                    resp_timestamp = self.__ack_cancel(order, recv_timestamp)

                else:
                    raise ValueError('req')

                # Check if the local waits for the order's response.
                if wait_resp == order.order_id:
                    next_timestamp = resp_timestamp
            else:
                i += 1
                # Find the next frontmost timestamp
                if next_frontmost_timestamp <= 0:
                    next_frontmost_timestamp = recv_timestamp
                else:
                    next_frontmost_timestamp = min(next_frontmost_timestamp, recv_timestamp)
        self.orders_from.frontmost_timestamp = next_frontmost_timestamp
        return next_timestamp

    def __process_data(self, wait_resp):
        # Move to the next row.
        self.row_num += 1
        if self.row_num == len(self.data):
            self.reader.release(self.data)
            self.data = self.next_data
            self.next_data = self.reader.next()
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
                    row[COL_EXCH_TIMESTAMP],
                    self
                )
            else:
                self.depth.update_ask_depth(
                    row[COL_PRICE],
                    row[COL_QTY],
                    row[COL_EXCH_TIMESTAMP],
                    self
                )

        # Process a trade event
        elif row[COL_EVENT] == TRADE_EVENT:
            # Check if a user order is filled.
            # To simplify the backtest and avoid a complex market-impact model, all user orders are
            # considered to be small enough not to make any market impact.
            price_tick = round(row[COL_PRICE] / self.depth.tick_size)
            qty = row[COL_QTY]
            # This side is a trade initiator's side.
            if row[COL_SIDE] == BUY:
                if self.depth.best_bid_tick != INVALID_MIN:
                    for t in range(self.depth.best_bid_tick + 1, price_tick + 1):
                        if t in self.sell_orders:
                            for order in list(self.sell_orders[t].values()):
                                if order.price_tick < price_tick:
                                    self.__fill(order, row[COL_EXCH_TIMESTAMP], True)
                                elif order.price_tick == price_tick:
                                    # Update the order's queue position.
                                    self.queue_model.trade(order, qty, self)
                                    if self.queue_model.is_filled(order, self):
                                        self.__fill(order, row[COL_EXCH_TIMESTAMP], True)
            else:
                if self.depth.best_ask_tick != INVALID_MAX:
                    for t in range(self.depth.best_ask_tick - 1, price_tick - 1, -1):
                        if t in self.buy_orders:
                            for order in list(self.buy_orders[t].values()):
                                if order.price_tick > price_tick:
                                    self.__fill(order, row[COL_EXCH_TIMESTAMP], True)
                                elif order.price_tick == price_tick:
                                    # Update the order's queue position.
                                    self.queue_model.trade(order, qty, self)
                                    if self.queue_model.is_filled(order, self):
                                        self.__fill(order, row[COL_EXCH_TIMESTAMP], True)
        return 0

    def process(self, wait_resp):
        next_data_timestamp = self.__next_data_timestamp()
        next_recv_order_timestamp = self.orders_from.frontmost_timestamp

        if (0 < next_recv_order_timestamp < next_data_timestamp) \
                or (next_data_timestamp <= 0 < next_recv_order_timestamp):
            return self.__process_recv_order(wait_resp)
        else:
            return self.__process_data(wait_resp)

    def on_new(self, order):
        self.queue_model.new(order, self)

    def on_bid_qty_chg(
            self,
            price_tick,
            prev_qty,
            new_qty,
            timestamp
    ):
        if price_tick in self.buy_orders:
            for order in self.buy_orders[price_tick].values():
                self.queue_model.depth(order, prev_qty, new_qty, self)

    def on_ask_qty_chg(
            self,
            price_tick,
            prev_qty,
            new_qty,
            timestamp
    ):
        if price_tick in self.sell_orders:
            for order in self.sell_orders[price_tick].values():
                self.queue_model.depth(order, prev_qty, new_qty, self)

    def on_best_bid_update(self, prev_best, new_best, timestamp):
        # If the best has been significantly updated compared to the previous best, it would be better to iterate
        # orders dict instead of order price ladder.
        if len(self.orders) < new_best - prev_best:
            for order in list(self.orders.values()):
                if order.side == SELL and prev_best < order.price_tick < new_best + 1:
                    self.__fill(order, timestamp, True)
        else:
            for t in range(prev_best + 1, new_best + 1):
                if t in self.sell_orders:
                    for order in list(self.sell_orders[t].values()):
                        self.__fill(order, timestamp, True)

    def on_best_ask_update(self, prev_best, new_best, timestamp):
        # If the best has been significantly updated compared to the previous best, it would be better to iterate
        # orders dict instead of order price ladder.
        if len(self.orders) < prev_best - new_best:
            for order in list(self.orders.values()):
                if order.side == BUY and new_best <= order.price_tick < prev_best:
                    self.__fill(order, timestamp, True)
        else:
            for t in range(new_best, prev_best):
                if t in self.buy_orders:
                    for order in list(self.buy_orders[t].values()):
                        self.__fill(order, timestamp, True)

    def __ack_new(self, order, timestamp):
        if order.side == BUY:
            # Check if the buy order price is greater than or equal to the current best ask.
            if order.price_tick >= self.depth.best_ask_tick:
                if order.time_in_force == GTX:
                    order.status = EXPIRED
                else:
                    # Take the market.
                    return self.__fill(
                        order,
                        timestamp,
                        False,
                        exec_price_tick=self.depth.best_ask_tick,
                        delete_order=False
                    )
            else:
                # The exchange accepts this order.
                self.orders[order.order_id] = order
                o = self.buy_orders.setdefault(order.price_tick, Dict.empty(int64, order_ladder_ty))
                o[order.order_id] = order
                # Initialize the order's queue position.
                self.queue_model.new(order, self)
                order.status = NEW
        else:
            # Check if the sell order price is less than or equal to the current best bid.
            if order.price_tick <= self.depth.best_bid_tick:
                if order.time_in_force == GTX:
                    order.status = EXPIRED
                else:
                    # Take the market.
                    return self.__fill(
                        order,
                        timestamp,
                        False,
                        exec_price_tick=self.depth.best_bid_tick,
                        delete_order=False
                    )
            else:
                # The exchange accepts this order.
                self.orders[order.order_id] = order
                o = self.sell_orders.setdefault(order.price_tick, Dict.empty(int64, order_ladder_ty))
                o[order.order_id] = order
                # Initialize the order's queue position.
                self.queue_model.new(order, self)
                order.status = NEW
        order.exch_timestamp = timestamp
        local_recv_timestamp = timestamp + self.order_latency.response(order, self)
        self.orders_to.append(order.copy(), local_recv_timestamp)
        return local_recv_timestamp

    def __ack_cancel(self, order, timestamp):
        exch_order = self.orders.get(order.order_id)

        # The order can be already deleted due to fill or expiration.
        if exch_order is None:
            order.status = EXPIRED
            order.exch_timestamp = timestamp
            local_recv_timestamp = timestamp + self.order_latency.response(order, self)
            self.orders_to.append(order.copy(), local_recv_timestamp)
            return local_recv_timestamp

        # Delete the order.
        del self.orders[exch_order.order_id]
        if exch_order.side == BUY:
            del self.buy_orders[exch_order.price_tick][exch_order.order_id]
        else:
            del self.sell_orders[exch_order.price_tick][exch_order.order_id]

        # Make the response.
        exch_order.status = CANCELED
        exch_order.exch_timestamp = timestamp
        local_recv_timestamp = timestamp + self.order_latency.response(exch_order, self)
        self.orders_to.append(exch_order.copy(), local_recv_timestamp)
        return local_recv_timestamp

    def __fill(self, order, timestamp, limit, exec_price_tick=0, delete_order=True):
        order.limit = limit
        order.exec_price_tick = order.price_tick if limit else exec_price_tick
        order.status = FILLED
        order.exch_timestamp = timestamp
        local_recv_timestamp = order.exch_timestamp + self.order_latency.response(order, self)

        if delete_order:
            del self.orders[order.order_id]

        if limit and delete_order:
            if order.side == BUY:
                del self.buy_orders[order.price_tick][order.order_id]
            else:
                del self.sell_orders[order.price_tick][order.order_id]

        self.state.apply_fill(order)
        self.orders_to.append(order.copy(), local_recv_timestamp)
        return local_recv_timestamp


def NoPartialFillExch(
        reader,
        orders_to_local,
        orders_from_local,
        depth,
        state,
        order_latency,
        queue_model
):
    jitted = jitclass(spec=[
        ('reader', typeof(reader)),
        ('data', float64[:, :]),
        ('next_data', float64[:, :]),
        ('row_num', int64),

        ('orders', order_ladder_ty),
        ('sell_orders', DictType(int64, order_ladder_ty)),
        ('buy_orders', DictType(int64, order_ladder_ty)),

        ('orders_to', OrderBus.class_type.instance_type),
        ('orders_from', OrderBus.class_type.instance_type),

        ('depth', MarketDepth.class_type.instance_type),
        ('state', typeof(state)),
        ('order_latency', typeof(order_latency)),
        ('queue_model', typeof(queue_model)),
    ])(NoPartialFillExch_)
    return jitted(
        reader,
        orders_to_local,
        orders_from_local,
        depth,
        state,
        order_latency,
        queue_model
    )
