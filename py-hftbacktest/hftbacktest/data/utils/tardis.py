import polars as pl
from typing import List, Optional, Literal

import numpy as np
from numba import njit
from numpy.typing import NDArray

from ..validation import correct_event_order, validate_event_order, correct_local_timestamp
from ...types import (
    DEPTH_EVENT,
    DEPTH_CLEAR_EVENT,
    DEPTH_SNAPSHOT_EVENT,
    TRADE_EVENT,
    BUY_EVENT,
    SELL_EVENT,
    event_dtype
)

trade_cols = [
    'exchange',
    'symbol',
    'timestamp',
    'local_timestamp',
    'id',
    'side',
    'price',
    'amount'
]

depth_cols = [
    'exchange',
    'symbol',
    'timestamp',
    'local_timestamp',
    'is_snapshot',
    'side',
    'price',
    'amount'
]


def convert(
        input_files: List[str],
        output_filename: Optional[str] = None,
        buffer_size: int = 100_000_000,
        ss_buffer_size: int = 1_000_000,
        base_latency: float = 0,
        snapshot_mode: Literal['process', 'ignore_sod', 'ignore'] = 'process',
) -> NDArray:
    r"""
    Converts Tardis.dev data files into a format compatible with HftBacktest.

    For Tardis's Binance Futures feed data, they use the 'E' event timestamp, representing the sending time, rather
    than the 'T' transaction time, indicating when the matching occurs. So the latency is slightly less than it actually
    is.

    If you encounter an ``IndexError`` due to an out-of-bounds, try increasing the ``buffer_size`` and
    ``ss_buffer_size``.

    Args:
        input_files: Input filenames for both incremental book and trades files,
                     e.g. ['incremental_book.csv.gz', 'trades.csv.gz'].
        output_filename: If provided, the converted data will be saved to the specified filename in ``npz`` format.
        buffer_size: Sets a preallocated row size for the buffer.
        ss_buffer_size: Sets a preallocated row size for the snapshot.
        base_latency: The value to be added to the feed latency.
                      See :func:`.correct_local_timestamp`.
        snapshot_mode: - If this is set to 'ignore', all snapshots are ignored. The order book will converge to a
                         complete order book over time.
                       - If this is set to 'ignore_sod', the SOD (Start of Day) snapshot is ignored.
                         Since Tardis intentionally adds the SOD snapshot, not due to a message ID gap or disconnection,
                         there might not be a need to process SOD snapshot to build a complete order book.
                         Please see https://docs.tardis.dev/historical-data-details#collected-order-book-data-details
                         for more details.
                       - Otherwise, all snapshot events will be processed.
    Returns:
        Converted data compatible with HftBacktest.
    """
    tmp = np.empty(buffer_size, event_dtype)
    ss_bid = np.empty(ss_buffer_size, event_dtype)
    ss_ask = np.empty(ss_buffer_size, event_dtype)

    row_num = 0

    for file in input_files:
        print('Reading %s' % file)
        df = pl.read_csv(file)
        if df.columns == trade_cols:
            arr = (
                df.with_columns(
                    pl.when(pl.col('side') == 'buy')
                        .then(BUY_EVENT | TRADE_EVENT)
                        .when(pl.col('side') == 'sell')
                        .then(SELL_EVENT | TRADE_EVENT)
                        .otherwise(TRADE_EVENT)
                        .cast(pl.UInt64, strict=True)
                        .alias('ev'),
                    (pl.col('timestamp') * 1000)
                        .cast(pl.Int64, strict=True)
                        .alias('exch_ts'),
                    (pl.col('local_timestamp') * 1000)
                        .cast(pl.Int64, strict=True)
                        .alias('local_ts'),
                    pl.col('price')
                        .cast(pl.Float64, strict=True)
                        .alias('px'),
                    pl.col('amount')
                        .cast(pl.Float64, strict=True)
                        .alias('qty'),
                    pl.lit(0)
                        .cast(pl.UInt64, strict=True)
                        .alias('order_id'),
                    pl.lit(0)
                        .cast(pl.Int64, strict=True)
                        .alias('ival'),
                    pl.lit(0.0)
                        .cast(pl.Float64, strict=True)
                        .alias('fval')
                )
                .select(['ev', 'exch_ts', 'local_ts', 'px', 'qty', 'order_id', 'ival', 'fval'])
                .to_numpy(structured=True)
            )
            tmp[row_num:row_num + len(arr)] = arr[:]
            row_num += len(arr)
        elif df.columns == depth_cols:
            arr = (
                df.with_columns(
                    (pl.col('timestamp') * 1000)
                        .cast(pl.Int64, strict=True)
                        .alias('exch_ts'),
                    (pl.col('local_timestamp') * 1000)
                        .cast(pl.Int64, strict=True)
                        .alias('local_ts'),
                    pl.col('price')
                        .cast(pl.Float64, strict=True)
                        .alias('px'),
                    pl.col('amount')
                        .cast(pl.Float64, strict=True)
                        .alias('qty'),
                    pl.when((pl.col('side') == 'bid') | (pl.col('side') == 'buy'))
                        .then(1)
                        .when((pl.col('side') == 'ask') | (pl.col('side') == 'sell'))
                        .then(-1)
                        .otherwise(0)
                        .cast(pl.Int8, strict=True)
                        .alias('side'),
                    pl.when(pl.col('is_snapshot'))
                        .then(1)
                        .otherwise(0)
                        .cast(pl.Int8, strict=True)
                        .alias('is_snapshot')
                )
                .select(['exch_ts', 'local_ts', 'px', 'qty', 'side', 'is_snapshot'])
                .to_numpy(structured=True)
            )

            snapshot_mode_flag = 0
            if snapshot_mode == 'ignore':
                snapshot_mode_flag = SNAPSHOT_MODE_IGNORE
            elif snapshot_mode == 'ignore_sod':
                snapshot_mode_flag = SNAPSHOT_MODE_IGNORE_SOD
            row_num = convert_depth(tmp, arr, row_num, ss_bid, ss_ask, snapshot_mode_flag)
    tmp = tmp[:row_num]

    print('Correcting the latency')
    tmp = correct_local_timestamp(tmp, base_latency)

    print('Correcting the event order')
    data = correct_event_order(
        tmp,
        np.argsort(tmp['exch_ts'], kind='mergesort'),
        np.argsort(tmp['local_ts'], kind='mergesort')
    )

    validate_event_order(data)

    if output_filename is not None:
        print('Saving to %s' % output_filename)
        np.savez_compressed(output_filename, data=data)

    return data


SNAPSHOT_MODE_IGNORE = 1
SNAPSHOT_MODE_IGNORE_SOD = 2


@njit
def convert_depth(out, inp, row_num, ss_bid, ss_ask, snapshot_mode):
    ss_bid_rn = 0
    ss_ask_rn = 0
    is_sod_snapshot = True
    is_snapshot = False
    for rn in range(len(inp)):
        row = inp[rn]
        if row.is_snapshot == 1:
            if (
                (snapshot_mode == SNAPSHOT_MODE_IGNORE)
                or (snapshot_mode == SNAPSHOT_MODE_IGNORE_SOD and is_sod_snapshot)
            ):
                continue
            # Prepare to insert DEPTH_SNAPSHOT_EVENT
            if not is_snapshot:
                is_snapshot = True
                ss_bid_rn = 0
                ss_ask_rn = 0
            if row.side == 1:
                ss_bid[ss_bid_rn].ev = DEPTH_SNAPSHOT_EVENT | BUY_EVENT
                ss_bid[ss_bid_rn].exch_ts = row.exch_ts
                ss_bid[ss_bid_rn].local_ts = row.local_ts
                ss_bid[ss_bid_rn].px = row.px
                ss_bid[ss_bid_rn].qty = row.qty
                ss_bid[ss_bid_rn].order_id = 0
                ss_bid[ss_bid_rn].ival = 0
                ss_bid[ss_bid_rn].fval = 0
                ss_bid_rn += 1
            else:
                ss_ask[ss_ask_rn].ev = DEPTH_SNAPSHOT_EVENT | SELL_EVENT
                ss_ask[ss_ask_rn].exch_ts = row.exch_ts
                ss_ask[ss_ask_rn].local_ts = row.local_ts
                ss_ask[ss_ask_rn].px = row.px
                ss_ask[ss_ask_rn].qty = row.qty
                ss_ask[ss_ask_rn].order_id = 0
                ss_ask[ss_ask_rn].ival = 0
                ss_ask[ss_ask_rn].fval = 0
                ss_ask_rn += 1
        else:
            is_sod_snapshot = False
            if is_snapshot:
                # End of the snapshot.
                is_snapshot = False

                # Add DEPTH_CLEAR_EVENT before refreshing the market depth by the snapshot.
                ss_bid = ss_bid[:ss_bid_rn]
                if len(ss_bid) > 0:
                    # Clear the bid market depth within the snapshot bid range.
                    out[row_num].ev = DEPTH_CLEAR_EVENT | BUY_EVENT
                    out[row_num].exch_ts = ss_bid[0].exch_ts
                    out[row_num].local_ts = ss_bid[0].local_ts
                    out[row_num].px = ss_bid[-1].px
                    out[row_num].qty = 0
                    out[row_num].order_id = 0
                    out[row_num].ival = 0
                    out[row_num].fval = 0
                    row_num += 1
                    # Add DEPTH_SNAPSHOT_EVENT for the bid snapshot
                    out[row_num:row_num + len(ss_bid)] = ss_bid[:]
                    row_num += len(ss_bid)
                ss_bid_rn = 0

                ss_ask = ss_ask[:ss_ask_rn]
                if len(ss_ask) > 0:
                    # Clear the ask market depth within the snapshot ask range.
                    out[row_num].ev = DEPTH_CLEAR_EVENT | SELL_EVENT
                    out[row_num].exch_ts = ss_ask[0].exch_ts
                    out[row_num].local_ts = ss_ask[0].local_ts
                    out[row_num].px = ss_ask[-1].px
                    out[row_num].qty = 0
                    out[row_num].order_id = 0
                    out[row_num].ival = 0
                    out[row_num].fval = 0
                    row_num += 1
                    # Add DEPTH_SNAPSHOT_EVENT for the ask snapshot
                    out[row_num:row_num + len(ss_ask)] = ss_ask[:]
                    row_num += len(ss_ask)
                ss_ask_rn = 0
            # Insert DEPTH_EVENT
            out[row_num].ev = DEPTH_EVENT | (BUY_EVENT if row.side == 1 else SELL_EVENT)
            out[row_num].exch_ts = row.exch_ts
            out[row_num].local_ts = row.local_ts
            out[row_num].px = row.px
            out[row_num].qty = row.qty
            out[row_num].order_id = 0
            out[row_num].ival = 0
            out[row_num].fval = 0
            row_num += 1
    return row_num
