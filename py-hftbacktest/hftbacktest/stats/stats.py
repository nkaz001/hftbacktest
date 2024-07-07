import inspect
from abc import ABC, abstractmethod
from typing import Any, List, Type, Mapping

import holoviews as hv
import numpy as np
import polars as pl
from numpy.typing import NDArray

from .metrics import (
    Metric,
    SR,
    Sortino,
    Ret,
    MaxDrawdown,
    DailyTradingValue,
    ReturnOverMDD,
    ReturnOverTrade,
    MaxPositionValue, DailyNumberOfTrades
)
from .utils import resample, monthly, daily, hourly


def compute_metrics(
        df: pl.DataFrame,
        metrics: List[Metric | Type[Metric]],
        kwargs: Mapping[str, Any]
) -> Mapping[str, Any]:
    context = {
        'start': df['timestamp'][0],
        'end': df['timestamp'][-1],
    }

    for metric in metrics:
        if isinstance(metric, type):
            sig = inspect.signature(metric.__init__)
            valid_kwargs = {k: v for k, v in kwargs.items() if k in sig.parameters}
            metric = metric(**valid_kwargs)

        ret = metric.compute(df, context)

        for key, value in ret.items():
            context[key] = value

    return context


class Stats:
    DEFAULT_EXTENSION = ('bokeh')

    def __init__(self, entire: pl.DataFrame, splits: List[Mapping[str, Any]], kwargs):
        self.entire = entire
        self.splits = splits
        self.kwargs = kwargs

    def summary(self, pretty: bool = False):
        df = pl.DataFrame(self.splits)
        return df

    def plot(self, price_as_ret: bool = False, extension: List[str] | None = DEFAULT_EXTENSION):
        if extension is not None:
            hv.extension(extension)

        entire_df = self.entire
        kwargs = self.kwargs

        equity = entire_df['equity_wo_fee'] - entire_df['fee']
        equity_wo_fee = entire_df['equity_wo_fee']

        book_size = kwargs.get('book_size')
        if book_size is not None:
            if price_as_ret:
                equity_plt = hv.Overlay([
                    hv.Curve(
                        (entire_df['timestamp'], equity / book_size),
                        label='Equity',
                        vdims=['Cumulative Return (%)']
                    ),
                    hv.Curve(
                        (entire_df['timestamp'], equity_wo_fee / book_size),
                        label='Equity w/o fee',
                        vdims=['Cumulative Return (%)']
                    ),
                    hv.Curve(
                        (entire_df['timestamp'], entire_df['price'] / entire_df['price'][0] - 1.0),
                        label='Price',
                        vdims=['Cumulative Return (%)']
                    ).opts(alpha=0.2, color='black')
                ])
            else:
                equity_plt = hv.Overlay([
                    hv.Curve(
                        (entire_df['timestamp'], equity / book_size),
                        label='Equity',
                        vdims=['Cumulative Return (%)']
                    ),
                    hv.Curve(
                        (entire_df['timestamp'], equity_wo_fee / book_size),
                        label='Equity w/o fee',
                        vdims=['Cumulative Return (%)']
                    )
                ]) * hv.Curve(
                    (entire_df['timestamp'], entire_df['price']),
                    label='Price',
                    vdims=['Price']
                ).opts(alpha=0.2, color='black')
        else:
            equity_plt = hv.Overlay([
                hv.Curve(
                    (entire_df['timestamp'], equity),
                    label='Equity',
                    vdims=['Cumulative Return (%)']
                ),
                hv.Curve(
                    (entire_df['timestamp'], equity_wo_fee),
                    label='Equity w/o fee',
                    vdims=['Cumulative Return (%)']
                )
            ]) * hv.Curve(
                (entire_df['timestamp'], entire_df['price']),
                label='Price',
                vdims=['Price']
            ).opts(alpha=0.2, color='black')

        px_plt = hv.Curve(
            (entire_df['timestamp'], entire_df['price']),
            label='Price',
            vdims=['Price']
        ).opts(alpha=0.2, color='black')
        pos_plt = hv.Curve(
            (entire_df['timestamp'], entire_df['position']),
            label='Position',
            vdims=['Position (Qty)']
        )

        plt1 = equity_plt.opts(yformatter='$%.2f')
        plt1.opts(multi_y=True, width=1000, height=400, legend_position='right')

        plt2 = pos_plt.opts(yformatter='$%d') * px_plt
        plt2.opts(multi_y=True, width=1000, height=400, legend_position='right')

        return (plt1.relabel('Equity') + plt2.relabel('Position')).cols(1)


class Record(ABC):
    DEFAULT_METRICS = (
        SR,
        Sortino,
        Ret,
        MaxDrawdown,
        DailyNumberOfTrades,
        DailyTradingValue,
        ReturnOverMDD,
        ReturnOverTrade,
        MaxPositionValue
    )

    def __init__(self, data: NDArray | pl.DataFrame):
        self._contract_size = 1.0
        self._time_unit = 'ns'
        self._frequency = '10s'
        self._partition = None

        if isinstance(data, np.ndarray):
            self.df = pl.DataFrame(data)
        elif isinstance(data, pl.DataFrame):
            self.df = data
        else:
            raise ValueError

    def contract_size(self, contract_size: str) -> 'Self':
        self._contract_size = contract_size
        return self

    def time_unit(self, time_unit: str) -> 'Self':
        self._time_unit = time_unit
        return self

    def resample(self, frequency: str) -> 'Self':
        self._frequency = frequency
        return self

    def monthly(self) -> 'Self':
        self._partition = 'monthly'
        return self

    def daily(self) -> 'Self':
        self._partition = 'daily'
        return self

    @abstractmethod
    def prepare(self):
        raise NotImplementedError

    def stats(
            self,
            metrics: List[Metric | Type[Metric]] = DEFAULT_METRICS,
            **kwargs: Any
    ) -> Stats:
        if not isinstance(self.df['timestamp'].dtype, pl.Datetime):
            self.df = self.df.with_columns(
                pl.from_epoch('timestamp', time_unit=self._time_unit)
            )

        if 'price' not in self.df and 'mid_price' in self.df:
            self.df = self.df.with_columns(
                pl.col('mid_price').alias('price')
            )

        if 'num_trades' not in self.df:
            if 'trade_num' not in self.df:
                # This may not reflect the exact value since information could be lost between recording intervals.
                num_trades = self.df['position'].diff().fill_null(0).abs()
                num_trades = num_trades.set(num_trades > 0, 1)
                self.df = self.df.with_columns(
                    num_trades.alias('num_trades')
                )
            else:
                self.df = self.df.with_columns(
                    pl.col('trade_num').diff().fill_null(0).alias('num_trades')
                )

        if 'trading_volume' not in self.df:
            if 'trade_qty' not in self.df:
                # This may not reflect the exact value since information could be lost between recording intervals.
                self.df = self.df.with_columns(
                    pl.col('position').diff().fill_null(0).abs().alias('trading_volume')
                )
            else:
                self.df = self.df.with_columns(
                    pl.col('trade_qty').diff().fill_null(0).alias('trading_volume')
                )

        # Prepares the asset type-specific data by computing it from the state records.
        self.prepare()

        if self._frequency is not None:
            # The DataFrame should be sorted by timestamp, even though it won't be resampled.
            self.df = self.df.set_sorted('timestamp')
            self.df = resample(self.df, self._frequency)

        if self._partition == 'monthly':
            splits = monthly(self.df)
        elif self._partition == 'daily':
            splits = daily(self.df)
        elif self._partition == 'hourly':
            splits = hourly(self.df)
        else:
            splits = []

        stats = [compute_metrics(df, metrics, kwargs) for df in splits]
        # For the entire period.
        stats.append(compute_metrics(self.df, metrics, kwargs))

        return Stats(self.df, stats, kwargs)


class LinearAssetRecord(Record):
    def prepare(self):
        if 'equity_wo_fee' not in self.df:
            self.df = self.df.with_columns(
                (
                    pl.col('balance') + pl.col('position') * pl.col('price') * self._contract_size
                ).alias('equity_wo_fee')
            )

        if 'trading_value' not in self.df:
            if 'trade_amount' not in self.df:
                # This may not reflect the exact value since information could be lost between recording intervals.
                self.df = self.df.with_columns(
                    (
                        pl.col('position').diff().fill_null(0) * pl.col('price') * self._contract_size
                    ).alias('trading_value')
                )
            else:
                self.df = self.df.with_columns(
                    pl.col('trade_amount').diff().fill_null(0).alias('trading_value')
                )


class InverseAssetRecord(Record):
    def prepare(self):
        if 'equity_wo_fee' not in self.df:
            self.df = self.df.with_columns(
                (
                    -pl.col('balance') - pl.col('position') / pl.col('price') * self._contract_size
                ).alias('equity_wo_fee')
            )

        if 'trade_amount_for' not in self.df:
            if 'trade_amount' not in self.df:
                # This may not reflect the exact value since information could be lost between recording intervals.
                self.df = self.df.with_columns(
                    (
                        (pl.col('position').diff().fill_null(0) / pl.col('price')) * self._contract_size
                    ).alias('trading_value')
                )
            else:
                self.df = self.df.with_columns(
                    pl.col('trade_amount').diff().fill_null(0).alias('trading_value')
                )
