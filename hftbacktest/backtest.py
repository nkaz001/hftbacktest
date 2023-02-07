import sys

import numba
from numba import int8, float64, int64, boolean
from numba.experimental import jitclass
from numba.typed import Dict
from numba.types import DictType
import numpy as np


COL_EVENT = 0
COL_EXCH_TIMESTAMP = 1
COL_LOCAL_TIMESTAMP = 2
COL_SIDE = 3
COL_PRICE = 4
COL_QTY = 5

DEPTH_EVENT = 1
TRADE_EVENT = 2
DEPTH_CLEAR_EVENT = 3
DEPTH_SNAPSHOT_EVENT = 4
USER_DEFINED_EVENT = 100

BUY = 1
SELL = -1

NONE = 0
NEW = 1
EXPIRED = 2
FILLED = 3
CANCELED = 4

GTC = 0  # Good 'till cancel
GTX = 1  # Post only

INVALID_MIN = -sys.maxsize
INVALID_MAX = sys.maxsize

WAIT_ORDER_RESPONSE_NONE = -1
WAIT_ORDER_RESPONSE_ANY = -2


@numba.njit
def depth_below(depth, start, end):
    for t in range(start - 1, end - 1, -1):
        if t in depth and depth[t] > 0:
            return t
    return INVALID_MIN


@numba.njit
def depth_above(depth, start, end):
    for t in range(start + 1, end + 1):
        if t in depth and depth[t] > 0:
            return t
    return INVALID_MAX


@jitclass([
    ('qty', float64),
    ('price_tick', int64),
    ('tick_size', float64),
    ('side', int8),
    ('time_in_force', int8),
    ('exch_status', int8),
    ('exch_timestamp', int64),
    ('status', int8),
    ('local_timestamp', int64),
    ('req', int8),
    ('req_recv_timestamp', int64),
    ('resp_recv_timestamp', int64),
    ('exec_recv_timestamp', int64),
    ('exec_price_tick', int64),
    ('order_id', int64),
    ('q', float64[:]),
    ('limit', boolean),
])
class Order:
    def __init__(self, order_id, price_tick, tick_size, qty, side, time_in_force):
        self.qty = qty
        self.price_tick = price_tick
        self.tick_size = tick_size
        self.side = side
        self.time_in_force = time_in_force
        # Exchange-acknowledged order status
        self.exch_status = NONE
        self.exch_timestamp = 0
        # Local-acknowledged order status
        self.status = NONE
        self.local_timestamp = 0
        self.req = NONE
        self.req_recv_timestamp = 0
        self.resp_recv_timestamp = 0
        self.exec_recv_timestamp = 0
        self.exec_price_tick = 0
        self.order_id = order_id
        self.q = np.zeros(2, float64)
        self.limit = False

    def __get_price(self):
        return self.price_tick * self.tick_size

    def __get_exec_price(self):
        return self.exec_price_tick * self.tick_size

    def __get_cancellable(self):
        return self.status == NEW and self.req == NONE

    price = property(__get_price)

    exec_price = property(__get_exec_price)

    cancellable = property(__get_cancellable)


order_type = Order.class_type.instance_type
dict_type = DictType(int64, order_type)

hbt_cls_spec = [
    ('data', float64[:, :]),
    ('row_num', int64),
    ('ask_depth', DictType(int64, float64)),
    ('bid_depth', DictType(int64, float64)),
    ('orders', dict_type),
    ('sell_orders', DictType(int64, dict_type)),
    ('buy_orders', DictType(int64, dict_type)),
    ('position', float64),
    ('balance', float64),
    ('fee', float64),
    ('trade_num', int64),
    ('trade_qty', float64),
    ('trade_amount', float64),
    ('tick_size', float64),
    ('lot_size', float64),
    ('best_bid_tick', int64),
    ('best_ask_tick', int64),
    ('low_bid_tick', int64),
    ('high_ask_tick', int64),
    ('run', boolean),
    ('maker_fee', float64),
    ('taker_fee', float64),
    ('local_timestamp', int64),
    ('trade_len', int64),
    ('last_trades_', float64[:]),
    ('user_data', float64[:, :]),
]


class HftBacktest:
    def __init__(self,
                 data,
                 tick_size,
                 lot_size,
                 maker_fee,
                 taker_fee,
                 order_latency,
                 asset_type,
                 queue_model,
                 snapshot=None,
                 start_row=0,
                 start_position=0,
                 start_balance=0,
                 start_fee=0,
                 trade_list_size=1000):
        self.data = data
        self.row_num = start_row
        self.ask_depth = Dict.empty(int64, float64)
        self.bid_depth = Dict.empty(int64, float64)
        self.orders = Dict.empty(int64, order_type)
        self.sell_orders = Dict.empty(int64, dict_type)
        self.buy_orders = Dict.empty(int64, dict_type)
        self.position = start_position
        self.balance = start_balance
        self.fee = start_fee
        self.trade_num = 0
        self.trade_qty = 0
        self.trade_amount = 0
        self.tick_size = tick_size
        self.lot_size = lot_size
        self.best_bid_tick = INVALID_MIN
        self.best_ask_tick = INVALID_MAX
        self.low_bid_tick = INVALID_MAX
        self.high_ask_tick = INVALID_MIN
        self.run = True
        self.maker_fee = maker_fee
        self.taker_fee = taker_fee
        self.local_timestamp = self.start_timestamp
        self.order_latency = order_latency
        self.asset_type = asset_type
        self.queue_model = queue_model
        self.trade_len = 0
        self.last_trades_ = np.full((trade_list_size, data.shape[1]), np.nan, np.float64)
        self.user_data = np.full((20, data.shape[1]), np.nan, np.float64)
        if snapshot is not None:
            self.__load_snapshot(snapshot)

    def __load_snapshot(self, data):
        self.best_bid_tick = INVALID_MIN
        self.best_ask_tick = INVALID_MAX
        self.low_bid_tick = INVALID_MAX
        self.high_ask_tick = INVALID_MIN
        self.bid_depth.clear()
        self.ask_depth.clear()
        best_bid = True
        best_ask = True
        for row_num in range(len(data)):
            row = data[row_num]
            price_tick = round(row[COL_PRICE] / self.tick_size)
            qty = row[COL_QTY]
            if row[COL_SIDE] == BUY:
                if best_bid:
                    self.best_bid_tick = price_tick
                    best_bid = False
                self.low_bid_tick = price_tick
                self.bid_depth[price_tick] = qty
            elif row[COL_SIDE] == SELL:
                if best_ask:
                    self.best_ask_tick = price_tick
                    best_ask = False
                self.high_ask_tick = price_tick
                self.ask_depth[price_tick] = qty

    def __fill(self, order, timestamp, limit, exec_price_tick=0):
        order.limit = limit
        order.exec_price_tick = order.price_tick if limit else exec_price_tick
        order.exch_status = FILLED
        order.exch_timestamp = timestamp
        order.exec_recv_timestamp = order.exch_timestamp + self.order_latency.response(order, self)
        if limit:
            if order.side == BUY:
                del self.buy_orders[order.price_tick][order.order_id]
            else:
                del self.sell_orders[order.price_tick][order.order_id]

    def __apply_fill(self, order):
        fee = self.maker_fee if order.limit else self.taker_fee
        exec_price = order.exec_price_tick * self.tick_size
        fill_qty = order.qty * order.side
        amount = self.asset_type.amount(exec_price, order.qty)
        fill_amount = amount * order.side
        fee_amount = amount * fee
        self.position += fill_qty
        self.balance -= fill_amount
        self.fee += fee_amount
        self.trade_num += 1
        self.trade_qty += order.qty
        self.trade_amount += amount

    def submit_buy_order(self, order_id, price, qty, time_in_force, wait=False):
        price_tick = round(price / self.tick_size)
        order = Order(order_id, price_tick, self.tick_size, qty, BUY, time_in_force)
        order.req = NEW
        order.req_recv_timestamp = self.local_timestamp + self.order_latency.entry(order, self)
        self.orders[order.order_id] = order
        if wait:
            return self.goto(self.last_timestamp, wait_order_response=order_id)
        return True

    def submit_sell_order(self, order_id, price, qty, time_in_force, wait=False):
        price_tick = round(price / self.tick_size)
        order = Order(order_id, price_tick, self.tick_size, qty, SELL, time_in_force)
        order.req = NEW
        order.req_recv_timestamp = self.local_timestamp + self.order_latency.entry(order, self)
        self.orders[order.order_id] = order
        if wait:
            return self.goto(self.last_timestamp, wait_order_response=order_id)
        return True

    def cancel(self, order_id, wait=False):
        order = self.orders.get(order_id)
        if order.req != NONE:
            raise ValueError('req')
        order.req = CANCELED
        order.req_recv_timestamp = self.local_timestamp + self.order_latency.entry(order, self)
        order.resp_recv_timestamp = 0
        if wait:
            return self.goto(self.last_timestamp, wait_order_response=order_id)
        return True

    def wait_order_response(self, order_id, timeout=-1):
        if timeout >= 0:
            timestamp = self.local_timestamp + timeout
        else:
            timestamp = max(self.local_timestamp, self.last_timestamp)
        return self.goto(timestamp, wait_order_response=order_id)

    def wait_next_feed(self, include_order_resp, timeout=-1):
        if timeout >= 0:
            timestamp = self.local_timestamp + timeout
        else:
            timestamp = max(self.local_timestamp, self.last_timestamp)

        next_local_timestamp = timestamp
        for row_num in range(self.row_num + 1, len(self.data)):
            next_local_timestamp = self.data[row_num, COL_LOCAL_TIMESTAMP]
            if next_local_timestamp > 0:
                break
        next_local_timestamp = min(timestamp, next_local_timestamp)
        wait_order_response = WAIT_ORDER_RESPONSE_ANY if include_order_resp else WAIT_ORDER_RESPONSE_NONE
        return self.goto(next_local_timestamp, wait_order_response=wait_order_response)

    def clear_inactive_orders(self):
        for order in list(self.orders.values()):
            if order.status == EXPIRED \
                    or order.status == FILLED \
                    or order.status == CANCELED:
                del self.orders[order.order_id]

    def clear_last_trades(self):
        self.trade_len = 0

    def get_user_data(self, event):
        return self.user_data[event - USER_DEFINED_EVENT]

    def __get_last_trades(self):
        return self.last_trades_[:self.trade_len]

    def __get_start_timestamp(self):
        return self.data[0, COL_LOCAL_TIMESTAMP]

    def __get_last_timestamp(self):
        return self.data[-1, COL_LOCAL_TIMESTAMP]

    def __get_best_bid(self):
        return self.best_bid_tick * self.tick_size

    def __get_best_ask(self):
        return self.best_ask_tick * self.tick_size

    def __compute_mid(self):
        return (self.best_bid + self.best_ask) / 2.0

    def __compute_equity(self):
        return self.asset_type.equity(self.mid, self.balance, self.position, self.fee)

    last_trades = property(__get_last_trades)

    start_timestamp = property(__get_start_timestamp)

    last_timestamp = property(__get_last_timestamp)

    best_bid = property(__get_best_bid)

    best_ask = property(__get_best_ask)

    mid = property(__compute_mid)

    equity = property(__compute_equity)

    def elapse(self, duration):
        return self.goto(self.local_timestamp + duration)

    def goto(self, timestamp, wait_order_response=WAIT_ORDER_RESPONSE_NONE):
        found_order_resp_timestamp = False
        while self.row_num + 1 < len(self.data):
            next_local_timestamp = self.data[self.row_num + 1, COL_LOCAL_TIMESTAMP]
            next_exch_timestamp = self.data[self.row_num + 1, COL_EXCH_TIMESTAMP]
            exch_timestamp = self.data[self.row_num, COL_EXCH_TIMESTAMP]

            # exchange timestamp must be ahead of local timestamp.
            # assert next_local_timestamp > next_exch_timestamp

            # A user order cannot be processed between the rows that have the same exchange timestamp.
            # These events happen all at once in an exchange.
            if exch_timestamp != next_exch_timestamp:
                for order in self.orders.values():
                    # Check if an exchange receives a user order.
                    if exch_timestamp >= order.req_recv_timestamp:
                        # Process a new order.
                        if order.req == NEW:
                            order.req = NONE
                            if order.side == BUY:
                                # Check if a buy order price is greater than or equal to the current best ask.
                                if order.price_tick >= self.best_ask_tick:
                                    if order.time_in_force == GTX:
                                        order.exch_status = EXPIRED
                                    else:
                                        # Take the market.
                                        self.__fill(order,
                                                    order.req_recv_timestamp,
                                                    False, exec_price_tick=self.best_ask_tick)
                                        if wait_order_response == WAIT_ORDER_RESPONSE_ANY \
                                                and self.local_timestamp < order.exec_recv_timestamp < timestamp:
                                            timestamp = order.exec_recv_timestamp
                                else:
                                    # Now a user order is active. An exchange accepts a user order.
                                    o = self.buy_orders.setdefault(order.price_tick, Dict.empty(int64, dict_type))
                                    o[order.order_id] = order
                                    # Initialize the order's queue position.
                                    self.queue_model.new(order, self)
                                    order.exch_status = NEW
                            else:
                                # Check if a sell order price is less than or equal to the current best bid.
                                if order.price_tick <= self.best_bid_tick:
                                    if order.time_in_force == GTX:
                                        order.exch_status = EXPIRED
                                    else:
                                        # Take the market.
                                        self.__fill(order,
                                                    order.req_recv_timestamp,
                                                    False, exec_price_tick=self.best_bid_tick)
                                        if wait_order_response == WAIT_ORDER_RESPONSE_ANY \
                                                and self.local_timestamp < order.exec_recv_timestamp < timestamp:
                                            timestamp = order.exec_recv_timestamp
                                else:
                                    # Now a user order is active. An exchange accepts a user order.
                                    o = self.sell_orders.setdefault(order.price_tick, Dict.empty(int64, dict_type))
                                    o[order.order_id] = order
                                    # Initialize the order's queue position.
                                    self.queue_model.new(order, self)
                                    order.exch_status = NEW
                            order.exch_timestamp = order.req_recv_timestamp
                            order.resp_recv_timestamp = order.exch_timestamp + self.order_latency.response(order, self)
                            if wait_order_response == WAIT_ORDER_RESPONSE_ANY \
                                    and self.local_timestamp < order.resp_recv_timestamp < timestamp:
                                timestamp = order.resp_recv_timestamp

                        # Process a cancel order.
                        if order.req == CANCELED:
                            order.req = NONE
                            # Cancel request is ignored if its status isn't active.
                            if order.exch_status == NEW:
                                order.exch_status = CANCELED
                                order.exch_timestamp = order.req_recv_timestamp
                                order.resp_recv_timestamp = order.exch_timestamp + self.order_latency.response(order, self)
                                if wait_order_response == WAIT_ORDER_RESPONSE_ANY \
                                        and self.local_timestamp < order.resp_recv_timestamp < timestamp:
                                    timestamp = order.resp_recv_timestamp

                                if order.side == BUY:
                                    del self.buy_orders[order.price_tick][order.order_id]
                                else:
                                    del self.sell_orders[order.price_tick][order.order_id]
                            else:
                                order.exch_timestamp = order.req_recv_timestamp
                                order.resp_recv_timestamp = order.req_recv_timestamp + self.order_latency.response(order, self)
                                if wait_order_response == WAIT_ORDER_RESPONSE_ANY \
                                        and self.local_timestamp < order.resp_recv_timestamp < timestamp:
                                    timestamp = order.resp_recv_timestamp
                    if wait_order_response == order.order_id \
                            and order.resp_recv_timestamp != 0 \
                            and not found_order_resp_timestamp:
                        timestamp = max(order.resp_recv_timestamp, self.local_timestamp)
                        found_order_resp_timestamp = True

            # Exit the loop if it processes all data rows before a given target local timestamp.
            # -1 means timestamp is invalid so ignored.
            if next_local_timestamp > timestamp and next_local_timestamp != -1:
                break
            # Get the next row.
            self.row_num += 1
            row = self.data[self.row_num]
            exch_timestamp = next_exch_timestamp
            local_timestamp = next_local_timestamp

            # Order fill logic needs valid exch_timestamp.
            # And market depth also needs valid local_timestamp as it needs to be received by local.
            if exch_timestamp != -1 and local_timestamp != -1:
                if row[COL_EVENT] == DEPTH_CLEAR_EVENT:
                    # To apply market depth snapshot, refresh the market depth.
                    clear_upto = round(row[COL_PRICE] / self.tick_size)
                    if row[COL_SIDE] == BUY:
                        if self.best_bid_tick != INVALID_MIN:
                            for t in range(self.best_bid_tick, clear_upto - 1, -1):
                                if t in self.ask_depth:
                                    del self.ask_depth[t]
                    elif row[COL_SIDE] == SELL:
                        if self.best_ask_tick != INVALID_MAX:
                            for t in range(self.best_ask_tick, clear_upto + 1):
                                if t in self.ask_depth:
                                    del self.ask_depth[t]
                    else:
                        self.bid_depth.clear()
                        self.ask_depth.clear()
                elif row[COL_EVENT] == DEPTH_EVENT or row[COL_EVENT] == DEPTH_SNAPSHOT_EVENT:
                    # Update the market depth.
                    price_tick = round(row[COL_PRICE] / self.tick_size)
                    qty = row[COL_QTY]
                    if row[COL_SIDE] == BUY:
                        prev_qty = self.bid_depth.get(price_tick, 0)
                        self.bid_depth[price_tick] = qty
                        # Update a user order's queue position.
                        if price_tick in self.buy_orders:
                            for order in self.buy_orders[price_tick].values():
                                self.queue_model.depth(order, prev_qty, qty, self)
                        # Update the best bid and the best ask.
                        if round(qty / self.lot_size) == 0:
                            del self.bid_depth[price_tick]
                            if price_tick == self.best_bid_tick:
                                self.best_bid_tick = depth_below(self.bid_depth, self.best_bid_tick, self.low_bid_tick)
                        else:
                            if price_tick > self.best_bid_tick:
                                # Not sure if it's okay to fill orders by the best bid/ask without trade. But, without it
                                # there are active orders even if they cross the best bid/ask and the backtest gets messy.
                                # As this backtest assumes no market impact it would be fine, but it's better to compare
                                # with the actual trading result.

                                # Fill sell orders placed in the bid-side.
                                if self.best_bid_tick != INVALID_MIN and row[COL_EVENT] == DEPTH_EVENT:
                                    for t in range(self.best_bid_tick + 1, price_tick + 1):
                                        if t in self.sell_orders:
                                            for order in list(self.sell_orders[t].values()):
                                                self.__fill(order, exch_timestamp, True)
                                                if wait_order_response == WAIT_ORDER_RESPONSE_ANY \
                                                        and self.local_timestamp < order.exec_recv_timestamp < timestamp:
                                                    timestamp = order.exec_recv_timestamp
                                self.best_bid_tick = price_tick
                                if self.best_bid_tick >= self.best_ask_tick:
                                    self.best_ask_tick = depth_above(self.ask_depth, self.best_bid_tick, self.high_ask_tick)
                            if price_tick < self.low_bid_tick:
                                self.low_bid_tick = price_tick
                    else:
                        prev_qty = self.ask_depth.get(price_tick, 0)
                        self.ask_depth[price_tick] = qty
                        # Update a user order's queue position.
                        if price_tick in self.sell_orders:
                            for order in self.sell_orders[price_tick].values():
                                self.queue_model.depth(order, prev_qty, qty, self)
                        # Update the best bid and the best ask.
                        if round(qty / self.lot_size) == 0:
                            del self.ask_depth[price_tick]
                            if price_tick == self.best_ask_tick:
                                self.best_ask_tick = depth_above(self.ask_depth, self.best_ask_tick, self.high_ask_tick)
                        else:
                            if price_tick < self.best_ask_tick:
                                # Not sure if it's okay to fill orders by the best bid/ask without trade. But, without it
                                # there are active orders even if they cross the best bid/ask and the backtest gets messy.
                                # As this backtest assumes no market impact it would be fine, but it's better to compare
                                # with the actual trading result.

                                # Fill buy orders placed in the ask-side.
                                if self.best_ask_tick != INVALID_MAX and row[COL_EVENT] == DEPTH_EVENT:
                                    for t in range(price_tick, self.best_ask_tick):
                                        if t in self.buy_orders:
                                            for order in list(self.buy_orders[t].values()):
                                                self.__fill(order, exch_timestamp, True)
                                                if wait_order_response == WAIT_ORDER_RESPONSE_ANY \
                                                        and self.local_timestamp < order.exec_recv_timestamp < timestamp:
                                                    timestamp = order.exec_recv_timestamp
                                self.best_ask_tick = price_tick
                                if self.best_ask_tick <= self.best_bid_tick:
                                    self.best_bid_tick = depth_below(self.bid_depth, self.best_ask_tick, self.low_bid_tick)
                            if price_tick > self.high_ask_tick:
                                self.high_ask_tick = price_tick
            # Order fill logic needs valid exch_timestamp.
            if exch_timestamp != -1:
                if row[COL_EVENT] == TRADE_EVENT:
                    # Check if a user order is filled.
                    # To simplify the backtest and avoid a complex market-impact model, all user orders are
                    # considered to be small enough not to make any market impact.
                    price_tick = round(row[COL_PRICE] / self.tick_size)
                    qty = row[COL_QTY]
                    # This side is a trade initiator's side.
                    if row[COL_SIDE] == BUY:
                        if self.best_bid_tick != INVALID_MIN:
                            for t in range(self.best_bid_tick + 1, price_tick + 1):
                                if t in self.sell_orders:
                                    for order in list(self.sell_orders[t].values()):
                                        # Only if a user order is active.
                                        if order.exch_status == NEW:
                                            if order.price_tick < price_tick:
                                                self.__fill(order, exch_timestamp, True)
                                                if wait_order_response == WAIT_ORDER_RESPONSE_ANY \
                                                        and self.local_timestamp < order.exec_recv_timestamp < timestamp:
                                                    timestamp = order.exec_recv_timestamp
                                            elif order.price_tick == price_tick:
                                                # Update the order's queue position.
                                                self.queue_model.trade(order, qty, self)
                                                if self.queue_model.is_filled(order, self):
                                                    self.__fill(order, exch_timestamp, True)
                                                    if wait_order_response == WAIT_ORDER_RESPONSE_ANY \
                                                            and self.local_timestamp < order.exec_recv_timestamp < timestamp:
                                                        timestamp = order.exec_recv_timestamp
                    else:
                        if self.best_ask_tick != INVALID_MAX:
                            for t in range(self.best_ask_tick - 1, price_tick - 1, -1):
                                if t in self.buy_orders:
                                    for order in list(self.buy_orders[t].values()):
                                        # Only if a user order is active.
                                        if order.exch_status == NEW:
                                            if order.price_tick > price_tick:
                                                self.__fill(order, exch_timestamp, True)
                                                if wait_order_response == WAIT_ORDER_RESPONSE_ANY \
                                                        and self.local_timestamp < order.exec_recv_timestamp < timestamp:
                                                    timestamp = order.exec_recv_timestamp
                                            elif order.price_tick == price_tick:
                                                # Update the order's queue position.
                                                self.queue_model.trade(order, qty, self)
                                                if self.queue_model.is_filled(order, self):
                                                    self.__fill(order, exch_timestamp, True)
                                                    if wait_order_response == WAIT_ORDER_RESPONSE_ANY \
                                                            and self.local_timestamp < order.exec_recv_timestamp < timestamp:
                                                        timestamp = order.exec_recv_timestamp
            # Only row with the valid local_timestamp will be received by local.
            if local_timestamp != -1:
                if row[COL_EVENT] == TRADE_EVENT and self.trade_len < self.last_trades_.shape[0] - 1:
                    self.last_trades_[self.trade_len, :] = row[:]
                    self.trade_len += 1
                elif row[COL_EVENT] >= USER_DEFINED_EVENT:
                    i = int(row[COL_EVENT]) - USER_DEFINED_EVENT
                    if i >= len(self.user_data):
                        raise ValueError
                    self.user_data[i, :] = row[:]

        # Check if the local can receive an order status.
        for order in self.orders.values():
            if order.status != order.exch_status:
                if timestamp >= order.exec_recv_timestamp:
                    order.status = order.exch_status
                    order.local_timestamp = order.exec_recv_timestamp
                    # The local can acknowledge the changes of balance and position by order fill.
                    if order.status == FILLED:
                        self.__apply_fill(order)
                elif timestamp >= order.resp_recv_timestamp:
                    order.status = order.exch_status
                    order.local_timestamp = order.resp_recv_timestamp

        self.local_timestamp = timestamp
        if self.row_num + 1 == len(self.data):
            self.run = False
            return False
        return True
