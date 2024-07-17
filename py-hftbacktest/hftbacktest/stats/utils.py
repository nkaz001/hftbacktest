import warnings
from typing import List

import polars as pl

SECONDS_PER_DAY = 24 * 60 * 60


def get_num_samples_per_day(timestamp: pl.Series) -> float:
    interval = timestamp.diff()
    if (interval[1:-1] != interval[2:]).sum() > 0:
        warnings.warn('The sampling interval is not consistent. Use resample().', UserWarning)
    sampling_interval = (timestamp[1] - timestamp[0]).total_seconds()
    return SECONDS_PER_DAY / sampling_interval


def get_total_days(timestamp: pl.Series) -> float:
    return (timestamp[-1] - timestamp[0]).total_seconds() / SECONDS_PER_DAY


def monthly(df: pl.DataFrame) -> List[pl.DataFrame]:
    return df.with_columns(
        pl.col('timestamp').dt.strftime('%Y%m').alias('dt')
    ).partition_by('dt')


def daily(df: pl.DataFrame) -> List[pl.DataFrame]:
    return df.with_columns(
        pl.col('timestamp').dt.strftime('%Y%m%d').alias('dt')
    ).partition_by('dt')


def hourly(df: pl.DataFrame) -> List[pl.DataFrame]:
    return df.with_columns(
        pl.col('timestamp').dt.strftime('%Y%m%d:%H').alias('dt')
    ).partition_by('dt')


def resample(df: pl.DataFrame, frequency: str) -> pl.DataFrame:
    agg_cols = []
    for col in df.columns:
        if col == 'timestamp':
            continue
        elif col == 'trading_value_':
            agg_cols.append(pl.col(col).sum())
        elif col == 'trading_volume_':
            agg_cols.append(pl.col(col).sum())
        elif col == 'num_trades_':
            agg_cols.append(pl.col(col).sum())
        else:
            agg_cols.append(pl.col(col).last())
    return df.group_by_dynamic('timestamp', every=frequency).agg(*agg_cols)
