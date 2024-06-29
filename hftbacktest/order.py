import numpy as np
from numba import float64, int64, int8, boolean, uint64
from numba.experimental import jitclass


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
    _ptr: uint64

    def __init__(self, ptr):
        self._ptr = ptr

    # @property
    # def price(self):
    #     return self.price_tick * self.tick_size
    #
    # @property
    # def exec_price(self):
    #     return self.exec_price_tick * self.tick_size
    #
    # @property
    # def cancellable(self):
    #     return (self.status == NEW or self.status == PARTIALLY_FILLED) and self.req == NONE


@jitclass
class OrderHashMap:
    _ptr: uint64

    def __init__(self, ptr: uint64):
        self._ptr = ptr

    def keys(self):
        pass

    def values(self):
        pass

    def get(self, order_id):
        pass
