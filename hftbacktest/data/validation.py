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


Data = Union[NDArray, DataFrame]


@njit
def _validate_data(
        data,
        tick_size=None,
        lot_size=None,
        err_bound=1e-8
):
    num_reversed_exch_timestamp = 0
    prev_exch_timestamp = 0
    prev_local_timestamp = 0
    for row_num in range(len(data)):
        event = data[row_num, COL_EVENT]
        exch_timestamp = data[row_num, COL_EXCH_TIMESTAMP]
        local_timestamp = data[row_num, COL_LOCAL_TIMESTAMP]
        price = data[row_num, COL_PRICE]
        qty = data[row_num, COL_QTY]

        if event in [
            TRADE_EVENT,
            DEPTH_EVENT,
            DEPTH_CLEAR_EVENT,
            DEPTH_SNAPSHOT_EVENT
        ]:
            if tick_size is not None:
                v = price / tick_size
                e = abs(v - round(v))
                if e > err_bound:
                    print('found a row that price does not match tick size. row_num =', row_num)
                    return -1
            if lot_size is not None:
                v = qty / lot_size
                e = abs(v - round(v))
                if e > err_bound:
                    print('found a row that qty does not match lot size. row_num =', row_num)
                    return -1

        if local_timestamp != -1 \
                and exch_timestamp != -1 \
                and exch_timestamp > local_timestamp \
                and event in [
                    TRADE_EVENT,
                    DEPTH_EVENT,
                    DEPTH_CLEAR_EVENT,
                    DEPTH_SNAPSHOT_EVENT
                ]:
            print('found a row that local_timestamp is ahead of exch_timestamp. row_num =', row_num)
            return -1
        if local_timestamp != -1 and prev_local_timestamp > local_timestamp:
            print('found a row that local_timestamp is ahead of the previous local_timestamp. row_num =', row_num)
            return -1

        if exch_timestamp != -1:
            if exch_timestamp < prev_exch_timestamp:
                num_reversed_exch_timestamp += 1
            else:
                prev_exch_timestamp = exch_timestamp

        if local_timestamp != -1:
            prev_local_timestamp = local_timestamp

    return num_reversed_exch_timestamp


def validate_data(
        data: Data,
        tick_size: Optional[float] = None,
        lot_size: Optional[float] = None,
        err_bound: float = 1e-8
) -> int:
    r"""
    Validates the specified data for the following aspects, excluding user events. Validation results will be printed out:

        - Ensures data's price aligns with tick_size.
        - Ensures data's quantity aligns with lot_size.
        - Ensures data's local timestamp is ordered.
        - Ensures data's exchange timestamp is ordered.

    Args:
        data: Data to be validated.
        tick_size: Minimum price increment for the given asset.
        lot_size: Minimum order quantity for the given asset.
        err_bound: Error bound used to verify if the specified ``tick_size`` or ``lot_size`` aligns with the price and
                   quantity.

    Returns:
        The number of rows with reversed exchange timestamps.
    """

    if isinstance(data, pd.DataFrame):
        num_reversed_exch_timestamp = _validate_data(data.to_numpy(), tick_size, lot_size, err_bound)
    elif isinstance(data, np.ndarray):
        num_reversed_exch_timestamp = _validate_data(data, tick_size, lot_size, err_bound)
    else:
        raise ValueError('Unsupported data type')
    if num_reversed_exch_timestamp > 0:
        print('found %d rows that exch_timestamp is ahead of the previous exch_timestamp' % num_reversed_exch_timestamp)
    return num_reversed_exch_timestamp


@njit
def _correct_local_timestamp(data, base_latency):
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


def correct_local_timestamp(data: Data, base_latency: float) -> Data:
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
    if isinstance(data, pd.DataFrame):
        df_corr = pd.DataFrame(_correct_local_timestamp(data.to_numpy(), base_latency), columns=data.columns)
        for col in df_corr.columns:
            df_corr[col] = df_corr[col].astype(data[col].dtype)
        return df_corr
    elif isinstance(data, np.ndarray):
        return _correct_local_timestamp(data, base_latency)
    else:
        raise ValueError('Unsupported data type')


@njit
def _correct_exch_timestamp(data, num_corr):
    row_size, col_size = data.shape
    corr = np.zeros((row_size + num_corr, col_size), np.float64)
    prev_exch_timestamp = 0
    out_row_num = 0
    for row_num in range(len(data)):
        exch_timestamp = data[row_num, COL_EXCH_TIMESTAMP]
        event = data[row_num, COL_EVENT]
        if exch_timestamp < prev_exch_timestamp and event in [
            TRADE_EVENT,
            DEPTH_EVENT,
            DEPTH_CLEAR_EVENT,
            DEPTH_SNAPSHOT_EVENT
        ]:
            # This new row should be inserted ahead.
            found = False
            for i in range(out_row_num - 1, -1, -1):
                if exch_timestamp < corr[i, COL_EXCH_TIMESTAMP] or found:
                    found = True
                    if i == 0 or \
                            (i > 0
                             and exch_timestamp >= corr[i - 1, COL_EXCH_TIMESTAMP] != -1):
                        corr[i + 1:out_row_num + 1, :] = corr[i:out_row_num, :]
                        corr[i, :] = data[row_num, :]
                        corr[i, COL_LOCAL_TIMESTAMP] = -1
                        out_row_num += 1
                        break
            corr[out_row_num, :] = data[row_num, :]
            corr[out_row_num, COL_EXCH_TIMESTAMP] = -1
        else:
            corr[out_row_num, :] = data[row_num, :]
            if exch_timestamp > 0:
                prev_exch_timestamp = exch_timestamp
        out_row_num += 1
    return corr[:out_row_num, :]


def correct_exch_timestamp(data: Data, num_corr: int) -> Data:
    r"""
    Corrects exchange timestamps that are reversed by splitting each row into separate events, ordered by both exchange
    and local timestamps, through duplication. See ``data`` for details.

    Args:
        data: Data to be corrected.
        num_corr: The number of rows to be corrected.

    Returns:
        Adjusted data with corrected exchange timestamps.
    """

    if isinstance(data, pd.DataFrame):
        df_corr = pd.DataFrame(_correct_exch_timestamp(data.to_numpy(), num_corr), columns=data.columns)
        for col in df_corr.columns:
            df_corr[col] = df_corr[col].astype(data[col].dtype)
        return df_corr
    elif isinstance(data, np.ndarray):
        return _correct_exch_timestamp(data, num_corr)
    else:
        raise ValueError('Unsupported data type')


@njit
def _correct_exch_timestamp_adjust(data):
    # Sort by exch_timestamp
    i = np.argsort(data[:, COL_EXCH_TIMESTAMP])
    sorted_data = data[i]
    # Adjust local_timestamp in reverse order to have a value equal to or greater than the previous local_timestamp.
    for row_num in range(1, len(sorted_data)):
        if sorted_data[row_num, COL_LOCAL_TIMESTAMP] < sorted_data[row_num - 1, COL_LOCAL_TIMESTAMP]:
            sorted_data[row_num, COL_LOCAL_TIMESTAMP] = sorted_data[row_num - 1, COL_LOCAL_TIMESTAMP]
    return sorted_data


def correct_exch_timestamp_adjust(data: Data) -> Data:
    r"""
    Corrects reversed exchange timestamps by adjusting the local timestamp value for proper ordering. It sorts the data
    by exchange timestamp and fixes out-of-order local timestamps by setting their value to the previous value, ensuring
    correct ordering.

    Args:
        data: Data to be corrected.

    Returns:
        Adjusted data with corrected exchange timestamps.
    """

    if isinstance(data, pd.DataFrame):
        df_corr = pd.DataFrame(_correct_exch_timestamp_adjust(data.to_numpy()), columns=data.columns)
        for col in df_corr.columns:
            df_corr[col] = df_corr[col].astype(data[col].dtype)
        return df_corr
    elif isinstance(data, np.ndarray):
        return _correct_exch_timestamp_adjust(data)
    else:
        raise ValueError('Unsupported data type')


def correct(
        data: Data,
        base_latency: float,
        tick_size: Optional[float] = None,
        lot_size: Optional[float] = None,
        err_bound: float = 1e-8,
        method: Literal['separate', 'adjust'] = 'separate'
) -> Data:
    r"""
    Validates the specified data and automatically corrects negative latency and unordered rows.
    See :func:`.validate_data`, :func:`.correct_local_timestamp`, :func:`.correct_exch_timestamp`, and
    :func:`.correct_exch_timestamp_adjust`.

    Args:
        data: Data to be checked and corrected.
        base_latency: The value to be added to the feed latency. See :func:`.correct_local_timestamp`.
        tick_size: Minimum price increment for the specified data.
        lot_size: Minimum order quantity for the specified data.
        err_bound: Error bound used to verify if the specified ``tick_size`` or ``lot_size`` aligns with the price and
                   quantity.
        method: The method to correct reversed exchange timestamp events.

                    - ``separate``: Use :func:`.correct_local_timestamp`.
                    - ``adjust``: Use :func:`.correct_exch_timestamp_adjust`.

    Returns:
        Corrected data
    """
    data = correct_local_timestamp(data, base_latency)
    num_corr = validate_data(
        data,
        tick_size=tick_size,
        lot_size=lot_size,
        err_bound=err_bound
    )
    if num_corr < 0:
        raise ValueError
    if method == 'separate':
        data = correct_exch_timestamp(data, num_corr)
    elif method == 'adjust':
        data = correct_exch_timestamp_adjust(data)
    else:
        raise ValueError('Invalid method')
    print('Correction is done.')
    return data


@njit
def correct_event_order(
        sorted_exch: np.ndarray,
        sorted_local: np.ndarray,
        add_exch_local_ev: bool
) -> np.ndarray:
    r"""
    Corrects exchange timestamps that are reversed by splitting each row into separate events, ordered by both exchange
    and local timestamps, through duplication. See ``data`` for details.

    Args:
        sorted_exch: Data sorted by exchange timestamp.
        sorted_local: Data sorted by local timestamp.
        add_exch_local_ev: If this is set to True, `EXCH_EVENT` and `LOCAL_EVENT` flags will be added to the event field
                           based on the validity of each timestamp.

    Returns:
        Adjusted data with corrected exchange timestamps.
    """
    sorted_final = np.zeros((sorted_exch.shape[0] * 2, sorted_exch.shape[1]), np.float64)

    out_rn = 0
    exch_rn = 0
    local_rn = 0
    while True:
        if (
                exch_rn < len(sorted_exch)
                and local_rn < len(sorted_local)
                and sorted_exch[exch_rn, COL_EXCH_TIMESTAMP] == sorted_local[local_rn, COL_EXCH_TIMESTAMP]
                and sorted_exch[exch_rn, COL_LOCAL_TIMESTAMP] == sorted_local[local_rn, COL_LOCAL_TIMESTAMP]
        ):
            assert sorted_exch[exch_rn, COL_EVENT] == sorted_local[local_rn, COL_EVENT]
            assert sorted_exch[exch_rn, COL_PRICE] == sorted_local[local_rn, COL_PRICE]
            assert sorted_exch[exch_rn, COL_QTY] == sorted_local[local_rn, COL_QTY]

            sorted_final[out_rn] = sorted_exch[exch_rn]
            if add_exch_local_ev:
                sorted_final[out_rn, COL_EVENT] = int(
                    sorted_final[out_rn, COL_EVENT]) | EXCH_EVENT | LOCAL_EVENT

            out_rn += 1
            exch_rn += 1
            local_rn += 1
        elif ((
                exch_rn < len(sorted_exch)
                and local_rn < len(sorted_local)
                and sorted_exch[exch_rn, COL_EXCH_TIMESTAMP] == sorted_local[local_rn, COL_EXCH_TIMESTAMP]
                and sorted_exch[exch_rn, COL_LOCAL_TIMESTAMP] < sorted_local[local_rn, COL_LOCAL_TIMESTAMP]
        ) or (
                exch_rn < len(sorted_exch)
                and sorted_exch[exch_rn, COL_EXCH_TIMESTAMP] < sorted_local[local_rn, COL_EXCH_TIMESTAMP]
        )):
            # exchange
            sorted_final[out_rn] = sorted_exch[exch_rn]
            if add_exch_local_ev:
                sorted_final[out_rn, COL_EVENT] = int(sorted_final[out_rn, COL_EVENT]) | EXCH_EVENT
            else:
                sorted_final[out_rn, COL_LOCAL_TIMESTAMP] = -1

            out_rn += 1
            exch_rn += 1
        elif ((
                exch_rn < len(sorted_exch)
                and local_rn < len(sorted_local)
                and sorted_exch[exch_rn, COL_EXCH_TIMESTAMP] == sorted_local[local_rn, COL_EXCH_TIMESTAMP]
                and sorted_exch[exch_rn, COL_LOCAL_TIMESTAMP] > sorted_local[local_rn, COL_LOCAL_TIMESTAMP]
        ) or (
                local_rn < len(sorted_local)
        )):
            # local
            sorted_final[out_rn] = sorted_local[local_rn]
            if add_exch_local_ev:
                sorted_final[out_rn, COL_EVENT] = int(sorted_final[out_rn, COL_EVENT]) | LOCAL_EVENT
            else:
                sorted_final[out_rn, COL_EXCH_TIMESTAMP] = -1

            out_rn += 1
            local_rn += 1
        else:
            assert exch_rn == len(sorted_exch)
            assert local_rn == len(sorted_local)
            break
    return sorted_final[:out_rn]


def convert_to_struct_arr(data: np.ndarray, add_exch_local_ev: bool = True) -> np.ndarray:
    r"""
    Converts the 2D ndarray currently used in Python hftbacktest into the structured array that can be used in Rust
    hftbacktest.

    Args:
        data: 2D ndarray to be converted.
        add_exch_local_ev: If this is set to True, `EXCH_EVENT` and `LOCAL_EVENT` flags will be added to the 'ev' event
                           field based on the validity of each timestamp. Set to True only when converting existing data
                           into the new format.

    Returns:
        Converted structured array.
    """
    ev = data[:, COL_EVENT].astype(int)
    if add_exch_local_ev:
        valid_exch_ts = data[:, COL_EXCH_TIMESTAMP] != -1
        valid_local_ts = data[:, COL_LOCAL_TIMESTAMP] != -1
        ev[valid_exch_ts] |= EXCH_EVENT
        ev[valid_local_ts] |= LOCAL_EVENT

    buy = data[:, COL_SIDE] == 1
    sell = data[:, COL_SIDE] == -1
    ev[buy] |= BUY
    ev[sell] |= SELL

    tup_list = [
        (
            ev[rn],
            data[rn, COL_EXCH_TIMESTAMP],
            data[rn, COL_LOCAL_TIMESTAMP],
            data[rn, COL_PRICE],
            data[rn, COL_QTY]
        ) for rn in range(len(data))
    ]

    return np.array(
        tup_list,
        dtype=[('ev', 'i8'), ('exch_ts', 'i8'), ('local_ts', 'i8'), ('px', 'f4'), ('qty', 'f4')]
    )


def convert_from_struct_arr(data: np.ndarray) -> np.ndarray:
    r"""
    Converts the structured array that can be used in Rust hftbacktest into the 2D ndarray currently used in Python
    hftbacktest.

    Args:
        data: the structured array to be converted.

    Returns:
        Converted 2D ndarray.
    """

    out = np.empty((len(data), 6), np.float64)
    for row in range(len(data)):
        ev = data[row][0]

        if ev & EXCH_EVENT == EXCH_EVENT:
            out[row, COL_EXCH_TIMESTAMP] = data[row][1]
        else:
            out[row, COL_EXCH_TIMESTAMP] = -1

        if ev & LOCAL_EVENT == LOCAL_EVENT:
            out[row, COL_LOCAL_TIMESTAMP] = data[row][2]
        else:
            out[row, COL_LOCAL_TIMESTAMP] = -1

        if ev & BUY == BUY:
            out[row, COL_SIDE] = 1
        elif ev & SELL == SELL:
            out[row, COL_SIDE] = -1
        else:
            out[row, COL_SIDE] = 0

        out[row, COL_PRICE] = data[row][3]
        out[row, COL_QTY] = data[row][4]
        out[row, COL_EVENT] = ev & 0xFF
    return out
