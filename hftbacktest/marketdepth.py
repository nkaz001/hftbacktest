import sys
from order_book import SortedDict
from .reader import COL_PRICE, COL_QTY, COL_SIDE
from .order import BUY, SELL

INVALID_MIN = -sys.maxsize
INVALID_MAX = sys.maxsize


class MarketDepth:
    def __init__(self, tick_size, lot_size):
        self.tick_size = tick_size
        self.lot_size = lot_size
        self.ask_depth = SortedDict(ordering='ASC')
        self.bid_depth = SortedDict(ordering='DESC')        

    def apply_snapshot(self, data):
        for row in data:
            price_tick = round(row[COL_PRICE] / self.tick_size)
            qty = row[COL_QTY]
            if row[COL_SIDE] == BUY:
                self.bid_depth[price_tick] = qty
            elif row[COL_SIDE] == SELL:
                self.ask_depth[price_tick] = qty    

    @property
    def best_bid_tick(self):
        return self.get_bid_tick()    
    
    @property
    def best_ask_tick(self):
        return self.get_ask_tick()    
    
    @property
    def low_bid_tick(self):
        return self.get_bid_tick(-1)    

    @property
    def high_ask_tick(self):
        return self.get_ask_tick(-1)    

    def get_bid_tick(self, idx=0):
        if len(self.bid_depth) > 0:
            return self.bid_depth.index(idx)
        return INVALID_MAX if idx==0 else INVALID_MIN
    
    def get_ask_tick(self, idx=0):
        if len(self.ask_depth) > 0:
            return self.ask_depth.index(idx)
        return INVALID_MIN if idx==0 else INVALID_MAX

    def clear_depth(
            self,
            side,
            clear_upto_price,
    ):
        clear_upto = round(clear_upto_price / self.tick_size)
        if side == BUY:
            if len(self.bid_depth) > 0:
                for t in range(self.best_bid_tick, clear_upto - 1, -1):
                    if t in self.bid_depth:
                        del self.bid_depth[t]
        elif side == SELL:
            if len(self.ask_depth) > 0:
                for t in range(self.best_ask_tick, clear_upto + 1):
                    if t in self.ask_depth:
                        del self.ask_depth[t]
        else:
            self.ask_depth = SortedDict(ordering='ASC')
            self.bid_depth = SortedDict(ordering='DESC')  

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

        else:
            if price_tick > self.best_bid_tick:
                if callback is not None:
                    callback.on_best_bid_update(self.best_bid_tick, price_tick, timestamp)

                self.best_bid_tick = price_tick
                if self.best_bid_tick >= self.best_ask_tick:
                    self.clear_depth(SELL, self.best_bid_tick * self.tick_size) 

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

        else:
            if price_tick < self.best_ask_tick:
                if callback is not None:
                    callback.on_best_ask_update(self.best_ask_tick, price_tick, timestamp)

                self.best_ask_tick = price_tick
                if self.best_ask_tick <= self.best_bid_tick:
                    self.clear_depth(BUY, self.best_ask_tick * self.tick_size)
