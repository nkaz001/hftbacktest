from typing import Literal, Optional

from numba.experimental import jitclass
from numba.typed import List
from numba.types import ListType
from numba import float64, int64
from matplotlib import pyplot as plt
import pandas as pd
import numpy as np

from .typing import HftBacktestType


class Recorder:
    timestamp: ListType(int64)
    mid: ListType(float64)
    balance: ListType(float64)
    position: ListType(float64)
    fee: ListType(float64)
    trade_num: ListType(int64)
    trade_qty: ListType(float64)
    trade_amount: ListType(float64)

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
        """
        Records the current stats.

        Args:
            hbt: An instance of the HftBacktest class.
        """
        self.timestamp.append(hbt.current_timestamp)
        self.mid.append((hbt.best_bid + hbt.best_ask) / 2.0)
        self.balance.append(hbt.balance)
        self.position.append(hbt.position)
        self.fee.append(hbt.fee)
        self.trade_num.append(hbt.trade_num)
        self.trade_qty.append(hbt.trade_qty)
        self.trade_amount.append(hbt.trade_amount)


class Stat:
    r"""
    Calculates performance statistics and generates a summary of performance metrics.

    Args:
        hbt: An instance of the HftBacktest class.
        utc: If ``True``, timestamps are in UTC.
        unit: The unit of the timestamp.
        allocated: The preallocated size of recorded time series.
    """

    def __init__(
            self,
            hbt: HftBacktestType,
            utc: bool = True,
            unit: Literal['s', 'ms', 'us', 'ns'] = 'us',
            allocated: int = 100_000
    ):
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

    @property
    def recorder(self):
        r"""
        Returns a ``Recorder`` instance to record performance statistics.
        """
        return jitclass()(Recorder)(
            self.timestamp,
            self.mid,
            self.balance,
            self.position,
            self.fee,
            self.trade_num,
            self.trade_qty,
            self.trade_amount
        )

    def datetime(self):
        r"""
        Converts and returns a DateTime series from the timestamp.

        Returns:
            DateTime series by converting from the timestamp.
        """
        return pd.to_datetime(np.asarray(self.timestamp), utc=self.utc, unit=self.unit)

    def equity(self, resample: Optional[str] = None, include_fee: bool = True, datetime: bool = True):
        r"""
        Calculates equity values.

        Args:
            resample: If provided, equity values will be resampled based on the specified period.
            include_fee: If set to ``True``, fees will be included in the calculation; otherwise, fees will be excluded.
            datetime: If set to ``True``, the timestamp is converted to a DateTime, which takes a long time. If you want
                      fast computation, set it to ``False``.

        Returns:
            the calculated equity values.
        """
        if include_fee:
            equity = pd.Series(
                self.hbt.local.state.asset_type.equity(
                    np.asarray(self.mid),
                    np.asarray(self.balance),
                    np.asarray(self.position),
                    np.asarray(self.fee)
                ),
                index=self.datetime() if datetime else np.asarray(self.timestamp)
            )
        else:
            equity = pd.Series(
                self.hbt.local.state.asset_type.equity(
                    np.asarray(self.mid),
                    np.asarray(self.balance),
                    np.asarray(self.position),
                    0
                ),
                index=self.datetime() if datetime else np.asarray(self.timestamp)
            )
        if resample is None:
            return equity
        else:
            return equity.resample(resample).last()

    def sharpe(self, resample: str, include_fee: bool = True, trading_days: int = 365):
        r"""
        Calculates the Sharpe Ratio without considering benchmark rates.

        Args:
            resample: The resampling period, such as '1s', '5min'.
            include_fee: If set to ``True``, fees will be included in the calculation; otherwise, fees will be excluded.
            trading_days: The number of trading days per year used for annualisation.

        Returns:
            The calculated Sharpe Ratio.
        """
        pnl = self.equity(resample, include_fee=include_fee).diff()
        c = (24 * 60 * 60 * 1e9) / (pnl.index[1] - pnl.index[0]).value
        std = pnl.std()
        return np.divide(pnl.mean(), std) * np.sqrt(c * trading_days)

    def sortino(self, resample: str, include_fee: bool = True, trading_days: int = 365):
        r"""
        Calculates Sortino Ratio.

        Args:
            resample: The resampling period, such as '1s', '5min'.
            include_fee: If set to ``True``, fees will be included in the calculation; otherwise, fees will be excluded.
            trading_days: The number of trading days per year used for annualisation.
        Returns:
            Sortino Ratio
        """
        pnl = self.equity(resample, include_fee=include_fee).diff()
        std = pnl[pnl < 0].std()
        c = (24 * 60 * 60 * 1e9) / (pnl.index[1] - pnl.index[0]).value
        return np.divide(pnl.mean(), std) * np.sqrt(c * trading_days)

    def riskreturnratio(self, include_fee: bool = True):
        r"""
        Calculates Risk-Return Ratio, which is Annualized Return / Maximum Draw Down over the entire period.

        Args:
            include_fee: If set to ``True``, fees will be included in the calculation; otherwise, fees will be excluded.

        Returns:
            Risk-Return Ratio
        """
        return self.annualised_return(include_fee=include_fee) / self.maxdrawdown(include_fee=include_fee)

    def drawdown(self, resample: Optional[str] = None, include_fee: bool = True):
        r"""
        Retrieves Draw Down time-series.

        Args:
            resample: The resampling period, such as '1s', '5min'.
            include_fee: If set to ``True``, fees will be included in the calculation; otherwise, fees will be excluded.

        Returns:
            Draw down time-series.
        """
        equity = self.equity(resample, include_fee=include_fee)
        max_equity = equity.cummax()
        drawdown = equity - max_equity
        return drawdown

    def maxdrawdown(self, denom: Optional[float] = None, include_fee: bool = True):
        r"""
        Retrieves Maximum Draw Down.

        Args:
            denom: If provided, MDD will be calculated in percentage terms by dividing by the specified denominator.
            include_fee: If set to ``True``, fees will be included in the calculation; otherwise, fees will be excluded.

        Returns:
            Maximum Draw Down.
        """
        mdd = -self.drawdown(None, include_fee=include_fee).min()
        if denom is None:
            return mdd
        else:
            return mdd / denom

    def daily_trade_num(self):
        r"""
        Retrieves the average number of daily trades.

        Returns:
            Average number of daily trades.
        """
        return pd.Series(self.trade_num, index=self.datetime()).diff().rolling('1d').sum().mean()

    def daily_trade_volume(self):
        r"""
        Retrieves the average quantity of daily trades.

        Returns:
            Average quantity of daily trades.
        """
        return pd.Series(self.trade_qty, index=self.datetime()).diff().rolling('1d').sum().mean()

    def daily_trade_amount(self):
        r"""
        Retrieves the average value of daily trades.

        Returns:
            Average value of daily trades.
        """
        return pd.Series(self.trade_amount, index=self.datetime()).diff().rolling('1d').sum().mean()

    def annualised_return(self, denom: Optional[float] = None, include_fee: bool = True, trading_days: int = 365):
        r"""
        Calculates annualised return.

        Args:
            denom: If provided, annualised return will be calculated in percentage terms by dividing by the specified
                   denominator.
            include_fee: If set to ``True``, fees will be included in the calculation; otherwise, fees will be excluded.
            trading_days: The number of trading days per year used for annualisation.

        Returns:
            Annaulised return.
        """
        equity = self.equity(None, include_fee=include_fee)
        c = (24 * 60 * 60 * 1e9) / (equity.index[-1] - equity.index[0]).value
        if denom is None:
            return equity[-1] * c * trading_days
        else:
            return equity[-1] * c * trading_days / denom

    def summary(self, capital: Optional[float] = None, resample: str = '5min', trading_days: int = 365):
        r"""
        Generates a summary of performance metrics.

        Args:
            capital: The initial capital investment for the strategy. If provided, it is used as the denominator
                     to calculate annualized return and MDD in percentage terms. Otherwise, absolute values are
                     displayed.
            resample: The resampling period, such as '1s', '5min'.
            trading_days: The number of trading days per year used for annualisation.
        """
        dt_index = self.datetime()
        raw_equity = self.hbt.local.state.asset_type.equity(
            np.asarray(self.mid),
            np.asarray(self.balance),
            np.asarray(self.position),
            np.asarray(self.fee)
        )
        raw_equity_wo_fee = self.hbt.local.state.asset_type.equity(
            np.asarray(self.mid),
            np.asarray(self.balance),
            np.asarray(self.position),
            0
        )
        equity = pd.Series(raw_equity, index=dt_index)
        rs_equity_wo_fee = pd.Series(raw_equity_wo_fee, index=dt_index).resample(resample).last()
        rs_equity = equity.resample(resample).last()
        rs_pnl = rs_equity.diff()

        c = (24 * 60 * 60 * 1e9) / (rs_pnl.index[1] - rs_pnl.index[0]).value
        sr = np.divide(rs_pnl.mean(), rs_pnl.std()) * np.sqrt(c * trading_days)

        std = rs_pnl[rs_pnl < 0].std()
        sortino = np.divide(rs_pnl.mean(), std) * np.sqrt(c * trading_days)

        max_equity = rs_equity.cummax()
        drawdown = rs_equity - max_equity
        mdd = -drawdown.min()

        ac = (24 * 60 * 60 * 1e9) / (equity.index[-1] - equity.index[0]).value
        ar = raw_equity[-1] * ac * trading_days
        rrr = ar / mdd

        dtn = pd.Series(self.trade_num, index=dt_index).diff().rolling('1d').sum().mean()
        dtq = pd.Series(self.trade_qty, index=dt_index).diff().rolling('1d').sum().mean()
        dta = pd.Series(self.trade_amount, index=dt_index).diff().rolling('1d').sum().mean()

        print('=========== Summary ===========')
        print('Sharpe ratio: %.1f' % sr)
        print('Sortino ratio: %.1f' % sortino)
        print('Risk return ratio: %.1f' % rrr)
        if capital is not None:
            print('Annualised return: %.2f %%' % (ar / capital * 100))
            print('Max. draw down: %.2f %%' % (mdd / capital * 100))
        else:
            print('Annualised return: %.2f' % ar)
            print('Max. draw down: %.2f' % mdd)
        print('The number of trades per day: %d' % dtn)
        print('Avg. daily trading volume: %d' % dtq)
        print('Avg. daily trading amount: %d' % dta)

        position = np.asarray(self.position) * np.asarray(self.mid)
        if capital is not None:
            print('Max leverage: %.2f' % (np.max(np.abs(position)) / capital))
            print('Median leverage: %.2f' % (np.median(np.abs(position)) / capital))

        fig, axs = plt.subplots(2, 1, sharex=True)
        fig.subplots_adjust(hspace=0)
        fig.set_size_inches(10, 6)

        mid = pd.Series(self.mid, index=dt_index)

        if capital is not None:
            ((mid / mid[0] - 1).resample(resample).last() * 100).plot(ax=axs[0], style='grey', alpha=0.5)
            (rs_equity / capital * 100).plot(ax=axs[0])
            (rs_equity_wo_fee / capital * 100).plot(ax=axs[0])
            axs[0].set_ylabel('Cumulative Returns (%)')
        else:
            mid.resample(resample).last().plot(ax=axs[0], style='grey', alpha=0.5)
            (rs_equity * 100).plot(ax=axs[0])
            (rs_equity_wo_fee * 100).plot(ax=axs[0])
            axs[0].set_ylabel('Cumulative Returns')

        # axs[0].set_title('Equity')
        axs[0].grid()
        axs[0].legend(['Trading asset', 'Strategy incl. fee', 'Strategy excl. fee'])

        # todo: this can mislead a user due to aggregation.
        position = pd.Series(self.position, index=dt_index).resample(resample).last()
        position.plot(ax=axs[1])
        # ax3 = ax2.twinx()
        # (position * mid).plot(ax=ax3, style='grey', alpha=0.2)
        # axs[1].set_title('Position')
        axs[1].set_ylabel('Position (Qty)')
        # ax3.set_ylabel('Value')
        axs[1].grid()
