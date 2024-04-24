import gzip
from typing import List, Optional, Literal

import numpy as np
from numpy.typing import NDArray

from .. import validate_data
from ..validation import correct_event_order, convert_to_struct_arr
from ... import (
    DEPTH_CLEAR_EVENT,
    DEPTH_SNAPSHOT_EVENT,
    TRADE_EVENT,
    DEPTH_EVENT,
    COL_LOCAL_TIMESTAMP,
    COL_EXCH_TIMESTAMP,
    correct_local_timestamp
)


def convert(
        input_files: List[str],
        output_filename: Optional[str] = None,
        buffer_size: int = 100_000_000,
        ss_buffer_size: int = 1_000_000,
        base_latency: float = 0,
        snapshot_mode: Literal['process', 'ignore_sod', 'ignore'] = 'process',
        compress: bool = False,
        structured_array: bool = False,
        timestamp_unit: Literal['us', 'ns'] = 'us'
) -> NDArray:
    r"""
    Converts Tardis.dev data files into a format compatible with HftBacktest.

    For Tardis's Binance Futures feed data, they use the 'E' event timestamp, representing the sending time, rather
    than the 'T' transaction time, indicating when the matching occurs. So the latency is slightly less than it actually
    is.

    Args:
        input_files: Input filenames for both incremental book and trades files,
                     e.g. ['incremental_book.csv', 'trades.csv'].
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
        compress: If this is set to True, the output file will be compressed.
        structured_array: If this is set to True, the output is converted into the new format(currently only Rust impl).
        timestamp_unit: The timestamp unit for timestamp to be converted in. Tardis provides timestamps in microseconds.
    Returns:
        Converted data compatible with HftBacktest.
    """
    if timestamp_unit == 'us':
        timestamp_mul = 1
    elif timestamp_unit == 'ns':
        timestamp_mul = 1000
    else:
        raise ValueError

    TRADE = 0
    DEPTH = 1

    sets = []
    for file in input_files:
        file_type = None
        tmp = np.empty((buffer_size, 6), np.float64)
        row_num = 0
        is_snapshot = False
        ss_bid = None
        ss_ask = None
        ss_bid_rn = 0
        ss_ask_rn = 0
        is_sod_snapshot = True
        print('Reading %s' % file)
        with gzip.open(file, 'r') as f:
            while True:
                line = f.readline()
                if line is None or line == b'':
                    break
                cols = line.decode().strip().split(',')
                if len(cols) < 8:
                    print('Warning: Invalid Data Row', cols, line)
                    continue
                if file_type is None:
                    if cols == [
                        'exchange',
                        'symbol',
                        'timestamp',
                        'local_timestamp',
                        'id',
                        'side',
                        'price',
                        'amount'
                    ]:
                        file_type = TRADE
                    elif cols == [
                        'exchange',
                        'symbol',
                        'timestamp',
                        'local_timestamp',
                        'is_snapshot',
                        'side',
                        'price',
                        'amount'
                    ]:
                        file_type = DEPTH
                elif file_type == TRADE:
                    # Insert TRADE_EVENT
                    tmp[row_num] = [
                        TRADE_EVENT,
                        int(cols[2]) * timestamp_mul,
                        int(cols[3]) * timestamp_mul,
                        1 if cols[5] == 'buy' else -1,
                        float(cols[6]),
                        float(cols[7])
                    ]
                    row_num += 1
                elif file_type == DEPTH:
                    if cols[4] == 'true':
                        if (snapshot_mode == 'ignore') or (snapshot_mode == 'ignore_sod' and is_sod_snapshot):
                            continue
                        # Prepare to insert DEPTH_SNAPSHOT_EVENT
                        if not is_snapshot:
                            is_snapshot = True
                            ss_bid = np.empty((ss_buffer_size, 6), np.float64)
                            ss_ask = np.empty((ss_buffer_size, 6), np.float64)
                            ss_bid_rn = 0
                            ss_ask_rn = 0
                        if cols[5] == 'bid':
                            ss_bid[ss_bid_rn] = [
                                DEPTH_SNAPSHOT_EVENT,
                                int(cols[2]) * timestamp_mul,
                                int(cols[3]) * timestamp_mul,
                                1,
                                float(cols[6]),
                                float(cols[7])
                            ]
                            ss_bid_rn += 1
                        else:
                            ss_ask[ss_ask_rn] = [
                                DEPTH_SNAPSHOT_EVENT,
                                int(cols[2]) * timestamp_mul,
                                int(cols[3]) * timestamp_mul,
                                -1,
                                float(cols[6]),
                                float(cols[7])
                            ]
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
                                tmp[row_num] = [
                                    DEPTH_CLEAR_EVENT,
                                    ss_bid[0, 1],
                                    ss_bid[0, 2],
                                    1,
                                    ss_bid[-1, 4],
                                    0
                                ]
                                row_num += 1
                                # Add DEPTH_SNAPSHOT_EVENT for the bid snapshot
                                tmp[row_num:row_num + len(ss_bid)] = ss_bid[:]
                                row_num += len(ss_bid)
                            ss_bid = None

                            ss_ask = ss_ask[:ss_ask_rn]
                            if len(ss_ask) > 0:
                                # Clear the ask market depth within the snapshot ask range.
                                tmp[row_num] = [
                                    DEPTH_CLEAR_EVENT,
                                    ss_ask[0, 1],
                                    ss_ask[0, 2],
                                    -1,
                                    ss_ask[-1, 4],
                                    0
                                ]
                                row_num += 1
                                # Add DEPTH_SNAPSHOT_EVENT for the ask snapshot
                                tmp[row_num:row_num + len(ss_ask)] = ss_ask[:]
                                row_num += len(ss_ask)
                            ss_ask = None
                        # Insert DEPTH_EVENT
                        tmp[row_num] = [
                            DEPTH_EVENT,
                            int(cols[2]) * timestamp_mul,
                            int(cols[3]) * timestamp_mul,
                            1 if cols[5] == 'bid' else -1,
                            float(cols[6]),
                            float(cols[7])
                        ]
                        row_num += 1
        sets.append(tmp[:row_num])

    print('Merging')
    merged = np.concatenate(sets)

    print('Correcting the latency')
    merged = correct_local_timestamp(merged, base_latency)

    print('Correcting the event order')
    sorted_exch_ts = merged[np.argsort(merged[:, COL_EXCH_TIMESTAMP], kind='mergesort')]
    sorted_local_ts = merged[np.argsort(merged[:, COL_LOCAL_TIMESTAMP], kind='mergesort')]

    data = correct_event_order(sorted_exch_ts, sorted_local_ts, structured_array)

    if not structured_array:
        # Validate again.
        num_corr = validate_data(data)
        if num_corr < 0:
            raise ValueError

    if structured_array:
        data = convert_to_struct_arr(data)

    if output_filename is not None:
        print('Saving to %s' % output_filename)
        if compress:
            np.savez_compressed(output_filename, data=data)
        else:
            np.savez(output_filename, data=data)

    return data
