import gzip
from typing import List, Optional, Literal

import numpy as np
from numpy.typing import NDArray

from .. import merge_on_local_timestamp, correct, validate_data
from ... import DEPTH_CLEAR_EVENT, DEPTH_SNAPSHOT_EVENT, TRADE_EVENT, DEPTH_EVENT


def convert(
        input_files: List[str],
        output_filename: Optional[str] = None,
        buffer_size: int = 100_000_000,
        ss_buffer_size: int = 10_000,
        base_latency: float = 0,
        method: Literal['separate', 'adjust'] = 'separate'
) -> NDArray:
    r"""
    Converts Tardis.dev data files into a format compatible with HftBacktest.

    Args:
        input_files: Input filenames for both incremental book and trades files,
                     e.g. ['incremental_book.csv', 'trades.csv'].
        output_filename: If provided, the converted data will be saved to the specified filename in ``npz`` format.
        buffer_size: Sets a preallocated row size for the buffer.
        base_latency: The value to be added to the feed latency.
                      See :func:`.correct_local_timestamp`.
        method: The method to correct reversed exchange timestamp events. See :func:`..validation.correct`.

    Returns:
        Converted data compatible with HftBacktest.
    """
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
                        int(cols[2]),
                        int(cols[3]),
                        1 if cols[5] == 'buy' else -1,
                        float(cols[6]),
                        float(cols[7])
                    ]
                    row_num += 1
                elif file_type == DEPTH:
                    if cols[4] == 'true':
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
                                int(cols[2]),
                                int(cols[3]),
                                1,
                                float(cols[6]),
                                float(cols[7])
                            ]
                            ss_bid_rn += 1
                        else:
                            ss_ask[ss_ask_rn] = [
                                DEPTH_SNAPSHOT_EVENT,
                                int(cols[2]),
                                int(cols[3]),
                                -1,
                                float(cols[6]),
                                float(cols[7])
                            ]
                            ss_ask_rn += 1
                    else:
                        if is_snapshot:
                            # End of the snapshot.
                            is_snapshot = False

                            # Add DEPTH_CLEAR_EVENT before refreshing the market depth by the snapshot.

                            ss_bid = ss_bid[:ss_bid_rn]
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
                            int(cols[2]),
                            int(cols[3]),
                            1 if cols[5] == 'bid' else -1,
                            float(cols[6]),
                            float(cols[7])
                        ]
                        row_num += 1
        sets.append(tmp[:row_num])

    print('Merging')
    data = sets[0]
    del sets[0]
    while len(sets) > 0:
        data = merge_on_local_timestamp(data, sets[0])
        del sets[0]

    data = correct(data, base_latency=base_latency, method=method)

    # Validate again.
    num_corr = validate_data(data)
    if num_corr < 0:
        raise ValueError

    if output_filename is not None:
        print('Saving to %s' % output_filename)
        np.savez(output_filename, data=data)

    return data
