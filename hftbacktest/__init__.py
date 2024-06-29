from numba import (
    boolean,
    uint64,
    int64,
    float64
)
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

lib = ctypes.CDLL(os.path.join(lib_path, so_file[0]))

hbt_elapse = lib.hbt_elapse
hbt_elapse.restype = ctypes.c_int64
hbt_elapse.argtypes = [ctypes.c_uint64, ctypes.c_uint64]

hbt_current_timestamp = lib.hbt_current_timestamp
hbt_current_timestamp.restype = ctypes.c_int64
hbt_current_timestamp.argtypes = [ctypes.c_uint64]

hbt_depth_typed = lib.hbt_depth_typed
hbt_depth_typed.restype = ctypes.c_uint64
hbt_depth_typed.argtypes = [ctypes.c_uint64, ctypes.c_uint64]

depth_best_bid_tick = lib.depth_best_bid_tick
depth_best_bid_tick.restype = ctypes.c_int32
depth_best_bid_tick.argtypes = [ctypes.c_uint64]

depth_best_ask_tick = lib.depth_best_ask_tick
depth_best_ask_tick.restype = ctypes.c_int32
depth_best_ask_tick.argtypes = [ctypes.c_uint64]


@jitclass
class MultiAssetMultiExchangeBacktest:
    _ptr: uint64

    def __init__(self, ptr: uint64):
        self._ptr = ptr

    def current_timestamp(self) -> int64:
        return hbt_current_timestamp(self._ptr)

    def depth_typed(self, asset_no: uint64) -> uint64:
        return MarketDepth(hbt_depth_typed(self._ptr, asset_no))

    def elapse(self, duration: uint64) -> int64:
        return hbt_elapse(self._ptr, duration)


@jitclass
class MarketDepth:
    _ptr: uint64

    def __init__(self, ptr: uint64):
        self._ptr = ptr

    def best_bid_tick(self):
        return depth_best_bid_tick(self._ptr)

    def best_ask_tick(self):
        return depth_best_ask_tick(self._ptr)
