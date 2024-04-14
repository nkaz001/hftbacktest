from numba import typeof, int64
from numba.experimental import jitclass
from numba.typed.typeddict import Dict
from numba.types import DictType

import numpy as np

from .proc import Proc, proc_spec
from ..marketdepth import INVALID_MAX, INVALID_MIN
from ..order import (
    BUY,
    SELL,
    NEW,
    CANCELED,
    FILLED,
    EXPIRED,
    PARTIALLY_FILLED,
    MODIFY,
    GTX,
    GTC,
    FOK,
    IOC,
    NONE,
    order_ladder_ty
)
from ..reader import (
    COL_EVENT,
    COL_EXCH_TIMESTAMP,
    COL_SIDE,
    COL_PRICE,
    COL_QTY,
    DEPTH_CLEAR_EVENT,
    DEPTH_EVENT,
    DEPTH_SNAPSHOT_EVENT,
    TRADE_EVENT
)


class PartialFillExchange_(Proc):
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
        self._proc_init(
            reader,
            orders_to_local,
            orders_from_local,
            depth,
            state,
            order_latency
        )
        self.sell_orders = Dict.empty(int64, order_ladder_ty)
        self.buy_orders = Dict.empty(int64, order_ladder_ty)
        self.queue_model = queue_model

    def reset(
            self,
            start_position,
            start_balance,
            start_fee,
            maker_fee,
            taker_fee,
            tick_size,
            lot_size,
            snapshot
    ):
        self._proc_reset(
            start_position,
            start_balance,
            start_fee,
            maker_fee,
            taker_fee,
            tick_size,
            lot_size,
            snapshot
        )
        self.sell_orders.clear()
        self.buy_orders.clear()
        self.queue_model.reset()

    def _next_data_timestamp(self):
        return self._next_data_timestamp_column(COL_EXCH_TIMESTAMP)

    def _process_recv_order(self, order, recv_timestamp, wait_resp, next_timestamp):
        # Process a new order.
        if order.req == NEW:
            order.req = NONE
            resp_timestamp = self.__ack_new(order, recv_timestamp)

        # Process a modify order.
        elif order.req == MODIFY:
            order.req = NONE
            resp_timestamp = self.__ack_modify(order, recv_timestamp)

        # Process a cancel order.
        elif order.req == CANCELED:
            order.req = NONE
            resp_timestamp = self.__ack_cancel(order, recv_timestamp)

        else:
            raise ValueError('req')

        # Check if the local waits for the order's response.
        if wait_resp == order.order_id:
            # If next_timestamp is valid, choose the earlier timestamp.
            if next_timestamp > 0:
                return min(resp_timestamp, next_timestamp)
            else:
                return resp_timestamp

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
                # Choose the faster computing path.
                if (self.depth.best_bid_tick == INVALID_MIN) \
                        or (len(self.orders) < price_tick - self.depth.best_bid_tick):
                    for order in list(self.orders.values()):
                        if order.side == SELL:
                            self.__check_if_sell_filled(
                                order,
                                price_tick,
                                qty,
                                row[COL_EXCH_TIMESTAMP]
                            )
                else:
                    for t in range(self.depth.best_bid_tick + 1, price_tick + 1):
                        if t in self.sell_orders:
                            for order in list(self.sell_orders[t].values()):
                                self.__check_if_sell_filled(
                                    order,
                                    price_tick,
                                    qty,
                                    row[COL_EXCH_TIMESTAMP]
                                )
            else:
                # Choose the faster computing path.
                if (self.depth.best_ask_tick == INVALID_MAX) \
                        or (len(self.orders) < self.depth.best_ask_tick - price_tick):
                    for order in list(self.orders.values()):
                        if order.side == BUY:
                            self.__check_if_buy_filled(
                                order,
                                price_tick,
                                qty,
                                row[COL_EXCH_TIMESTAMP]
                            )
                else:
                    for t in range(self.depth.best_ask_tick - 1, price_tick - 1, -1):
                        if t in self.buy_orders:
                            for order in list(self.buy_orders[t].values()):
                                self.__check_if_buy_filled(
                                    order,
                                    price_tick,
                                    qty,
                                    row[COL_EXCH_TIMESTAMP]
                                )
        return 0

    def __check_if_sell_filled(self, order, price_tick, qty, timestamp):
        if order.price_tick < price_tick:
            self.__fill(
                order,
                order.leaves_qty,
                timestamp,
                True
            )
        elif order.price_tick == price_tick:
            # Update the order's queue position.
            self.queue_model.trade(order, qty, self)
            if self.queue_model.is_filled(order, self):
                q_qty = np.floor(-order.q[0] / self.depth.lot_size) * self.depth.lot_size
                exec_qty = min(q_qty, qty, order.leaves_qty)
                self.__fill(
                    order,
                    exec_qty,
                    timestamp,
                    True
                )

    def __check_if_buy_filled(self, order, price_tick, qty, timestamp):
        if order.price_tick > price_tick:
            self.__fill(
                order,
                order.leaves_qty,
                timestamp,
                True
            )
        elif order.price_tick == price_tick:
            # Update the order's queue position.
            self.queue_model.trade(order, qty, self)
            if self.queue_model.is_filled(order, self):
                q_qty = np.floor(-order.q[0] / self.depth.lot_size) * self.depth.lot_size
                exec_qty = min(q_qty, qty, order.leaves_qty)
                self.__fill(
                    order,
                    exec_qty,
                    timestamp,
                    True
                )

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
        if (prev_best == INVALID_MIN) \
                or (len(self.orders) < new_best - prev_best):
            for order in list(self.orders.values()):
                if order.side == SELL and order.price_tick <= new_best:
                    self.__fill(
                        order,
                        order.leaves_qty,
                        timestamp,
                        True
                    )
        else:
            for t in range(prev_best + 1, new_best + 1):
                if t in self.sell_orders:
                    for order in list(self.sell_orders[t].values()):
                        self.__fill(
                            order,
                            order.leaves_qty,
                            timestamp,
                            True
                        )

    def on_best_ask_update(self, prev_best, new_best, timestamp):
        # If the best has been significantly updated compared to the previous best, it would be better to iterate
        # orders dict instead of order price ladder.
        if (prev_best == INVALID_MAX) \
                or (len(self.orders) < prev_best - new_best):
            for order in list(self.orders.values()):
                if order.side == BUY and new_best <= order.price_tick:
                    self.__fill(
                        order,
                        order.leaves_qty,
                        timestamp,
                        True
                    )
        else:
            for t in range(new_best, prev_best):
                if t in self.buy_orders:
                    for order in list(self.buy_orders[t].values()):
                        self.__fill(
                            order,
                            order.leaves_qty,
                            timestamp,
                            True
                        )

    def __ack_new(self, order, timestamp):
        if order.order_id in self.orders:
            raise KeyError('order_id already exists')

        if order.side == BUY:
            # Check if the buy order price is greater than or equal to the current best ask.
            if order.price_tick >= self.depth.best_ask_tick:
                if order.time_in_force == GTX:
                    order.status = EXPIRED
                elif order.time_in_force == FOK:
                    # The order must be executed immediately in its entirety; otherwise, the entire order will be
                    # cancelled.
                    execute = False
                    cum_qty = 0
                    for t in range(self.depth.best_ask_tick, order.price_tick + 1):
                        if t in self.depth.ask_depth:
                            cum_qty += self.depth.ask_depth[t]
                            if round(cum_qty / self.depth.lot_size) >= round(order.qty / self.depth.lot_size):
                                execute = True
                                break
                    if execute:
                        for t in range(self.depth.best_ask_tick, order.price_tick + 1):
                            if t in self.depth.ask_depth:
                                exec_qty = min(self.depth.ask_depth[t], order.leaves_qty)
                                local_recv_timestamp = self.__fill(
                                    order,
                                    exec_qty,
                                    timestamp,
                                    False,
                                    exec_price_tick=t,
                                    delete_order=False
                                )
                                if order.status == FILLED:
                                    return local_recv_timestamp
                    else:
                        order.status = EXPIRED
                elif order.time_in_force == IOC:
                    # The order must be executed immediately
                    for t in range(self.depth.best_ask_tick, order.price_tick + 1):
                        if t in self.depth.ask_depth:
                            exec_qty = min(self.depth.ask_depth[t], order.leaves_qty)
                            local_recv_timestamp = self.__fill(
                                order,
                                exec_qty,
                                timestamp,
                                False,
                                exec_price_tick=t,
                                delete_order=False
                            )
                            if order.status == FILLED:
                                return local_recv_timestamp
                    order.status = EXPIRED
                else:
                    # time_in_force == GTC
                    # Take the market.
                    for t in range(self.depth.best_ask_tick, order.price_tick):
                        if t in self.depth.ask_depth:
                            exec_qty = min(self.depth.ask_depth[t], order.leaves_qty)
                            local_recv_timestamp = self.__fill(
                                order,
                                exec_qty,
                                timestamp,
                                False,
                                exec_price_tick=t,
                                delete_order=False
                            )
                            if order.status == FILLED:
                                return local_recv_timestamp
                    # The buy order cannot remain in the ask book, as it cannot affect the market depth during
                    # backtesting based on market-data replay. So, even though it simulates partial fill, if the order
                    # size is not small enough, it introduces unreality.
                    return self.__fill(
                        order,
                        order.leaves_qty,
                        timestamp,
                        False,
                        exec_price_tick=order.price_tick,
                        delete_order=False
                    )
            else:
                # The exchange accepts this order.
                self.orders[order.order_id] = order
                o = self.buy_orders.setdefault(
                    order.price_tick,
                    Dict.empty(int64, order_ladder_ty)
                )
                o[order.order_id] = order
                # Initialize the order's queue position.
                self.queue_model.new(order, self)
                order.status = NEW
        else:
            # Check if the sell order price is less than or equal to the current best bid.
            if order.price_tick <= self.depth.best_bid_tick:
                if order.time_in_force == GTX:
                    order.status = EXPIRED
                elif order.time_in_force == FOK:
                    # The order must be executed immediately in its entirety; otherwise, the entire order will be
                    # cancelled.
                    execute = False
                    cum_qty = 0
                    for t in range(self.depth.best_bid_tick, order.price_tick - 1, -1):
                        if t in self.depth.bid_depth:
                            cum_qty += self.depth.bid_depth[t]
                            if round(cum_qty / self.depth.lot_size) >= round(order.qty / self.depth.lot_size):
                                execute = True
                                break
                    if execute:
                        for t in range(self.depth.best_bid_tick, order.price_tick - 1, -1):
                            if t in self.depth.bid_depth:
                                exec_qty = min(self.depth.bid_depth[t], order.leaves_qty)
                                local_recv_timestamp = self.__fill(
                                    order,
                                    exec_qty,
                                    timestamp,
                                    False,
                                    exec_price_tick=t,
                                    delete_order=False
                                )
                                if order.status == FILLED:
                                    return local_recv_timestamp
                    else:
                        order.status = EXPIRED
                elif order.time_in_force == IOC:
                    # The order must be executed immediately
                    for t in range(self.depth.best_bid_tick, order.price_tick - 1, -1):
                        if t in self.depth.bid_depth:
                            exec_qty = min(self.depth.bid_depth[t], order.leaves_qty)
                            local_recv_timestamp = self.__fill(
                                order,
                                exec_qty,
                                timestamp,
                                False,
                                exec_price_tick=t,
                                delete_order=False
                            )
                            if order.status == FILLED:
                                return local_recv_timestamp
                    order.status = EXPIRED
                else:
                    # time_in_force == GTC
                    # Take the market.
                    for t in range(self.depth.best_bid_tick, order.price_tick, -1):
                        if t in self.depth.bid_depth:
                            exec_qty = min(self.depth.bid_depth[t], order.leaves_qty)
                            local_recv_timestamp = self.__fill(
                                order,
                                exec_qty,
                                timestamp,
                                False,
                                exec_price_tick=t,
                                delete_order=False
                            )
                            if order.status == FILLED:
                                return local_recv_timestamp

                    # The sell order cannot remain in the bid book, as it cannot affect the market depth during
                    # backtesting based on market-data replay. So, even though it simulates partial fill, if the order
                    # size is not small enough, it introduces unreality.
                    return self.__fill(
                        order,
                        order.leaves_qty,
                        timestamp,
                        False,
                        exec_price_tick=order.price_tick,
                        delete_order=False
                    )
            else:
                # The exchange accepts this order.
                self.orders[order.order_id] = order
                o = self.sell_orders.setdefault(
                    order.price_tick,
                    Dict.empty(int64, order_ladder_ty)
                )
                o[order.order_id] = order
                # Initialize the order's queue position.
                self.queue_model.new(order, self)
                order.status = NEW
        order.exch_timestamp = timestamp
        local_recv_timestamp = timestamp + self.order_latency.response(timestamp, order, self)
        self.orders_to.append(order.copy(), local_recv_timestamp)
        return local_recv_timestamp

    def __ack_modify(self, order, timestamp):
        exch_order = self.orders.get(order.order_id)

        # The order can be already deleted due to fill or expiration.
        if exch_order is None:
            order.status = EXPIRED
            order.exch_timestamp = timestamp
            local_recv_timestamp = timestamp + self.order_latency.response(timestamp, order, self)
            # It can overwrite another existing order on the local side if order_id is the same. So, commented out.
            # self.orders_to.append(order.copy(), local_recv_timestamp)
            return local_recv_timestamp

        prev_price_tick = exch_order.price_tick
        exch_order.price_tick = order.price_tick

        # See https://binance-docs.github.io/apidocs/futures/en/#modify-order-trade
        # It's not sure if this is a general behavior across exchanges.
        executed_qty = exch_order.qty - exch_order.leaves_qty
        if exch_order.status == PARTIALLY_FILLED and order.qty <= executed_qty:
            exch_order.status = EXPIRED

            if exch_order.side == BUY:
                del self.buy_orders[prev_price_tick][exch_order.order_id]
            else:
                del self.sell_orders[prev_price_tick][exch_order.order_id]
            del self.orders[exch_order.order_id]
        else:
            # The initialization of the order queue position may not occur when the modified quantity is smaller than
            # the previous quantity, depending on the exchanges. It may need to implement exchange-specific
            # specialization.
            init_q_pos = True
            exch_order.qty = order.qty

            if exch_order.side == BUY:
                # Check if the buy order price is greater than or equal to the current best ask.
                if exch_order.price_tick >= self.depth.best_ask_tick:
                    del self.buy_orders[prev_price_tick][exch_order.order_id]
                    del self.orders[exch_order.order_id]

                    if exch_order.time_in_force == GTX:
                        exch_order.status = EXPIRED
                    elif exch_order.time_in_force == GTC:
                        # Take the market.
                        for t in range(self.depth.best_ask_tick, exch_order.price_tick):
                            if t in self.depth.ask_depth:
                                exec_qty = min(self.depth.ask_depth[t], exch_order.leaves_qty)
                                local_recv_timestamp = self.__fill(
                                    exch_order,
                                    exec_qty,
                                    timestamp,
                                    False,
                                    exec_price_tick=t,
                                    delete_order=False
                                )
                                if exch_order.status == FILLED:
                                    return local_recv_timestamp
                        # The buy order cannot remain in the ask book, as it cannot affect the market depth during
                        # backtesting based on market-data replay. So, even though it simulates partial fill, if the order
                        # size is not small enough, it introduces unreality.
                        return self.__fill(
                            exch_order,
                            exch_order.leaves_qty,
                            timestamp,
                            False,
                            exec_price_tick=exch_order.price_tick,
                            delete_order=False
                        )
                else:
                    # The exchange accepts this order.
                    if prev_price_tick != exch_order.price_tick:
                        del self.buy_orders[prev_price_tick][exch_order.order_id]
                        o = self.buy_orders.setdefault(
                            exch_order.price_tick,
                            Dict.empty(int64, order_ladder_ty)
                        )
                        o[exch_order.order_id] = exch_order
                    if init_q_pos or prev_price_tick != exch_order.price_tick:
                        # Initialize the order's queue position.
                        self.queue_model.new(exch_order, self)
                    exch_order.status = NEW
            else:
                # Check if the sell order price is less than or equal to the current best bid.
                if exch_order.price_tick <= self.depth.best_bid_tick:
                    del self.sell_orders[prev_price_tick][exch_order.order_id]
                    del self.orders[exch_order.order_id]

                    if exch_order.time_in_force == GTX:
                        exch_order.status = EXPIRED
                    elif exch_order.time_in_force == GTC:
                        # Take the market.
                        for t in range(self.depth.best_bid_tick, exch_order.price_tick, -1):
                            if t in self.depth.bid_depth:
                                exec_qty = min(self.depth.bid_depth[t], exch_order.leaves_qty)
                                local_recv_timestamp = self.__fill(
                                    exch_order,
                                    exec_qty,
                                    timestamp,
                                    False,
                                    exec_price_tick=t,
                                    delete_order=False
                                )
                                if exch_order.status == FILLED:
                                    return local_recv_timestamp
                        # The sell order cannot remain in the bid book, as it cannot affect the market depth during
                        # backtesting based on market-data replay. So, even though it simulates partial fill, if the order
                        # size is not small enough, it introduces unreality.
                        return self.__fill(
                            exch_order,
                            exch_order.leaves_qty,
                            timestamp,
                            False,
                            exec_price_tick=exch_order.price_tick,
                            delete_order=False
                        )
                else:
                    # The exchange accepts this order.
                    if prev_price_tick != exch_order.price_tick:
                        del self.sell_orders[prev_price_tick][exch_order.order_id]
                        o = self.sell_orders.setdefault(
                            exch_order.price_tick,
                            Dict.empty(int64, order_ladder_ty)
                        )
                        o[exch_order.order_id] = exch_order
                    if init_q_pos or prev_price_tick != exch_order.price_tick:
                        # Initialize the order's queue position.
                        self.queue_model.new(exch_order, self)
                    exch_order.status = NEW
        exch_order.exch_timestamp = timestamp
        local_recv_timestamp = timestamp + self.order_latency.response(timestamp, exch_order, self)
        self.orders_to.append(exch_order.copy(), local_recv_timestamp)
        return local_recv_timestamp

    def __ack_cancel(self, order, timestamp):
        exch_order = self.orders.get(order.order_id)

        # The order can be already deleted due to fill or expiration.
        if exch_order is None:
            order.status = EXPIRED
            order.exch_timestamp = timestamp
            local_recv_timestamp = timestamp + self.order_latency.response(timestamp, order, self)
            # It can overwrite another existing order on the local side if order_id is the same. So, commented out.
            # self.orders_to.append(order.copy(), local_recv_timestamp)
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
        local_recv_timestamp = timestamp + self.order_latency.response(timestamp, exch_order, self)
        self.orders_to.append(exch_order.copy(), local_recv_timestamp)
        return local_recv_timestamp

    def __fill(
            self,
            order,
            exec_qty,
            timestamp,
            maker,
            exec_price_tick=0,
            delete_order=True
    ):
        if order.status == EXPIRED \
                or order.status == CANCELED \
                or order.status == FILLED:
            raise ValueError('status')

        order.maker = maker
        order.exec_price_tick = order.price_tick if maker else exec_price_tick
        order.exec_qty = exec_qty
        order.leaves_qty -= exec_qty
        order.status = PARTIALLY_FILLED if round(order.leaves_qty / self.depth.lot_size) > 0 else FILLED
        order.exch_timestamp = timestamp
        local_recv_timestamp = order.exch_timestamp + self.order_latency.response(timestamp, order, self)

        if delete_order and order.status == FILLED:
            del self.orders[order.order_id]

            if order.side == BUY:
                del self.buy_orders[order.price_tick][order.order_id]
            else:
                del self.sell_orders[order.price_tick][order.order_id]

        self.state.apply_fill(order)
        self.orders_to.append(order.copy(), local_recv_timestamp)
        return local_recv_timestamp


def PartialFillExchange(
        reader,
        orders_to_local,
        orders_from_local,
        depth,
        state,
        order_latency,
        queue_model
):
    jitted = jitclass(
        spec=proc_spec(reader, state, order_latency) + [
            ('sell_orders', DictType(int64, order_ladder_ty)),
            ('buy_orders', DictType(int64, order_ladder_ty)),
            ('queue_model', typeof(queue_model))
        ]
    )(PartialFillExchange_)
    return jitted(
        reader,
        orders_to_local,
        orders_from_local,
        depth,
        state,
        order_latency,
        queue_model
    )
