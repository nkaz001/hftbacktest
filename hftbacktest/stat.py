from numba.experimental import jitclass
from numba.typed import List
from numba.types import ListType
from numba import float64, int64
from matplotlib import pyplot
import pandas as pd
import numpy as np


@jitclass([
    ('timestamp', ListType(int64)),
    ('mid', ListType(float64)),
    ('balance', ListType(float64)),
    ('position', ListType(float64)),
    ('fee', ListType(float64)),
    ('trade_num', ListType(int64)),
    ('trade_qty', ListType(float64)),
    ('trade_amount', ListType(float64)),
])
class Recorder:
    def __init__(self, timestamp, mid, balance, position, fee, trade_num, trade_qty, trade_amount):
        self.timestamp = timestamp
        self.mid = mid
        self.balance = balance
        self.position = position
        self.fee = fee
        self.trade_num = trade_num
        self.trade_qty = trade_qty
        self.trade_amount = trade_amount

    def record(self, hbt):
        self.timestamp.append(hbt.local_timestamp)
        self.mid.append((hbt.best_bid + hbt.best_ask) / 2.0)
        self.balance.append(hbt.balance)
        self.position.append(hbt.position)
        self.fee.append(hbt.fee)
        self.trade_num.append(hbt.trade_num)
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
        self.trade_num = List.empty_list(int64, allocated=allocated)
        self.trade_qty = List.empty_list(float64, allocated=allocated)
        self.trade_amount = List.empty_list(float64, allocated=allocated)

    def __get_recorder(self):
        return Recorder(self.timestamp, self.mid, self.balance, self.position, self.fee,
                        self.trade_num, self.trade_qty, self.trade_amount)

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

    def sharpe(self, resample, include_fee=True, trading_days=365):
        pnl = self.equity(resample, include_fee=include_fee).diff()
        c = (24 * 60 * 60 * 1e9) / (pnl.index[1] - pnl.index[0]).value
        return pnl.mean() / pnl.std() * np.sqrt(c * trading_days)

    def sortino(self, resample, include_fee=True, trading_days=365):
        pnl = self.equity(resample, include_fee=include_fee).diff()
        std = pnl[pnl < 0].std()
        c = (24 * 60 * 60 * 1e9) / (pnl.index[1] - pnl.index[0]).value
        return pnl.mean() / std * np.sqrt(c * trading_days)

    def riskreturnratio(self, include_fee=True):
        return self.annualised_return(include_fee=include_fee) / self.maxdrawdown(include_fee=include_fee)

    def drawdown(self, resample=None, include_fee=True):
        equity = self.equity(resample, include_fee=include_fee)
        max_equity = equity.cummax()
        drawdown = equity - max_equity
        return drawdown

    def maxdrawdown(self, denom=None, include_fee=True):
        mdd = np.abs(self.drawdown(None, include_fee=include_fee).min())
        if denom is None:
            return mdd
        else:
            return mdd / denom

    def daily_trade_num(self):
        return pd.Series(self.trade_num, index=self.datetime()).diff().rolling('1d').sum().mean()

    def daily_trade_volume(self):
        return pd.Series(self.trade_qty, index=self.datetime()).diff().rolling('1d').sum().mean()

    def daily_trade_amount(self):
        return pd.Series(self.trade_amount, index=self.datetime()).diff().rolling('1d').sum().mean()

    def annualised_return(self, denom=None, include_fee=True):
        equity = self.equity('1y', include_fee=include_fee)
        ret = pd.concat([pd.Series([0]), equity]).diff().mean()
        if denom is None:
            return ret
        else:
            return ret / denom

    def summary(self, capital, resample='1h', trading_days=365):
        print('=========== Summary ===========')
        print('Sharpe ratio: %.1f' % (self.sharpe(resample, trading_days=trading_days)))
        print('Sortino ratio: %.1f' % (self.sortino(resample, trading_days=trading_days)))
        print('Risk return ratio: %.1f' % self.riskreturnratio())
        print('Annualised return: %.2f %%' % (self.annualised_return(capital) * 100))
        print('Max Drawdown: %.2f %%' % (self.maxdrawdown() / capital * 100))
        print('Average daily trading number: %d' % self.daily_trade_num())
        print('Average daily trading volume: %d' % self.daily_trade_volume())
        print('Average daily trading amount: %d' % self.daily_trade_amount())
        position = np.asarray(self.position) * np.asarray(self.mid)
        print('Leverage: %.2f (Max), %.2f (Avg)' % (np.abs(np.max(position) / capital),
                                                    np.abs(np.mean(position) / capital)))
        self.equity('5min').plot()
        self.equity('5min', include_fee=False).plot()
        pyplot.figure(1)
        pyplot.plot(self.position)
        pyplot.plot(self.mid)
