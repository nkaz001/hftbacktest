import warnings
from abc import ABC, abstractmethod
from typing import Mapping, Dict

import polars as pl
import numpy as np
from .utils import get_total_days, get_num_samples_per_day


class Metric(ABC):
    @abstractmethod
    def compute(self, df: pl.DataFrame, context: Dict[str, float]) -> Mapping[str, float]:
        raise NotImplementedError


class Ret(Metric):
    def __init__(self, name: str = None, book_size: float | None = None):
        self.name = name if name is not None else 'Return'
        self.book_size = book_size

    def compute(self, df: pl.DataFrame, context: Dict[str, float]) -> Mapping[str, float]:
        equity = df['equity_wo_fee'] - df['fee']
        pnl = equity[-1] - equity[0]

        if self.book_size is not None:
            pnl /= self.book_size

        return {self.name: pnl}


class AnnualRet(Ret):
    def __init__(self, name: str = None, book_size: float | None = None, trading_days_per_year: float = 365):
        super().__init__(
            name if name is not None else 'AnnualReturn',
            book_size
        )
        self.trading_days_per_year = trading_days_per_year

    def compute(self, df: pl.DataFrame, context: Dict[str, float]) -> Mapping[str, float]:
        pnl = super().compute(df, context)[self.name]
        pnl = pnl / get_total_days(df['timestamp']) * self.trading_days_per_year
        return {self.name: pnl}


class SR(Metric):
    def __init__(self, name: str = None, trading_days_per_year: float = 365):
        self.name = name if name is not None else 'SR'
        self.trading_days_per_year = trading_days_per_year

    def compute(self, df: pl.DataFrame, context: Dict[str, float]) -> Mapping[str, float]:
        equity = df['equity_wo_fee'] - df['fee']

        pnl = equity.diff()
        c = get_num_samples_per_day(df['timestamp']) * self.trading_days_per_year

        with np.errstate(divide='ignore'):
            return {self.name: np.divide(pnl.mean(), pnl.std()) * np.sqrt(c)}


class Sortino(Metric):
    def __init__(self, name=None, trading_days_per_year: float = 365):
        self.name = name if name is not None else 'Sortino'
        self.trading_days_per_year = trading_days_per_year

    def compute(self, df: pl.DataFrame, context: Dict[str, float]) -> Mapping[str, float]:
        equity = df['equity_wo_fee'] - df['fee']

        pnl = equity.diff()
        c = get_num_samples_per_day(df['timestamp']) * self.trading_days_per_year

        dr = np.sqrt((np.minimum(0, pnl) ** 2).mean())
        with np.errstate(divide='ignore'):
            return {self.name: np.divide(pnl.mean(), dr) * np.sqrt(c)}


class ReturnOverMDD(Metric):
    def __init__(self, name: str = None):
        self.name = (
            name if name is not None else 'ReturnOverMDD'
        )

    def compute(self, df: pl.DataFrame, context: Dict[str, float]) -> Mapping[str, float]:
        ret = Ret().compute(df, context)['Return']
        mdd = MaxDrawdown().compute(df, context)['MaxDrawdown']
        return {self.name: ret / mdd}


class ReturnOverTrade(Metric):
    def __init__(self, name: str = None):
        self.name = name if name is not None else 'ReturnOverTrade'

    def compute(self, df: pl.DataFrame, context: Dict[str, float]) -> Mapping[str, float]:
        ret = Ret().compute(df, context)['Return']
        trade_volume = TradingValue().compute(df, context)['TradingValue']
        return {self.name: ret / trade_volume}


class MaxDrawdown(Metric):
    def __init__(self, name: str = None, book_size: float | None = None):
        self.name = name if name is not None else 'MaxDrawdown'
        self.book_size = book_size

    def compute(self, df: pl.DataFrame, context: Dict[str, float]) -> Mapping[str, float]:
        equity = df['equity_wo_fee'] - df['fee']

        max_equity = equity.cum_max()
        dd = equity - max_equity

        if self.book_size is not None:
            dd /= self.book_size

        return {self.name: abs(dd.min())}


class NumberOfTrades(Metric):
    def __init__(self, name: str = None):
        self.name = name if name is not None else 'NumberOfTrades'

    def compute(self, df: pl.DataFrame, context: Dict[str, float]) -> Mapping[str, float]:
        num_trades = df['num_trades'].sum()
        return {self.name: num_trades}


class DailyNumberOfTrades(NumberOfTrades):
    def __init__(self, name: str = None):
        super().__init__(name if name is not None else 'DailyNumberOfTrades')

    def compute(self, df: pl.DataFrame, context: Dict[str, float]) -> Mapping[str, float]:
        num_trades = super().compute(df, context)[self.name]
        num_trades /= get_total_days(df['timestamp'])
        return {self.name: num_trades}


class TradingVolume(Metric):
    def __init__(self, name: str = None):
        self.name = name if name is not None else 'TradingVolume'

    def compute(self, df: pl.DataFrame, context: Dict[str, float]) -> Mapping[str, float]:
        trading_volume = df['trading_volume'].sum()
        return {self.name: trading_volume}


class DailyTradingVolume(TradingVolume):
    def __init__(self, name: str = None):
        super().__init__(name if name is not None else 'DailyTradingVolume')

    def compute(self, df: pl.DataFrame, context: Dict[str, float]) -> Mapping[str, float]:
        trading_volume = super().compute(df, context)[self.name]
        trading_volume /= get_total_days(df['timestamp'])
        return {self.name: trading_volume}


class TradingValue(Metric):
    def __init__(self, name: str = None, book_size: float | None = None):
        self.name = (
            name if name is not None else ('TradingValue' if book_size is None else 'Turnover')
        )
        self.book_size = book_size

    def compute(self, df: pl.DataFrame, context: Dict[str, float]) -> Mapping[str, float]:
        trading_value = df['trading_value'].sum()
        if self.book_size is not None:
            trading_value /= self.book_size
        return {self.name: trading_value}


class DailyTradingValue(TradingValue):
    def __init__(self, name: str = None, book_size: float | None = None):
        super().__init__(
            name if name is not None else ('DailyTradingValue' if book_size is None else 'DailyTurnover'),
            book_size
        )

    def compute(self, df: pl.DataFrame, context: Dict[str, float]) -> Mapping[str, float]:
        trading_value = super().compute(df, context)[self.name]
        trading_value /= get_total_days(df['timestamp'])
        return {self.name: trading_value}


class MaxPositionValue(Metric):
    def __init__(self, name: str = None):
        self.name = name if name is not None else 'MaxPositionValue'

    def compute(self, df: pl.DataFrame, context: Dict[str, float]) -> Mapping[str, float]:
        return {self.name: (df['position'].abs() * df['price']).max()}


class MeanPositionValue(Metric):
    def __init__(self, name: str = None):
        self.name = name if name is not None else 'MeanPositionValue'

    def compute(self, df: pl.DataFrame, context: Dict[str, float]) -> Mapping[str, float]:
        return {self.name: (df['position'].abs() * df['price']).mean()}


class MedianPositionValue(Metric):
    def __init__(self, name: str = None):
        self.name = name if name is not None else 'MedianPositionValue'

    def compute(self, df: pl.DataFrame, context: Dict[str, float]) -> Mapping[str, float]:
        return {self.name: (df['position'].abs() * df['price']).median()}


class MaxLeverage(Metric):
    def __init__(self, name: str = None, book_size: float = 0.0):
        if book_size <= 0.0:
            warnings.warn('book_size should be positive.', UserWarning)
        self.name = name if name is not None else 'MaxLeverage'
        self.book_size = book_size

    def compute(self, df: pl.DataFrame, context: Dict[str, float]) -> Mapping[str, float]:
        return {self.name: (df['position'].abs() * df['price']).max() / self.book_size}
