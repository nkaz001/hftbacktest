from typing import Any

import numpy as np
import polars as pl
from numba import njit

from hftbacktest import LOCAL_EVENT, EXCH_EVENT, EVENT_ARRAY

order_latency_dtype = np.dtype(
    [
        ('req_ts', 'i8'),
        ('exch_ts', 'i8'),
        ('resp_ts', 'i8'),
        ('_padding', 'i8')
    ],
    align=True
)


@njit
def generate_order_latency_nb(
        data: EVENT_ARRAY,
        order_latency: np.ndarray[Any, order_latency_dtype],
        mul_entry: float,
        offset_entry: float,
        mul_resp: float,
        offset_resp: float
):
    for i in range(len(data)):
        exch_ts = data[i].exch_ts
        local_ts = data[i].local_ts
        feed_latency = local_ts - exch_ts
        order_entry_latency = mul_entry * feed_latency + offset_entry
        order_resp_latency = mul_resp * feed_latency + offset_resp

        req_ts = local_ts
        order_exch_ts = req_ts + order_entry_latency
        resp_ts = order_exch_ts + order_resp_latency

        order_latency[i].req_ts = req_ts
        order_latency[i].exch_ts = order_exch_ts
        order_latency[i].resp_ts = resp_ts


def generate_order_latency(
        feed_file: str,
        output_file: str | None = None,
        mul_entry: float = 1,
        offset_entry: float = 0,
        mul_resp: float = 1,
        offset_resp: float = 0,
        resampling_ns: int = 1_000_000_000
) -> np.ndarray[Any, order_latency_dtype]:
    """
    Generates synthetic order latency data based on market feed latency.

    Order latencies are modeled as:
        - Order entry latency    = mul_entry * feed_latency + offset_entry
        - Order response latency = mul_resp  * feed_latency + offset_resp

    Args:
        feed_file: Path to the market feed file.
        output_file: If provided, saves the generated order latency data into this file.
        mul_entry: Multiplier applied to the feed latency to compute entry latency.
        offset_entry: Constant offset added to the entry latency.
        mul_resp: Multiplier applied to the feed latency to compute response latency.
        offset_resp: Constant offset added to the response latency.
        resampling_ns: Resampling interval in nanoseconds. The synthetic order latency data is produced by resampling
                       the feed latency data at this interval. Default: 1_000_000_000 ns (1 s)

    Returns:
        Generated order latency data
    """
    data = np.load(feed_file)['data']
    df = pl.DataFrame(data)

    df = df.filter(
        (pl.col('ev') & EXCH_EVENT == EXCH_EVENT) & (pl.col('ev') & LOCAL_EVENT == LOCAL_EVENT)
    ).with_columns(
        pl.col('local_ts').alias('ts')
    ).group_by_dynamic(
        'ts', every=f'{resampling_ns}i'
    ).agg(
        pl.col('exch_ts').last(),
        pl.col('local_ts').last()
    ).drop('ts')

    data = df.to_numpy(structured=True)

    order_latency = np.zeros(
        len(data),
        dtype=order_latency_dtype
    )
    generate_order_latency_nb(data, order_latency, mul_entry, offset_entry, mul_resp, offset_resp)

    if output_file is not None:
        np.savez_compressed(output_file, data=order_latency)

    return order_latency
