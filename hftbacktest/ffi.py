import os.path
from ctypes import (
    c_void_p,
    c_bool,
    c_float,
    c_double,
    c_uint8,
    c_uint64,
    c_int32,
    c_int64,
    POINTER,
    CDLL
)
from typing import Tuple

import hftbacktest_ffi
import numba
import numpy as np
from numba import (
    carray,
    uint64,
    int64,
    float64,
    int32,
    float32,
    uint8,
)
from numba.core.types import voidptr
from numba.experimental import jitclass

from .intrinsic import ptr_from_val, address_as_void_pointer, val_from_ptr, is_null_ptr
from .order import order_dtype, Order

lib_path = hftbacktest_ffi.__path__[0]
so_file = [
    f for f in os.listdir(lib_path)
    if os.path.isfile(os.path.join(lib_path, f)) and f.startswith('hftbacktest_ffi') and f.endswith('.so')
]
if len(so_file) == 0:
    raise RuntimeError('Couldn\'t find hftbackest_ffi.')


lib = CDLL(os.path.join(lib_path, so_file[0]))


depth_best_bid_tick = lib.depth_best_bid_tick
depth_best_bid_tick.restype = c_int32
depth_best_bid_tick.argtypes = [c_void_p]

depth_best_ask_tick = lib.depth_best_ask_tick
depth_best_ask_tick.restype = c_int32
depth_best_ask_tick.argtypes = [c_void_p]

depth_best_bid = lib.depth_best_bid
depth_best_bid.restype = c_float
depth_best_bid.argtypes = [c_void_p]

depth_best_ask = lib.depth_best_ask
depth_best_ask.restype = c_float
depth_best_ask.argtypes = [c_void_p]

depth_tick_size = lib.depth_tick_size
depth_tick_size.restype = c_float
depth_tick_size.argtypes = [c_void_p]

depth_lot_size = lib.depth_lot_size
depth_lot_size.restype = c_float
depth_lot_size.argtypes = [c_void_p]

depth_bid_qty_at_tick = lib.depth_bid_qty_at_tick
depth_bid_qty_at_tick.restype = c_float
depth_bid_qty_at_tick.argtypes = [c_void_p, c_int32]

depth_ask_qty_at_tick = lib.depth_ask_qty_at_tick
depth_ask_qty_at_tick.restype = c_float
depth_ask_qty_at_tick.argtypes = [c_void_p, c_int32]


@jitclass
class MarketDepth:
    ptr: voidptr

    def __init__(self, ptr: voidptr):
        self.ptr = ptr

    def best_bid_tick(self) -> int32:
        return depth_best_bid_tick(self.ptr)

    def best_ask_tick(self) -> int32:
        return depth_best_ask_tick(self.ptr)

    def best_bid(self) -> float32:
        return depth_best_bid(self.ptr)

    def best_ask(self) -> float32:
        return depth_best_ask(self.ptr)

    def tick_size(self) -> float32:
        return depth_tick_size(self.ptr)

    def lot_size(self) -> float32:
        return depth_lot_size(self.ptr)

    def bid_qty_at_tick(self, price_tick: int32) -> float32:
        return depth_bid_qty_at_tick(self.ptr, price_tick)

    def ask_qty_at_tick(self, price_tick: int32) -> float32:
        return depth_ask_qty_at_tick(self.ptr, price_tick)


orders_get = lib.orders_get
orders_get.restype = c_void_p
orders_get.argtypes = [c_void_p, c_int64]

orders_contains = lib.orders_get
orders_contains.restype = bool
orders_contains.argtypes = [c_void_p, c_int64]

orders_values = lib.orders_values
orders_values.restype = c_void_p
orders_values.argtypes = [c_void_p]

orders_values_next = lib.orders_values_next
orders_values_next.restype = c_void_p
orders_values_next.argtypes = [c_void_p]


@jitclass
class Values:
    ptr: voidptr

    def __init__(self, ptr: voidptr):
        self.ptr = ptr

    # def __next__(self) -> Order:
    #     if is_null_ptr(self.ptr):
    #         return None
    #     order_ptr = orders_values_next(self.ptr)
    #     if is_null_ptr(order_ptr):
    #         self.ptr = 0
    #         raise StopIteration
    #     else:
    #         arr = carray(
    #             address_as_void_pointer(order_ptr),
    #             1,
    #             dtype=order_dtype
    #         )
    #         return Order(arr)

    def next(self) -> Order | None:
        if is_null_ptr(self.ptr):
            return None
        order_ptr = orders_values_next(self.ptr)
        if is_null_ptr(order_ptr):
            self.ptr = 0
            return None
        else:
            arr = carray(
                address_as_void_pointer(order_ptr),
                1,
                dtype=order_dtype
            )
            return Order(arr)


@jitclass
class OrderDict:
    ptr: voidptr

    def __init__(self, ptr: voidptr):
        self.ptr = ptr

    def values(self) -> Values:
        return Values(orders_values(self.ptr))

    def get(self, order_id: int64) -> Order | None:
        order_ptr = orders_get(self.ptr, order_id)
        if is_null_ptr(order_ptr):
            return None
        else:
            arr = carray(
                address_as_void_pointer(order_ptr),
                1,
                dtype=order_dtype
            )
            return Order(arr)

    def __contains__(self, item: int64) -> bool:
        return orders_contains(self.ptr, item)


hbt_elapse = lib.hbt_elapse
hbt_elapse.restype = c_int64
hbt_elapse.argtypes = [c_void_p, c_uint64]

hbt_elapse_bt = lib.hbt_elapse_bt
hbt_elapse_bt.restype = c_int64
hbt_elapse_bt.argtypes = [c_void_p, c_uint64]

hbt_hbt_wait_order_response = lib.hbt_wait_order_response
hbt_hbt_wait_order_response.restype = c_int64
hbt_hbt_wait_order_response.argtypes = [c_void_p, c_int64, c_int64]

hbt_wait_next_feed = lib.hbt_wait_next_feed
hbt_wait_next_feed.restype = c_int64
hbt_wait_next_feed.argtypes = [c_void_p, c_bool, c_int64]

hbt_close = lib.hbt_close
hbt_close.restype = c_int64
hbt_close.argtypes = [c_void_p]

hbt_position = lib.hbt_position
hbt_position.restype = c_double
hbt_position.argtypes = [c_void_p, c_uint64]

hbt_current_timestamp = lib.hbt_current_timestamp
hbt_current_timestamp.restype = c_int64
hbt_current_timestamp.argtypes = [c_void_p]

hbt_depth_typed = lib.hbt_depth_typed
hbt_depth_typed.restype = c_void_p
hbt_depth_typed.argtypes = [c_void_p, c_uint64]

hbt_trade_typed = lib.hbt_trade_typed
hbt_trade_typed.restype = c_void_p
hbt_trade_typed.argtypes = [c_void_p, c_uint64, POINTER(c_uint64)]

hbt_num_assets = lib.hbt_num_assets
hbt_num_assets.restype = c_uint64
hbt_num_assets.argtypes = [c_void_p]

hbt_submit_buy_order = lib.hbt_submit_buy_order
hbt_submit_buy_order.restype = c_int64
hbt_submit_buy_order.argtypes = [
    c_void_p,
    c_uint64,
    c_int64,
    c_float,
    c_float,
    c_uint8,
    c_uint8,
    c_bool
]

hbt_submit_sell_order = lib.hbt_submit_sell_order
hbt_submit_sell_order.restype = c_int64
hbt_submit_sell_order.argtypes = [
    c_void_p,
    c_uint64,
    c_int64,
    c_float,
    c_float,
    c_uint8,
    c_uint8,
    c_bool
]

hbt_cancel = lib.hbt_cancel
hbt_cancel.restype = c_int64
hbt_cancel.argtypes = [c_void_p, c_uint64, c_int64, c_bool]

hbt_clear_last_trades = lib.hbt_clear_last_trades
hbt_clear_last_trades.restype = c_void_p
hbt_clear_last_trades.argtypes = [c_void_p, c_uint64]

hbt_clear_inactive_orders = lib.hbt_clear_inactive_orders
hbt_clear_inactive_orders.restype = c_void_p
hbt_clear_inactive_orders.argtypes = [c_void_p, c_uint64]

hbt_orders = lib.hbt_orders
hbt_orders.restype = c_void_p
hbt_orders.argtypes = [c_void_p, c_uint64]

hbt_state_values = lib.hbt_state_values
hbt_state_values.restype = c_void_p
hbt_state_values.argtypes = [c_void_p, c_uint64]

hbt_feed_latency = lib.hbt_feed_latency
hbt_feed_latency.restype = c_bool
hbt_feed_latency.argtypes = [c_void_p, c_uint64, POINTER(c_int64), POINTER(c_int64)]

hbt_order_latency = lib.hbt_order_latency
hbt_order_latency.restype = c_bool
hbt_order_latency.argtypes = [c_void_p, c_uint64, POINTER(c_int64), POINTER(c_int64), POINTER(c_int64)]

event_dtype = np.dtype([
    ('ev', 'i8'),
    ('exch_ts', 'i8'),
    ('local_ts', 'i8'),
    ('px', 'f4'),
    ('qty', 'f4')
])

state_values_dtype = np.dtype([
    ('position', 'f8'),
    ('balance', 'f8'),
    ('fee', 'f8'),
    ('trade_qty', 'f8'),
    ('trade_amount', 'f8'),
    ('trade_num', 'i4'),
])


@jitclass
class MultiAssetMultiExchangeBacktest:
    ptr: voidptr

    def __init__(self, ptr: voidptr):
        self.ptr = ptr

    def current_timestamp(self) -> int64:
        return hbt_current_timestamp(self.ptr)

    def depth_typed(self, asset_no: uint64) -> MarketDepth:
        return MarketDepth(hbt_depth_typed(self.ptr, asset_no))

    def num_assets(self) -> uint64:
        return hbt_num_assets(self.ptr)

    def position(self, asset_no: uint64) -> float64:
        return hbt_position(self.ptr, asset_no)

    def state_values(self, asset_no: uint64) -> state_values_dtype:
        ptr = hbt_state_values(self.ptr, asset_no)
        return numba.carray(
            address_as_void_pointer(ptr),
            1,
            state_values_dtype
        )

    def trade_typed(self, asset_no: uint64) -> event_dtype:
        length = uint64(0)
        len_ptr = ptr_from_val(length)
        ptr = hbt_trade_typed(self.ptr, asset_no, len_ptr)
        return numba.carray(
            address_as_void_pointer(ptr),
            val_from_ptr(len_ptr),
            event_dtype
        )

    def clear_last_trades(self, asset_no: uint64) -> None:
        hbt_clear_last_trades(self.ptr, asset_no)

    def orders(self, asset_no: uint64) -> OrderDict:
        return OrderDict(hbt_orders(self.ptr, asset_no))

    def submit_buy_order(
            self,
            asset_no: uint64,
            order_id: int64,
            price: float32,
            qty: float32,
            time_in_force: uint8,
            order_type: uint8,
            wait: bool
    ) -> int64:
        return hbt_submit_buy_order(self.ptr, asset_no, order_id, price, qty, time_in_force, order_type, wait)

    def submit_sell_order(
            self,
            asset_no: uint64,
            order_id: int64,
            price: float32,
            qty: float32,
            time_in_force: uint8,
            order_type: uint8,
            wait: bool
    ) -> int64:
        return hbt_submit_sell_order(self.ptr, asset_no, order_id, price, qty, time_in_force, order_type, wait)

    def cancel(self, asset_no: uint64, order_id: int64, wait: bool) -> int64:
        return hbt_cancel(self.ptr, asset_no, order_id, wait)

    def clear_inactive_orders(self, asset_no: uint64) -> None:
        hbt_clear_inactive_orders(self.ptr, asset_no)

    def wait_order_response(self, asset_no: uint64, order_id: int64, timeout: int64) -> int64:
        return hbt_hbt_wait_order_response(self.ptr, asset_no, order_id, timeout)

    def wait_next_feed(self, include_order_resp: bool, timeout: int64) -> int64:
        return hbt_wait_next_feed(self.ptr, include_order_resp, timeout)

    def elapse(self, duration: uint64) -> int64:
        return hbt_elapse(self.ptr, duration)

    def elapse_bt(self, duration: int64) -> int64:
        return hbt_elapse_bt(self.ptr, duration)

    def close(self) -> int64:
        return hbt_close(self.ptr)

    def feed_latency(self, asset_no: uint64) -> Tuple[int64, int64] | None:
        exch_ts = int64(0)
        local_ts = int64(0)
        exch_ts_ptr = ptr_from_val(exch_ts)
        local_ts_ptr = ptr_from_val(local_ts)
        if hbt_feed_latency(self.ptr, asset_no, exch_ts_ptr, local_ts_ptr):
            return val_from_ptr(exch_ts_ptr), val_from_ptr(local_ts_ptr)
        return None

    def order_latency(self, asset_no: uint64) -> Tuple[int64, int64, int64] | None:
        req_ts = int64(0)
        exch_ts = int64(0)
        resp_ts = int64(0)
        req_ts_ptr = ptr_from_val(req_ts)
        exch_ts_ptr = ptr_from_val(exch_ts)
        resp_ts_ptr = ptr_from_val(resp_ts)
        if hbt_order_latency(self.ptr, asset_no, req_ts_ptr, exch_ts_ptr, resp_ts_ptr):
            return val_from_ptr(req_ts_ptr), val_from_ptr(exch_ts_ptr), val_from_ptr(resp_ts_ptr)
        return None
