from numba.experimental import jitclass
from numba.typed import List
from numba.types import ListType
from numba import float64, int64
import pandas as pd
import numpy as np


@jitclass([
    ('timestamp', ListType(int64)),
    ('mid', ListType(float64)),
    ('balance', ListType(float64)),
    ('position', ListType(float64)),
    ('fee', ListType(float64)),
    ('trade_qty', ListType(float64)),
    ('trade_amount', ListType(float64)),
])
class Recorder:
    def __init__(self, timestamp, mid, balance, position, fee, trade_qty, trade_amount):
        self.timestamp = timestamp
        self.mid = mid
        self.balance = balance
        self.position = position
        self.fee = fee
        self.trade_qty = trade_qty
        self.trade_amount = trade_amount

    def record(self, hbt):
        self.timestamp.append(hbt.local_timestamp)
        self.mid.append((hbt.best_bid + hbt.best_ask) / 2.0)
        self.balance.append(hbt.balance)
        self.position.append(hbt.position)
        self.fee.append(hbt.fee)
        self.trade_qty.append(hbt.trade_qty)
        self.trade_amount.append(hbt.trade_amount)


class Stat:
    def __init__(self, hbt, utc=True, unit='us', allocated=100000):
        self.hbt = hbt
        self.utc = utc
        self.unit = unit
        self.timestamp = List.empty_list(int64, allocated=allocated)
        self.mid = List.empty_list(float64, allocated=allocated)
        self.balance = List.empty_list(float64, allocated=allocated)
        self.position = List.empty_list(float64, allocated=allocated)
        self.fee = List.empty_list(float64, allocated=allocated)
        self.trade_qty = List.empty_list(float64, allocated=allocated)
        self.trade_amount = List.empty_list(float64, allocated=allocated)

    def __get_recorder(self):
        return Recorder(self.timestamp, self.mid, self.balance, self.position, self.fee, self.trade_qty, self.trade_amount)

    recorder = property(__get_recorder)

    def datetime(self):
        return pd.to_datetime(np.asarray(self.timestamp), utc=self.utc, unit=self.unit)

    def equity(self, resample=None, include_fee=True):
        if include_fee:
            equity = pd.Series(self.hbt.asset_type.equity(np.asarray(self.mid),
                                                          np.asarray(self.balance),
                                                          np.asarray(self.position),
                                                          np.asarray(self.fee)),
                               index=self.datetime())
        else:
            equity = pd.Series(self.hbt.asset_type.equity(np.asarray(self.mid),
                                                          np.asarray(self.balance),
                                                          np.asarray(self.position),
                                                          0),
                               index=self.datetime())
        if resample is None:
            return equity
        else:
            return equity.resample(resample).last()

    def sharpe(self, resample=None, include_fee=True):
        pnl = self.equity(resample, include_fee=include_fee).diff()
        return pnl.mean() / pnl.std()

    def sortino(self, resample=None, include_fee=True):
        pnl = self.equity(resample, include_fee=include_fee).diff()
        std = pnl[pnl < 0].std()
        return pnl.mean() / std

    def drawdown(self, resample=None, include_fee=True):
        equity = self.equity(resample, include_fee=include_fee)
        max_equity = equity.cummax()
        drawdown = equity - max_equity
        return drawdown

    def maxdrawdown(self, resample=None, include_fee=True):
        return abs(self.drawdown(resample, include_fee=include_fee).min())

    def annualised_return(self, denom=None, include_fee=True):
        equity = self.equity('1y', include_fee=include_fee)
        ret = pd.concat([pd.Series([0]), equity]).diff().mean()
        if denom is None:
            return ret
        else:
            return ret / denom

    def profit_factor(self, include_fee=True):
        return self.annualised_return(include_fee=include_fee) / self.maxdrawdown(include_fee=include_fee)
