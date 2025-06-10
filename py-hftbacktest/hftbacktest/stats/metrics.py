import warnings
from abc import ABC, abstractmethod
from typing import Mapping, Dict, Any

import polars as pl
import numpy as np
from .utils import get_total_days, get_num_samples_per_day


class Metric(ABC):
    """
    A base class for computing a strategy's performance metrics. Implementing a custom metric class derived from this
    base class enables the computation of the custom metric in the :class:`Stats` and displays the summary.
    """
    @abstractmethod
    def compute(self, df: pl.DataFrame, context: Dict[str, Any]) -> Mapping[str, Any]:
        """
        Args:
            df: Polars :class:`DataFrame <pl.DataFrame>` containing the strategy's state records.
            context: A dictionary of calculated metrics or other values.

        Returns:
            A dictionary where the key is the name of the metric and the value is the computed metric.
        """
        raise NotImplementedError


class Ret(Metric):
    """
    Return

    Parameters:
        name: Name of this metric. The default value is `Return`.
        book_size: If the book size, or capital allocation, is set, the metric is divided by the book size to express it
                   as a percentage ratio of the book size; otherwise, the metric is in raw units.
    """

    def __init__(self, name: str = None, book_size: float | None = None):
        self.name = name if name is not None else 'Return'
        self.book_size = book_size

    def compute(self, df: pl.DataFrame, context: Dict[str, Any]) -> Mapping[str, Any]:
        equity = (df['equity_wo_fee'] - df['fee']).drop_nans()
        pnl = equity[-1] - equity[0]

        if self.book_size is not None:
            pnl /= self.book_size

        return {self.name: pnl}


class AnnualRet(Ret):
    """
    Annualised return

    Parameters:
        name: Name of this metric. The default value is `AnnualReturn`.
        book_size: If the book size, or capital allocation, is set, the metric is divided by the book size to express it
                   as a percentage ratio of the book size; otherwise, the metric is in raw units.
        trading_days_per_year: The number of trading days per year to annualise. Commonly, 252 is used in trad-fi, so
                               the default value is 252 to match that scale. However, you can use 365 instead of 252 for
                               crypto markets, which run 24/7.
    """

    def __init__(self, name: str = None, book_size: float | None = None, trading_days_per_year: float = 252):
        super().__init__(
            name if name is not None else 'AnnualReturn',
            book_size
        )
        self.trading_days_per_year = trading_days_per_year

    def compute(self, df: pl.DataFrame, context: Dict[str, Any]) -> Mapping[str, Any]:
        pnl = super().compute(df, context)[self.name]
        pnl = pnl / get_total_days(df['timestamp']) * self.trading_days_per_year
        return {self.name: pnl}


class SR(Metric):
    """
    Sharpe Ratio without considering a benchmark.

    Parameters:
        name: Name of this metric. The default value is `SR`.
        trading_days_per_year: Trading days per year to annualise. Commonly, 252 is used in trad-fi, so the default
                               value is 252 to match that scale. However, you can use 365 instead of 252 for crypto
                               markets, which run 24/7. Additionally, be aware that to compute the daily Sharpe Ratio,
                               it also multiplies by `sqrt(the sample number per day)`, so the computed Sharpe Ratio is
                               affected by the sampling interval.
    """

    def __init__(self, name: str = None, trading_days_per_year: float = 252):
        self.name = name if name is not None else 'SR'
        self.trading_days_per_year = trading_days_per_year

    def compute(self, df: pl.DataFrame, context: Dict[str, Any]) -> Mapping[str, Any]:
        equity = df['equity_wo_fee'] - df['fee']

        pnl = equity.diff()
        c = get_num_samples_per_day(df['timestamp']) * self.trading_days_per_year

        with np.errstate(divide='ignore'):
            return {self.name: np.divide(pnl.drop_nans().mean(), pnl.drop_nans().std()) * np.sqrt(c)}


class Sortino(Metric):
    """
    Sortino Ratio without considering a benchmark.

    Parameters:
        name: Name of this metric. The default value is `Sortino`.
        trading_days_per_year: Trading days per year to annualise. Commonly, 252 is used in trad-fi, so the default
                               value is 252 to match that scale. However, you can use 365 instead of 252 for crypto
                               markets, which run 24/7. Additionally, be aware that to compute the daily Sharpe Ratio,
                               it also multiplies by `sqrt(the sample number per day)`, so the computed Sharpe Ratio is
                               affected by the sampling interval.
    """

    def __init__(self, name=None, trading_days_per_year: float = 252):
        self.name = name if name is not None else 'Sortino'
        self.trading_days_per_year = trading_days_per_year

    def compute(self, df: pl.DataFrame, context: Dict[str, Any]) -> Mapping[str, Any]:
        equity = df['equity_wo_fee'] - df['fee']

        pnl = equity.diff()
        c = get_num_samples_per_day(df['timestamp']) * self.trading_days_per_year

        dr = np.sqrt((np.minimum(0, pnl) ** 2).drop_nans().mean())
        with np.errstate(divide='ignore'):
            return {self.name: np.divide(pnl.drop_nans().mean(), dr) * np.sqrt(c)}


class ReturnOverMDD(Metric):
    """
    Return over Maximum Drawdown

    Parameters:
        name: Name of this metric. The default value is `ReturnOverMDD`.
    """

    def __init__(self, name: str = None):
        self.name = (
            name if name is not None else 'ReturnOverMDD'
        )

    def compute(self, df: pl.DataFrame, context: Dict[str, Any]) -> Mapping[str, Any]:
        ret = Ret().compute(df, context)['Return']
        mdd = MaxDrawdown().compute(df, context)['MaxDrawdown']
        return {self.name: np.divide(ret, mdd)}


class ReturnOverTrade(Metric):
    """
    Return over Trade value, which represents the profit made per unit of trading value, for instance,
    `$profit / $trading_value`.

    Parameters:
        name: Name of this metric. The default value is `ReturnOverTrade`.
    """

    def __init__(self, name: str = None):
        self.name = name if name is not None else 'ReturnOverTrade'

    def compute(self, df: pl.DataFrame, context: Dict[str, Any]) -> Mapping[str, Any]:
        ret = Ret().compute(df, context)['Return']
        trade_volume = TradingValue().compute(df, context)['TradingValue']
        return {self.name: np.divide(ret, trade_volume)}


class MaxDrawdown(Metric):
    """
    Maximum Drawdown

    Parameters:
        name: Name of this metric. The default value is `MaxDrawdown`.
        book_size: If the book size, or capital allocation, is set, the metric is divided by the book size to express it
                   as a percentage ratio of the book size; otherwise, the metric is in raw units.
    """

    def __init__(self, name: str = None, book_size: float | None = None):
        self.name = name if name is not None else 'MaxDrawdown'
        self.book_size = book_size

    def compute(self, df: pl.DataFrame, context: Dict[str, Any]) -> Mapping[str, Any]:
        equity = df['equity_wo_fee'] - df['fee']

        max_equity = equity.cum_max()
        dd = equity - max_equity

        if self.book_size is not None:
            dd /= self.book_size

        return {self.name: abs(dd.min())}


class NumberOfTrades(Metric):
    """
    Calculates the total number of trades.

    Parameters:
        name: Name of this metric. The default value is `NumberOfTrades`.
    """

    def __init__(self, name: str = None):
        self.name = name if name is not None else 'NumberOfTrades'

    def compute(self, df: pl.DataFrame, context: Dict[str, Any]) -> Mapping[str, Any]:
        num_trades = df['num_trades_'].sum()
        return {self.name: num_trades}


class DailyNumberOfTrades(NumberOfTrades):
    """
    Calculates the daily number of trades.
    
    Parameters:
        name: Name of this metric. The default value is `DailyNumberOfTrades`.
    """

    def __init__(self, name: str = None):
        super().__init__(name if name is not None else 'DailyNumberOfTrades')

    def compute(self, df: pl.DataFrame, context: Dict[str, Any]) -> Mapping[str, Any]:
        num_trades = super().compute(df, context)[self.name]
        num_trades /= get_total_days(df['timestamp'])
        return {self.name: num_trades}


class TradingVolume(Metric):
    """
    Calculates the total trading volume, defined as the total number of shares or contracts traded.

    Parameters:
        name: Name of this metric. The default value is `TradingVolume`.
    """

    def __init__(self, name: str = None):
        self.name = name if name is not None else 'TradingVolume'

    def compute(self, df: pl.DataFrame, context: Dict[str, Any]) -> Mapping[str, Any]:
        trading_volume = df['trading_volume_'].sum()
        return {self.name: trading_volume}


class DailyTradingVolume(TradingVolume):
    """
    Calculates the daily trading volume, defined as the daily number of shares or contracts traded.

    Parameters:
        name: Name of this metric. The default value is `DailyTradingVolume`.
    """

    def __init__(self, name: str = None):
        super().__init__(name if name is not None else 'DailyTradingVolume')

    def compute(self, df: pl.DataFrame, context: Dict[str, Any]) -> Mapping[str, Any]:
        trading_volume = super().compute(df, context)[self.name]
        trading_volume /= get_total_days(df['timestamp'])
        return {self.name: trading_volume}


class TradingValue(Metric):
    """
    Calculates total trading value, or total turnover defined as trading value divided by the book size.

    Parameters:
        name: Name of this metric. The default value is `TradingValue` or `Turnover` if book_size is provided.
        book_size: If the book size, or capital allocation, is set, the metric is divided by the book size to express it
                   as a percentage ratio of the book size; otherwise, the metric is in raw units.
    """

    def __init__(self, name: str = None, book_size: float | None = None):
        self.name = (
            name if name is not None else ('TradingValue' if book_size is None else 'Turnover')
        )
        self.book_size = book_size

    def compute(self, df: pl.DataFrame, context: Dict[str, Any]) -> Mapping[str, Any]:
        trading_value = df['trading_value_'].sum()
        if self.book_size is not None:
            trading_value /= self.book_size
        return {self.name: trading_value}


class DailyTradingValue(TradingValue):
    """
    Calculates daily trading value, or daily turnover defined as daily trading value divided by the book size.

    Parameters:
        name: Name of this metric. The default value is `DailyTradingValue` or `DailyTurnover` if book_size is provided.
        book_size: If the book size, or capital allocation, is set, the metric is divided by the book size to express it
                   as a percentage ratio of the book size; otherwise, the metric is in raw units.
    """

    def __init__(self, name: str = None, book_size: float | None = None):
        super().__init__(
            name if name is not None else ('DailyTradingValue' if book_size is None else 'DailyTurnover'),
            book_size
        )

    def compute(self, df: pl.DataFrame, context: Dict[str, Any]) -> Mapping[str, Any]:
        trading_value = super().compute(df, context)[self.name]
        trading_value /= get_total_days(df['timestamp'])
        return {self.name: trading_value}


class MaxPositionValue(Metric):
    """
    Calculates the maximum open position value.

    Parameters:
        name: Name of this metric. The default value is `MaxPositionValue`.
    """

    def __init__(self, name: str = None):
        self.name = name if name is not None else 'MaxPositionValue'

    def compute(self, df: pl.DataFrame, context: Dict[str, Any]) -> Mapping[str, Any]:
        return {self.name: (df['position'].abs() * df['price']).max()}


class MeanPositionValue(Metric):
    """
    Calculates the average open position value.

    Parameters:
        name: Name of this metric. The default value is `MeanPositionValue`.
    """

    def __init__(self, name: str = None):
        self.name = name if name is not None else 'MeanPositionValue'

    def compute(self, df: pl.DataFrame, context: Dict[str, Any]) -> Mapping[str, Any]:
        return {self.name: (df['position'].abs() * df['price']).mean()}


class MedianPositionValue(Metric):
    """
    Calculates the median open position value.

    Parameters:
        name: Name of this metric. The default value is `MedianPositionValue`.
    """

    def __init__(self, name: str = None):
        self.name = name if name is not None else 'MedianPositionValue'

    def compute(self, df: pl.DataFrame, context: Dict[str, Any]) -> Mapping[str, Any]:
        return {self.name: (df['position'].abs() * df['price']).median()}


class MaxLeverage(Metric):
    """
    Calculates the maximum leverage, defined as the maximum open position value divided by the capital.

    Parameters:
        name: Name of this metric. The default value is `MaxLeverage`.
        book_size: Capital allocation.
    """

    def __init__(self, name: str = None, book_size: float = 0.0):
        if book_size <= 0.0:
            warnings.warn('book_size should be positive.', UserWarning)
        self.name = name if name is not None else 'MaxLeverage'
        self.capital = book_size

    def compute(self, df: pl.DataFrame, context: Dict[str, Any]) -> Mapping[str, Any]:
        return {self.name: (df['position'].abs() * df['price']).max() / self.capital}
