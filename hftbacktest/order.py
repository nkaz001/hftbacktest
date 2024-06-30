import numpy as np
from numba.experimental import jitclass

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


@jitclass
class Order:
    _arr: np.ndarray

    def __init__(self, arr: np.ndarray):
        self._arr = arr

    @property
    def price(self):
        return self._arr.price_tick * self._arr.tick_size

    @property
    def exec_price(self):
        return self._arr.exec_price_tick * self._arr.tick_size

    @property
    def cancellable(self):
        return (self._arr.tatus == NEW or self._arr.status == PARTIALLY_FILLED) and self._arr.req == NONE
