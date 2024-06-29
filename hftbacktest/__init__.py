from typing import Tuple, Optional

import numpy as np
from numba import (
    carray,
    boolean,
    uint64,
    int64,
    float64,
    int32,
    float32,
    uint8, types
)
from numba.core import cgutils
from numba.core.extending import intrinsic
from numba.experimental import jitclass

import ctypes

import os
import os.path

from .data import (
    merge_on_local_timestamp,
    validate_data,
    correct_local_timestamp,
    correct_exch_timestamp,
    correct_exch_timestamp_adjust,
    correct,
)

from .order import BUY, SELL, NONE, NEW, EXPIRED, FILLED, CANCELED, MODIFY, GTC, GTX

# Re-exports
import hftbacktest_ffi
from hftbacktest_ffi import *

__all__ = (
    # Side
    'BUY',
    'SELL',

    # Order status
    'NONE',
    'NEW',
    'EXPIRED',
    'FILLED',
    'CANCELED',
    'MODIFY',

    # Time-In-Force
    'GTC',
    'GTX',

    'merge_on_local_timestamp',
    'validate_data',
    'correct_local_timestamp',
    'correct_exch_timestamp',
    'correct_exch_timestamp_adjust',
    'correct'
)

__version__ = '2.0.0-alpha'

lib_path = hftbacktest_ffi.__path__[0]
so_file = [f for f in os.listdir(lib_path) if os.path.isfile(os.path.join(lib_path, f)) and f.endswith('.so')]
if len(so_file) == 0:
    raise RuntimeError('Couldn\'t find hftbackest_ffi.')


@intrinsic
def ptr_from_val(typingctx, data):
    def impl(context, builder, signature, args):
        ptr = cgutils.alloca_once_value(builder,args[0])
        return ptr
    sig = types.CPointer(data)(data)
    return sig, impl


@intrinsic
def val_from_ptr(typingctx, data):
    def impl(context, builder, signature, args):
        val = builder.load(args[0])
        return val
    sig = data.dtype(data)
    return sig, impl


lib = ctypes.CDLL(os.path.join(lib_path, so_file[0]))

hbt_elapse = lib.hbt_elapse
hbt_elapse.restype = ctypes.c_int64
hbt_elapse.argtypes = [ctypes.c_uint64, ctypes.c_uint64]

hbt_elapse_bt = lib.hbt_elapse_bt
hbt_elapse_bt.restype = ctypes.c_int64
hbt_elapse_bt.argtypes = [ctypes.c_uint64, ctypes.c_uint64]

hbt_hbt_wait_order_response = lib.hbt_wait_order_response
hbt_hbt_wait_order_response.restype = ctypes.c_int64
hbt_hbt_wait_order_response.argtypes = [ctypes.c_uint64, ctypes.c_int64, ctypes.c_int64]

hbt_wait_next_feed = lib.hbt_wait_next_feed
hbt_wait_next_feed.restype = ctypes.c_int64
hbt_wait_next_feed.argtypes = [ctypes.c_uint64, ctypes.c_bool, ctypes.c_int64]

hbt_close = lib.hbt_close
hbt_close.restype = ctypes.c_int64
hbt_close.argtypes = [ctypes.c_uint64]

hbt_position = lib.hbt_position
hbt_position.restype = ctypes.c_double
hbt_position.argtypes = [ctypes.c_uint64, ctypes.c_uint64]

hbt_current_timestamp = lib.hbt_current_timestamp
hbt_current_timestamp.restype = ctypes.c_int64
hbt_current_timestamp.argtypes = [ctypes.c_uint64]

hbt_depth_typed = lib.hbt_depth_typed
hbt_depth_typed.restype = ctypes.c_uint64
hbt_depth_typed.argtypes = [ctypes.c_uint64, ctypes.c_uint64]

hbt_trade_typed = lib.hbt_trade_typed
hbt_trade_typed.restype = ctypes.c_uint64
hbt_trade_typed.argtypes = [ctypes.c_uint64, ctypes.c_uint64, ctypes.c_void_p]

hbt_num_assets = lib.hbt_num_assets
hbt_num_assets.restype = ctypes.c_uint64
hbt_num_assets.argtypes = [ctypes.c_uint64]

hbt_submit_buy_order = lib.hbt_submit_buy_order
hbt_submit_buy_order.restype = ctypes.c_int64
hbt_submit_buy_order.argtypes = [
    ctypes.c_uint64,
    ctypes.c_uint64,
    ctypes.c_int64,
    ctypes.c_float,
    ctypes.c_float,
    ctypes.c_uint8,
    ctypes.c_uint8,
    ctypes.c_bool
]

hbt_submit_sell_order = lib.hbt_submit_sell_order
hbt_submit_sell_order.restype = ctypes.c_int64
hbt_submit_sell_order.argtypes = [
    ctypes.c_uint64,
    ctypes.c_uint64,
    ctypes.c_int64,
    ctypes.c_float,
    ctypes.c_float,
    ctypes.c_uint8,
    ctypes.c_uint8,
    ctypes.c_bool
]

hbt_cancel = lib.hbt_cancel
hbt_cancel.restype = ctypes.c_int64
hbt_cancel.argtypes = [ctypes.c_uint64, ctypes.c_uint64, ctypes.c_int64, ctypes.c_bool]

hbt_clear_last_trades = lib.hbt_clear_last_trades
hbt_clear_last_trades.restype = ctypes.c_void_p
hbt_clear_last_trades.argtypes = [ctypes.c_uint64, ctypes.c_uint64]

hbt_clear_inactive_orders = lib.hbt_clear_inactive_orders
hbt_clear_inactive_orders.restype = ctypes.c_void_p
hbt_clear_inactive_orders.argtypes = [ctypes.c_uint64, ctypes.c_uint64]


@jitclass
class MultiAssetMultiExchangeBacktest:
    _ptr: uint64

    def __init__(self, ptr: uint64):
        self._ptr = ptr

    def current_timestamp(self) -> int64:
        return hbt_current_timestamp(self._ptr)

    def depth_typed(self, asset_no: uint64) -> 'MarketDepth':
        return MarketDepth(hbt_depth_typed(self._ptr, asset_no))

    def num_assets(self) -> uint64:
        return hbt_num_assets(self._ptr)

    def position(self, asset_no: uint64) -> float64:
        return hbt_position(self._ptr, asset_no)

    def state_values(self, asset_no: uint64) -> np.ndarray:
        raise NotImplementedError

    def trade_typed(self, asset_no: uint64) -> np.ndarray:
        length: uint64 = 0
        len_ptr = ptr_from_val(length)
        ptr = hbt_trade_typed(self._ptr, asset_no, len_ptr)
        return carray(
            ptr,
            (val_from_ptr(len_ptr),),
            [('ev', 'i8'), ('exch_ts', 'i8'), ('local_ts', 'i8'), ('px', 'f4'), ('qty', 'f4')]
        )

    def clear_last_trades(self, asset_no: Optional[uint64]) -> None:
        if asset_no is None:
            hbt_clear_last_trades(self._ptr, -1)
        else:
            hbt_clear_last_trades(self._ptr, asset_no)

    def orders(self, asset_no: uint64):
        raise NotImplementedError

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
        return hbt_submit_buy_order(self._ptr, asset_no, order_id, price, qty, time_in_force, order_type, wait)

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
        return hbt_submit_sell_order(self._ptr, asset_no, order_id, price, qty, time_in_force, order_type, wait)

    def cancel(self, asset_no: uint64, order_id: int64, wait: bool) -> int64:
        return hbt_cancel(self._ptr, asset_no, order_id, wait)

    def clear_inactive_orders(self, asset_no: Optional[uint64]) -> None:
        if asset_no is None:
            hbt_clear_inactive_orders(self._ptr, asset_no)
        else:
            hbt_clear_inactive_orders(self._ptr, asset_no)

    def wait_order_response(self, asset_no: uint64, order_id: int64, timeout: int64) -> int64:
        return hbt_hbt_wait_order_response(self._ptr, asset_no, order_id, timeout)

    def wait_next_feed(self, include_order_resp: bool, timeout: int64) -> int64:
        return hbt_wait_next_feed(self._ptr, include_order_resp, timeout)

    def elapse(self, duration: uint64) -> int64:
        return hbt_elapse(self._ptr, duration)

    def elapse_bt(self, duration: int64) -> int64:
        return hbt_elapse_bt(self._ptr, duration)

    def close(self) -> int64:
        return hbt_close(self._ptr)

    def feed_latency(self, asset_no: uint64) -> Tuple[int64, int64]:
        raise NotImplementedError

    def order_latency(self, asset_no: uint64) -> Tuple[int64, int64, int64]:
        raise NotImplementedError


depth_best_bid_tick = lib.depth_best_bid_tick
depth_best_bid_tick.restype = ctypes.c_int32
depth_best_bid_tick.argtypes = [ctypes.c_uint64]

depth_best_ask_tick = lib.depth_best_ask_tick
depth_best_ask_tick.restype = ctypes.c_int32
depth_best_ask_tick.argtypes = [ctypes.c_uint64]

depth_best_bid = lib.depth_best_bid
depth_best_bid.restype = ctypes.c_float
depth_best_bid.argtypes = [ctypes.c_uint64]

depth_best_ask = lib.depth_best_ask
depth_best_ask.restype = ctypes.c_float
depth_best_ask.argtypes = [ctypes.c_uint64]

depth_tick_size = lib.depth_tick_size
depth_tick_size.restype = ctypes.c_float
depth_tick_size.argtypes = [ctypes.c_uint64]

depth_lot_size = lib.depth_lot_size
depth_lot_size.restype = ctypes.c_float
depth_lot_size.argtypes = [ctypes.c_uint64]

depth_bid_qty_at_tick = lib.depth_bid_qty_at_tick
depth_bid_qty_at_tick.restype = ctypes.c_float
depth_bid_qty_at_tick.argtypes = [ctypes.c_uint64, ctypes.c_int32]

depth_ask_qty_at_tick = lib.depth_ask_qty_at_tick
depth_ask_qty_at_tick.restype = ctypes.c_float
depth_ask_qty_at_tick.argtypes = [ctypes.c_uint64, ctypes.c_int32]


@jitclass
class MarketDepth:
    _ptr: uint64

    def __init__(self, ptr: uint64):
        self._ptr = ptr

    def best_bid_tick(self) -> int32:
        return depth_best_bid_tick(self._ptr)

    def best_ask_tick(self) -> int32:
        return depth_best_ask_tick(self._ptr)

    def best_bid(self) -> float32:
        return depth_best_bid(self._ptr)

    def best_ask(self) -> float32:
        return depth_best_ask(self._ptr)

    def tick_size(self) -> float32:
        return depth_tick_size(self._ptr)

    def lot_size(self) -> float32:
        return depth_lot_size(self._ptr)

    def bid_qty_at_tick(self, price_tick: int32) -> float32:
        return depth_bid_qty_at_tick(self._ptr, price_tick)

    def ask_qty_at_tick(self, price_tick: int32) -> float32:
        return depth_ask_qty_at_tick(self._ptr, price_tick)
