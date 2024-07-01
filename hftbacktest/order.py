import numpy as np
from numba import float32, int32, int64, uint8, from_dtype
from numba.experimental import jitclass
from numpy.typing import NDArray

UNSUPPORTED = 255

BUY = 1
SELL = -1

NONE = 0
NEW = 1
EXPIRED = 2
FILLED = 3
CANCELED = 4
PARTIALLY_FILLED = 5
MODIFY = 6
REJECTED = 7

GTC = 0  # Good 'till cancel
GTX = 1  # Post only
FOK = 2  # Fill or kill
IOC = 3  # Immediate or cancel

LIMIT = 0
MARKET = 1

order_dtype = np.dtype([
    ('qty', 'f4'),
    ('leaves_qty', 'f4'),
    ('price_tick', 'i4'),
    ('tick_size', 'f4'),
    ('exch_timestamp', 'i8'),
    ('local_timestamp', 'i8'),
    ('exec_price_tick', 'i4'),
    ('exec_qty', 'f4'),
    ('order_id', 'i8'),
    ('_q1', 'u8'),
    ('_q2', 'u8'),
    ('maker', 'bool'),
    ('order_type', 'u1'),
    ('req', 'u1'),
    ('status', 'u1'),
    ('side', 'u1'),
    ('time_in_force', 'u1'),
])


@jitclass
class Order:
    _arr: from_dtype(order_dtype)[:]

    def __init__(self, arr: order_dtype):
        self._arr = arr

    @property
    def price(self) -> float32:
        return self._arr[0].price_tick * self._arr[0].tick_size

    @property
    def exec_price(self) -> float32:
        return self._arr[0].exec_price_tick * self._arr[0].tick_size

    @property
    def cancellable(self) -> bool:
        return (self._arr[0].status == NEW or self._arr[0].status == PARTIALLY_FILLED) and self._arr[0].req == NONE

    @property
    def qty(self) -> float32:
        return self._arr[0].qty

    @property
    def leaves_qty(self) -> float32:
        return self._arr[0].leaves_qty

    @property
    def price_tick(self) -> int32:
        return self._arr[0].price_tick

    @property
    def tick_size(self) -> float32:
        return self._arr[0].price_tick

    @property
    def exch_timestamp(self) -> int64:
        return self._arr[0].exch_timestamp

    @property
    def local_timestamp(self) -> int64:
        return self._arr[0].local_timestamp

    @property
    def exec_price_tick(self) -> int32:
        return self._arr[0].exec_price_tick

    @property
    def exec_qty(self) -> float32:
        return self._arr[0].exec_qty

    @property
    def order_id(self) -> int64:
        return self._arr[0].order_id

    @property
    def order_type(self) -> uint8:
        return self._arr[0].order_type

    @property
    def req(self) -> uint8:
        return self._arr[0].req

    @property
    def status(self) -> uint8:
        return self._arr[0].status

    @property
    def side(self) -> uint8:
        return self._arr[0].side

    @property
    def time_in_force(self) -> uint8:
        return self._arr[0].time_in_force
