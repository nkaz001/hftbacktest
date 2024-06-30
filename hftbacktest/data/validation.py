import sys
from typing import Optional, Union, Literal

import numpy as np
import pandas as pd
from numba import njit
from numpy.typing import NDArray
from pandas import DataFrame

from ..reader import (
    COL_EVENT,
    COL_EXCH_TIMESTAMP,
    COL_LOCAL_TIMESTAMP,
    COL_SIDE,
    COL_PRICE,
    COL_QTY,
    TRADE_EVENT,
    DEPTH_EVENT,
    DEPTH_CLEAR_EVENT,
    DEPTH_SNAPSHOT_EVENT,
    EXCH_EVENT,
    LOCAL_EVENT,
    BUY,
    SELL
)


@njit
def correct_local_timestamp(data: np.ndarray, base_latency: float) -> np.ndarray:
    r"""
    Adjusts the local timestamp if the feed latency is negative by offsetting the maximum negative latency value as
    follows:

    .. code-block::

        feed_latency = local_timestamp - exch_timestamp
        adjusted_local_timestamp = local_timestamp + min(feed_latency, 0) + base_latency

    Args:
        data: Data to be corrected.
        base_latency: Due to discrepancies in system time between the exchange and the local machine, latency may be
                      measured inaccurately, resulting in negative latency values. The conversion process automatically
                      adjusts for positive latency but may still produce zero latency cases. By adding ``base_latency``,
                      more realistic values can be obtained. Unit should be the same as the feed data's timestamp unit.

    Returns:
        Adjusted data with corrected timestamps
    """

    latency = sys.maxsize
    for row_num in range(len(data)):
        exch_timestamp = data[row_num, COL_EXCH_TIMESTAMP]
        local_timestamp = data[row_num, COL_LOCAL_TIMESTAMP]

        latency = min(latency, local_timestamp - exch_timestamp)

    if latency < 0:
        local_timestamp_offset = -latency + base_latency
        print('local_timestamp is ahead of exch_timestamp by', -latency)
        for row_num in range(len(data)):
            data[row_num, COL_LOCAL_TIMESTAMP] += local_timestamp_offset

    return data


@njit
def correct_event_order(
        data: np.ndarray,
        sorted_exch_index: np.ndarray,
        sorted_local_index: np.ndarray,
) -> np.ndarray:
    r"""
    Corrects exchange timestamps that are reversed by splitting each row into separate events, ordered by both exchange
    and local timestamps, through duplication. See ``data`` for details.

    Args:
        data: Data to be reordered.
        sorted_exch_index: Index of data sorted by exchange timestamp.
        sorted_local_index: Index of data sorted by local timestamp.

    Returns:
        Adjusted data with corrected exchange timestamps.
    """
    sorted_final = np.zeros((data.shape[0] * 2, data.shape[1]), np.float64)

    out_rn = 0
    exch_rn = 0
    local_rn = 0
    while True:
        sorted_exch = data[sorted_exch_index[exch_rn]]
        sorted_local = data[sorted_local_index[local_rn]]
        if (
                exch_rn < len(data)
                and local_rn < len(data)
                and sorted_exch[COL_EXCH_TIMESTAMP] == sorted_local[COL_EXCH_TIMESTAMP]
                and sorted_exch[COL_LOCAL_TIMESTAMP] == sorted_local[COL_LOCAL_TIMESTAMP]
        ):
            assert sorted_exch[COL_EVENT] == sorted_local[COL_EVENT]
            assert sorted_exch[COL_PRICE] == sorted_local[COL_PRICE]
            assert sorted_exch[COL_QTY] == sorted_local[COL_QTY]

            sorted_final[out_rn] = sorted_exch[:]
            sorted_final[out_rn, COL_EVENT] = int(sorted_final[out_rn, COL_EVENT]) | EXCH_EVENT | LOCAL_EVENT

            out_rn += 1
            exch_rn += 1
            local_rn += 1
        elif ((
                exch_rn < len(data)
                and local_rn < len(data)
                and sorted_exch[COL_EXCH_TIMESTAMP] == sorted_local[COL_EXCH_TIMESTAMP]
                and sorted_exch[COL_LOCAL_TIMESTAMP] < sorted_local[COL_LOCAL_TIMESTAMP]
        ) or (
                exch_rn < len(data)
                and sorted_exch[COL_EXCH_TIMESTAMP] < sorted_local[COL_EXCH_TIMESTAMP]
        )):
            # exchange
            sorted_final[out_rn] = sorted_exch[:]
            sorted_final[out_rn, COL_EVENT] = int(sorted_final[out_rn, COL_EVENT]) | EXCH_EVENT

            out_rn += 1
            exch_rn += 1
        elif ((
                exch_rn < len(data)
                and local_rn < len(data)
                and sorted_exch[COL_EXCH_TIMESTAMP] == sorted_local[COL_EXCH_TIMESTAMP]
                and sorted_exch[COL_LOCAL_TIMESTAMP] > sorted_local[COL_LOCAL_TIMESTAMP]
        ) or (
                local_rn < len(data)
        )):
            # local
            sorted_final[out_rn] = sorted_local[:]
            sorted_final[out_rn, COL_EVENT] = int(sorted_final[out_rn, COL_EVENT]) | LOCAL_EVENT

            out_rn += 1
            local_rn += 1
        elif exch_rn < len(data):
            # exchange
            sorted_final[out_rn] = sorted_exch[:]
            sorted_final[out_rn, COL_EVENT] = int(sorted_final[out_rn, COL_EVENT]) | EXCH_EVENT

            out_rn += 1
            exch_rn += 1
        else:
            assert exch_rn == len(data)
            assert local_rn == len(data)
            break
    return sorted_final[:out_rn]


def validate_event_order(data: np.ndarray) -> None:
    r"""
    Validates that the order of events is correct.

    Args:
        data: event structured array.
    """
    exch_ev = data['ev'] & EXCH_EVENT == EXCH_EVENT
    local_ev = data['ev'] & LOCAL_EVENT == LOCAL_EVENT
    if np.sum(np.diff(data['exch_ts'][exch_ev]) < 0) > 0:
        raise ValueError('exchange events are out of order.')
    if np.sum(np.diff(data['local_ts'][local_ev]) < 0) > 0:
        raise ValueError('local events are out of order.')
