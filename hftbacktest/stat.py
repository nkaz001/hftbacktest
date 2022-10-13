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

    def annualised_return(self, denom=None, include_fee=True, trading_days=365):
        equity = self.equity(None, include_fee=include_fee)
        c = (24 * 60 * 60 * 1e9) / (equity.index[-1] - equity.index[0]).value
        if denom is None:
            return equity[-1] * c * trading_days
        else:
            return equity[-1] * c * trading_days / denom

    def summary(self, capital, resample='5min', trading_days=365):
        print('=========== Summary ===========')
        print('Sharpe ratio: %.1f' % (self.sharpe(resample, trading_days=trading_days)))
        print('Sortino ratio: %.1f' % (self.sortino(resample, trading_days=trading_days)))
        print('Risk return ratio: %.1f' % self.riskreturnratio())
        print('Annualised return: %.2f %%' % (self.annualised_return(capital) * 100))
        print('Max. draw down: %.2f %%' % (self.maxdrawdown() / capital * 100))
        print('The number of trades per day: %d' % self.daily_trade_num())
        print('Avg. daily trading volume: %d' % self.daily_trade_volume())
        print('Avg. daily trading amount: %d' % self.daily_trade_amount())
        position = np.asarray(self.position) * np.asarray(self.mid)
        print('Max leverage: %.2f' % (np.max(np.abs(position)) / capital))
        print('Median leverage: %.2f' % (np.median(np.abs(position)) / capital))

        pyplot.figure(0)
        mid = pd.Series(self.mid, index=self.datetime())

        ax1 = ((mid / mid[0] - 1).resample(resample).last() * 100).plot(style='grey', alpha=0.5)
        (self.equity(resample) / capital * 100).plot()
        (self.equity(resample, include_fee=False) / capital * 100).plot()
        ax1.set_title('Equity')
        ax1.set_ylabel('Cumulative Returns (%)')
        ax1.grid()
        ax1.legend(['Trading asset', 'Strategy incl. fee', 'Strategy excl. fee'])

        pyplot.figure(1)
        position = pd.Series(self.position, index=self.datetime())
        ax2 = position.plot()
        ax3 = ax2.twinx()
        (position * mid).plot(ax=ax3, style='grey', alpha=0.2)
        ax2.set_title('Position')
        ax2.set_ylabel('Qty')
        ax3.set_ylabel('Value')
        ax2.grid()
