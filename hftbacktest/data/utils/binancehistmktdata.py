from typing import Optional, Literal

import numpy as np
from numpy.typing import NDArray

from .. import merge_on_local_timestamp, correct, validate_data


def convert_snapshot(
        snapshot_filename: str,
        output_filename: Optional[str] = None,
        feed_latency: float = 0
) -> NDArray:
    r"""
    Converts Binance Historical Market Data files into a format compatible with HftBacktest.
    Since it doesn't have a local timestamp, it lacks feed latency information, which can result in a significant
    discrepancy between live and backtest results.
    Collecting feed data yourself or obtaining the high quality of data from a data vendor is strongly recommended.

    https://www.binance.com/en/landing/data

    Args:
        snapshot_filename: Snapshot filename
        output_filename: If provided, the converted data will be saved to the specified filename in ``npz`` format.
        feed_latency: Artificial feed latency value to be added to the exchange timestamp to create local timestamp.

    Returns:
        Converted data compatible with HftBacktest.
    """
    ss_bid = []
    ss_ask = []

    # Reads snapshot file
    print('Reading %s' % snapshot_filename)
    with open(snapshot_filename, 'r') as f:
        # Skips the header
        f.readline()
        while True:
            line = f.readline()
            if line is None or line == '':
                break
            cols = line.strip().split(',')

            exch_timestamp = int(cols[1])
            loc_timestamp = exch_timestamp + feed_latency
            side = 1 if cols[4] == 'b' else -1
            price = float(cols[6])
            qty = float(cols[7])

            if side == 1:
                ss_bid.append([
                    4,
                    exch_timestamp,
                    loc_timestamp,
                    side,
                    price,
                    qty
                ])
            else:
                ss_ask.append([
                    4,
                    exch_timestamp,
                    loc_timestamp,
                    side,
                    price,
                    qty
                ])

    snapshot = []
    snapshot += [cols for cols in sorted(ss_bid, key=lambda v: -float(v[4]))]
    snapshot += [cols for cols in sorted(ss_ask, key=lambda v: float(v[4]))]

    snapshot = np.asarray(snapshot, np.float64)

    if output_filename is not None:
        np.savez(output_filename, data=snapshot)

    return snapshot


def convert(
        depth_filename: str,
        trades_filename: str,
        output_filename: Optional[str] = None,
        buffer_size: int = 100_000_000,
        feed_latency: float = 0,
        base_latency: float = 0,
        method: Literal['separate', 'adjust'] = 'separate'
) -> NDArray:
    r"""
    Converts Binance Historical Market Data files into a format compatible with HftBacktest.
    Since it doesn't have a local timestamp, it lacks feed latency information, which can result in a significant
    discrepancy between live and backtest results.
    Collecting feed data yourself or obtaining the high quality of data from a data vendor is strongly recommended.

    https://www.binance.com/en/landing/data

    Args:
        depth_filename: Depth data filename
        trades_filename: Trades data filename
        output_filename: If provided, the converted data will be saved to the specified filename in ``npz`` format.
        buffer_size: Sets a preallocated row size for the buffer.
        feed_latency: Artificial feed latency value to be added to the exchange timestamp to create local timestamp.
        base_latency: The value to be added to the feed latency.
                      See :func:`.correct_local_timestamp`.
        method: The method to correct reversed exchange timestamp events. See :func:`..validation.correct`.

    Returns:
        Converted data compatible with HftBacktest.
    """
    tmp_depth = np.empty((buffer_size, 6), np.float64)
    row_num = 0

    print('Reading %s' % depth_filename)
    with open(depth_filename, 'r') as f:
        # Skips the header
        f.readline()
        while True:
            line = f.readline()
            if line is None or line == '':
                break
            cols = line.strip().split(',')

            exch_timestamp = int(cols[1])
            loc_timestamp = exch_timestamp + feed_latency
            side = 1 if cols[4] == 'b' else -1
            price = float(cols[6])
            qty = float(cols[7])

            # Insert DEPTH_EVENT
            tmp_depth[row_num] = [
                1,
                exch_timestamp,
                loc_timestamp,
                side,
                price,
                qty
            ]
            row_num += 1
    tmp_depth = tmp_depth[:row_num]

    tmp_trades = np.empty((buffer_size, 6), np.float64)
    row_num = 0

    print('Reading %s' % trades_filename)
    with open(trades_filename, 'r') as f:
        while True:
            line = f.readline()
            if line is None or line == '':
                break
            cols = line.strip().split(',')
            # Checks if it's a header.
            if cols[0] == 'id':
                continue

            exch_timestamp = int(cols[4])
            loc_timestamp = exch_timestamp + feed_latency
            side = -1 if cols[5] else 1  # trade initiator's side
            price = float(cols[1])
            qty = float(cols[2])

            # Insert TRADE_EVENT
            tmp_trades[row_num] = [
                2,
                exch_timestamp,
                loc_timestamp,
                side,
                price,
                qty
            ]
            row_num += 1
    tmp_trades = tmp_trades[:row_num]

    print('Merging')
    data = merge_on_local_timestamp(tmp_depth, tmp_trades)
    data = correct(data, base_latency=base_latency, method=method)

    # Validate again.
    num_corr = validate_data(data)
    if num_corr < 0:
        raise ValueError

    if output_filename is not None:
        print('Saving to %s' % output_filename)
        np.savez(output_filename, data=data)

    return data
