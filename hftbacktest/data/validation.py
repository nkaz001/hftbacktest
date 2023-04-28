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
    COL_PRICE,
    COL_QTY,
    TRADE_EVENT,
    DEPTH_EVENT,
    DEPTH_CLEAR_EVENT,
    DEPTH_SNAPSHOT_EVENT
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
