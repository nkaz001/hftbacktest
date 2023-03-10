import sys

from numba import njit, float64, int64
from numba.experimental import jitclass
from numba.typed import Dict
from numba.types import DictType

from .reader import COL_PRICE, COL_QTY, COL_SIDE
from .order import BUY, SELL

INVALID_MIN = -sys.maxsize
INVALID_MAX = sys.maxsize


@njit
def depth_below(depth, start, end):
    for t in range(start - 1, end - 1, -1):
        if t in depth and depth[t] > 0:
            return t
    return INVALID_MIN


@njit
def depth_above(depth, start, end):
    for t in range(start + 1, end + 1):
        if t in depth and depth[t] > 0:
            return t
    return INVALID_MAX


@jitclass
class MarketDepth:
    tick_size: float64
    lot_size: float64
    ask_depth: DictType(int64, float64)
    bid_depth: DictType(int64, float64)
    best_bid_tick: int64
    best_ask_tick: int64
    low_bid_tick: int64
    high_ask_tick: int64

    def __init__(self, tick_size, lot_size):
        self.tick_size = tick_size
        self.lot_size = lot_size
        self.ask_depth = Dict.empty(int64, float64)
        self.bid_depth = Dict.empty(int64, float64)
        self.best_bid_tick = INVALID_MIN
        self.best_ask_tick = INVALID_MAX
        self.low_bid_tick = INVALID_MAX
        self.high_ask_tick = INVALID_MIN

    def apply_snapshot(self, data):
        self.best_bid_tick = INVALID_MIN
        self.best_ask_tick = INVALID_MAX
        self.low_bid_tick = INVALID_MAX
        self.high_ask_tick = INVALID_MIN
        self.bid_depth.clear()
        self.ask_depth.clear()
        for row in data:
            price_tick = round(row[COL_PRICE] / self.tick_size)
            qty = row[COL_QTY]
            if row[COL_SIDE] == BUY:
                if price_tick > self.best_bid_tick:
                    self.best_bid_tick = price_tick
                if price_tick < self.low_bid_tick:
                    self.low_bid_tick = price_tick
                self.bid_depth[price_tick] = qty
            elif row[COL_SIDE] == SELL:
                if price_tick < self.best_ask_tick:
                    self.best_ask_tick = price_tick
                if price_tick > self.high_ask_tick:
                    self.high_ask_tick = price_tick
                self.ask_depth[price_tick] = qty

    def clear_depth(
            self,
            side,
            clear_upto_price,
    ):
        clear_upto = round(clear_upto_price / self.tick_size)
        if side == BUY:
            if self.best_bid_tick != INVALID_MIN:
                for t in range(self.best_bid_tick, clear_upto - 1, -1):
                    if t in self.bid_depth:
                        del self.bid_depth[t]
                self.best_bid_tick = depth_below(self.bid_depth, clear_upto - 1, self.low_bid_tick)
                if self.best_bid_tick == INVALID_MIN:
                    self.low_bid_tick = INVALID_MAX
        elif side == SELL:
            if self.best_ask_tick != INVALID_MAX:
                for t in range(self.best_ask_tick, clear_upto + 1):
                    if t in self.ask_depth:
                        del self.ask_depth[t]
                self.best_ask_tick = depth_above(self.ask_depth, clear_upto + 1, self.high_ask_tick)
                if self.best_ask_tick == INVALID_MAX:
                    self.high_ask_tick = INVALID_MIN
        else:
            self.bid_depth.clear()
            self.ask_depth.clear()
            self.best_bid_tick = INVALID_MIN
            self.best_ask_tick = INVALID_MAX
            self.low_bid_tick = INVALID_MAX
            self.high_ask_tick = INVALID_MIN

    def update_bid_depth(
            self,
            price,
            qty,
            timestamp,
            callback=None
    ):
        price_tick = round(price / self.tick_size)
        prev_qty = self.bid_depth.get(price_tick, 0)
        self.bid_depth[price_tick] = qty

        if callback is not None:
            callback.on_bid_qty_chg(price_tick, prev_qty, qty, timestamp)

        if round(qty / self.lot_size) == 0:
            del self.bid_depth[price_tick]
            if price_tick == self.best_bid_tick:
                self.best_bid_tick = depth_below(self.bid_depth, self.best_bid_tick, self.low_bid_tick)
                if self.best_bid_tick == INVALID_MIN:
                    self.low_bid_tick = INVALID_MAX
        else:
            if price_tick > self.best_bid_tick:
                if callback is not None:
                    callback.on_best_bid_update(self.best_bid_tick, price_tick, timestamp)

                self.best_bid_tick = price_tick
                if self.best_bid_tick >= self.best_ask_tick:
                    self.best_ask_tick = depth_above(self.ask_depth, self.best_bid_tick, self.high_ask_tick)
            if price_tick < self.low_bid_tick:
                self.low_bid_tick = price_tick

    def update_ask_depth(
            self,
            price,
            qty,
            timestamp,
            callback=None
    ):
        price_tick = round(price / self.tick_size)
        prev_qty = self.ask_depth.get(price_tick, 0)
        self.ask_depth[price_tick] = qty

        if callback is not None:
            callback.on_ask_qty_chg(price_tick, prev_qty, qty, timestamp)

        if round(qty / self.lot_size) == 0:
            del self.ask_depth[price_tick]
            if price_tick == self.best_ask_tick:
                self.best_ask_tick = depth_above(self.ask_depth, self.best_ask_tick, self.high_ask_tick)
                if self.best_ask_tick == INVALID_MAX:
                    self.high_ask_tick = INVALID_MIN
        else:
            if price_tick < self.best_ask_tick:
                if callback is not None:
                    callback.on_best_ask_update(self.best_ask_tick, price_tick, timestamp)

                self.best_ask_tick = price_tick
                if self.best_ask_tick <= self.best_bid_tick:
                    self.best_bid_tick = depth_below(self.bid_depth, self.best_ask_tick, self.low_bid_tick)
            if price_tick > self.high_ask_tick:
                self.high_ask_tick = price_tick
