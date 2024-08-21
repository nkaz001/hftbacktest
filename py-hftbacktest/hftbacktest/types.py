from typing import Any

import numpy as np

#: Indicates all assets.
ALL_ASSETS = -1  # the maximum value of uint64

#: Indicates that the market depth is changed.
DEPTH_EVENT = 1

#: Indicates that a trade occurs in the market.
TRADE_EVENT = 2

#: Indicates that the market depth is cleared.
DEPTH_CLEAR_EVENT = 3

#: Indicates that the market depth snapshot is received.
DEPTH_SNAPSHOT_EVENT = 4

#: Indicates that the best bid and best ask update event is received.
DEPTH_BBO_EVENT = 5

#: Indicates that an order has been added to the order book.
ADD_ORDER_EVENT = 10

#: Indicates that an order in the order book has been canceled.
CANCEL_ORDER_EVENT = 11

#: Indicates that an order in the order book has been modified.
MODIFY_ORDER_EVENT = 12

#: Indicates that an order in the order book has been filled.
FILL_EVENT = 13

# todo: fix WAIT_ORDER_RESPONSE flags.
WAIT_ORDER_RESPONSE_NONE = -1
WAIT_ORDER_RESPONSE_ANY = -2

#: Indicates that one should continue until the end of the data.
UNTIL_END_OF_DATA = 9223372036854775807  # the maximum value of int64

#: Indicates that it is a valid event to be handled by the exchange processor at the exchange timestamp.
EXCH_EVENT = 1 << 31

#: Indicates that it is a valid event to be handled by the local processor at the local timestamp.
LOCAL_EVENT = 1 << 30

BUY_EVENT = 1 << 29
"""
Indicates a buy, with specific meaning that can vary depending on the situation. 
For example, when combined with a depth event, it means a bid-side event, while when combined with a trade event, 
it means that the trade initiator is a buyer.
"""

SELL_EVENT = 1 << 28
"""
Indicates a sell, with specific meaning that can vary depending on the situation. 
For example, when combined with a depth event, it means an ask-side event, while when combined with a trade event, 
it means that the trade initiator is a seller.
"""

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
        ('ev', 'u8'),
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
