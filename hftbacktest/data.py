import sys

import numpy as np
import pandas as pd
from numba import njit

from .reader import COL_EVENT, COL_EXCH_TIMESTAMP, COL_LOCAL_TIMESTAMP, COL_PRICE, COL_QTY, TRADE_EVENT, \
    DEPTH_EVENT, DEPTH_CLEAR_EVENT, DEPTH_SNAPSHOT_EVENT


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

        # Rows of the same event type must be correctly ordered.

        # All depth events must have valid timestamp.
        if event in [
            DEPTH_EVENT,
            DEPTH_CLEAR_EVENT,
            DEPTH_SNAPSHOT_EVENT
        ]:
            if local_timestamp == -1 or exch_timestamp == -1:
                print('All depth events must have valid timestamp.')
                return -1

    return num_reversed_exch_timestamp


def validate_data(
        data,
        tick_size=None,
        lot_size=None,
        err_bound=1e-8
):
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


def correct_local_timestamp(data, base_latency):
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
        if exch_timestamp < prev_exch_timestamp:
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
            prev_exch_timestamp = exch_timestamp
        out_row_num += 1
    return corr[:out_row_num, :]


def correct_exch_timestamp(data, num_corr):
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


def correct_exch_timestamp_adjust(data):
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
        data,
        base_latency,
        tick_size=None,
        lot_size=None,
        err_bound=1e-8,
        method='separate'
):
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
def merge_on_local_timestamp(a, b):
    a_shape = a.shape
    b_shape = b.shape
    assert a_shape[1] == b_shape[1]
    tmp = np.empty((a_shape[0] + b_shape[0], a_shape[1]), np.float64)
    i = 0
    j = 0
    k = 0
    while True:
        if i < len(a) and j < len(b):
            if a[i, 2] < b[j, 2]:
                tmp[k] = a[i]
                i += 1
                k += 1
            elif a[i, 2] > b[j, 2]:
                tmp[k] = b[j]
                j += 1
                k += 1
            elif a[i, 1] < b[j, 1]:
                tmp[k] = a[i]
                i += 1
                k += 1
            else:
                tmp[k] = b[j]
                j += 1
                k += 1
        elif i < len(a):
            tmp[k] = a[i]
            i += 1
            k += 1
        elif j < len(b):
            tmp[k] = b[j]
            j += 1
            k += 1
        else:
            break
    return tmp[:k]
