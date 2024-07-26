import sys
from typing import Any

import numpy as np

ALL_ASSETS = -1  # the maximum value of uint64_t

DEPTH_EVENT = 1
TRADE_EVENT = 2
DEPTH_CLEAR_EVENT = 3
DEPTH_SNAPSHOT_EVENT = 4

# todo: fix WAIT_ORDER_RESPONSE flags.
WAIT_ORDER_RESPONSE_NONE = -1
WAIT_ORDER_RESPONSE_ANY = -2

UNTIL_END_OF_DATA = sys.maxsize

EXCH_EVENT = 1 << 31
LOCAL_EVENT = 1 << 30

BUY_EVENT = 1 << 29
SELL_EVENT = 1 << 28

state_values_dtype = np.dtype(
    [
        ('position', 'f8'),
        ('balance', 'f8'),
        ('fee', 'f8'),
        ('num_trades', 'i8'),
        ('trading_volume', 'f8'),
        ('trading_value', 'f8')
    ],
    align=True
)

event_dtype = np.dtype(
    [
        ('ev', 'i8'),
        ('exch_ts', 'i8'),
        ('local_ts', 'i8'),
        ('px', 'f8'),
        ('qty', 'f8'),
        ('order_id', 'u8'),
        ('ival', 'i8'),
        ('fval', 'f8')
    ],
    align=True
)

EVENT_ARRAY = np.ndarray[Any, event_dtype]

order_dtype = np.dtype(
    [
        ('qty', 'f8'),
        ('leaves_qty', 'f8'),
        ('exec_qty', 'f8'),
        ('exec_price_tick', 'i8'),
        ('price_tick', 'i8'),
        ('tick_size', 'f8'),
        ('exch_timestamp', 'i8'),
        ('local_timestamp', 'i8'),
        ('order_id', 'u8'),
        ('_q1', 'u8'),
        ('_q2', 'u8'),
        ('maker', 'bool'),
        ('order_type', 'u1'),
        ('req', 'u1'),
        ('status', 'u1'),
        ('side', 'i1'),
        ('time_in_force', 'u1')
    ],
    align=True
)

record_dtype = np.dtype(
    [
        ('timestamp', 'i8'),
        ('price', 'f8'),
        ('position', 'f8'),
        ('balance', 'f8'),
        ('fee', 'f8'),
        ('num_trades', 'i8'),
        ('trading_volume', 'f8'),
        ('trading_value', 'f8')
    ],
    align=True
)
