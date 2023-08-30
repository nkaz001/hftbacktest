import csv
from typing import Optional, Literal

import numpy as np
from numpy.typing import NDArray

from .. import merge_on_local_timestamp, correct, validate_data
from ... import DEPTH_EVENT, TRADE_EVENT, DEPTH_SNAPSHOT_EVENT


def convert_snapshot(
        snapshot_filename: str,
        output_filename: Optional[str] = None,
        feed_latency: float = 0,
        has_header: Optional[bool] = None,
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
        has_header: True if the given file has a header, it will automatically detect it if set to None.

    Returns:
        Converted data compatible with HftBacktest.
    """
    ss_bid = []
    ss_ask = []

    timestamp_col = None
    side_col = None
    price_col = None
    qty_col = None

    # Reads snapshot file
    print('Reading %s' % snapshot_filename)
    with open(snapshot_filename, 'r', newline='') as f:
        reader = csv.reader(f, delimiter=',')
        for row in reader:
            if timestamp_col is None:
                if has_header is None:
                    if row[0] == 'symbol':
                        has_header = True
                    else:
                        has_header = False

                if has_header:
                    header = row
                else:
                    header = [
                        'symbol',
                        'timestamp',
                        'trans_id',
                        'first_update_id',
                        'last_update_id',
                        'side',
                        'update_type',
                        'price',
                        'qty'
                    ]
                    if len(header) != len(row):
                        raise ValueError

                timestamp_col = header.index('timestamp')
                side_col = header.index('side')
                price_col = header.index('price')
                qty_col = header.index('qty')

                if has_header:
                    continue

            exch_timestamp = int(row[timestamp_col])
            loc_timestamp = exch_timestamp + feed_latency
            side = 1 if row[side_col] == 'b' else -1
            price = float(row[price_col])
            qty = float(row[qty_col])

            if side == 1:
                ss_bid.append([
                    DEPTH_SNAPSHOT_EVENT,
                    exch_timestamp,
                    loc_timestamp,
                    side,
                    price,
                    qty
                ])
            else:
                ss_ask.append([
                    DEPTH_SNAPSHOT_EVENT,
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
        method: Literal['separate', 'adjust'] = 'separate',
        depth_has_header: Optional[bool] = None,
        trades_has_header: Optional[bool] = None
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
        depth_has_header: True if the given file has a header, it will automatically detect it if set to None.
        trades_has_header: True if the given file has a header, it will automatically detect it if set to None.

    Returns:
        Converted data compatible with HftBacktest.
    """
    tmp_depth = np.empty((buffer_size, 6), np.float64)
    row_num = 0

    timestamp_col = None
    side_col = None
    price_col = None
    qty_col = None

    print('Reading %s' % depth_filename)
    with open(depth_filename, 'r', newline='') as f:
        reader = csv.reader(f, delimiter=',')
        for row in reader:
            if timestamp_col is None:
                if depth_has_header is None:
                    if row[0] == 'symbol':
                        depth_has_header = True
                    else:
                        depth_has_header = False

                if depth_has_header:
                    header = row
                else:
                    header = [
                        'symbol',
                        'timestamp',
                        'trans_id',
                        'first_update_id',
                        'last_update_id',
                        'side',
                        'update_type',
                        'price',
                        'qty'
                    ]
                    if len(header) != len(row):
                        raise ValueError

                timestamp_col = header.index('timestamp')
                side_col = header.index('side')
                price_col = header.index('price')
                qty_col = header.index('qty')

                if depth_has_header:
                    continue

            exch_timestamp = int(row[timestamp_col])
            loc_timestamp = exch_timestamp + feed_latency
            side = 1 if row[side_col] == 'b' else -1
            price = float(row[price_col])
            qty = float(row[qty_col])

            # Insert DEPTH_EVENT
            tmp_depth[row_num] = [
                DEPTH_EVENT,
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

    timestamp_col = None
    side_col = None
    price_col = None
    qty_col = None

    print('Reading %s' % trades_filename)
    with open(trades_filename, 'r', newline='') as f:
        reader = csv.reader(f, delimiter=',')
        for row in reader:
            if timestamp_col is None:
                if trades_has_header is None:
                    if row[0] == 'id':
                        trades_has_header = True
                    else:
                        trades_has_header = False

                if trades_has_header:
                    header = row
                else:
                    header = [
                        'id',
                        'price',
                        'qty',
                        'quote_qty',
                        'time',
                        'is_buyer_maker'
                    ]
                    if len(header) != len(row):
                        raise ValueError

                timestamp_col = header.index('time')
                side_col = header.index('is_buyer_maker')
                price_col = header.index('price')
                qty_col = header.index('qty')

                if trades_has_header:
                    continue

            exch_timestamp = int(row[timestamp_col])
            loc_timestamp = exch_timestamp + feed_latency
            side = -1 if row[side_col] else 1  # trade initiator's side
            price = float(row[price_col])
            qty = float(row[qty_col])

            # Insert TRADE_EVENT
            tmp_trades[row_num] = [
                TRADE_EVENT,
                exch_timestamp,
                loc_timestamp,
                side,
                price,
                qty
            ]
            row_num += 1
    tmp_trades = tmp_trades[:row_num]

    # A mingled exchange timestamp is frequently observed on Binance.
    # But, because the data doesn't have a local timestamp, there's difficulty in preserving the received order while
    # keeping the local timestamp in sequence.
    # A simple solution is to sort by the derived local timestamp to resolve the issue.
    tmp_depth = tmp_depth[tmp_depth[:, 2].argsort()]
    tmp_trades = tmp_trades[tmp_trades[:, 2].argsort()]

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
