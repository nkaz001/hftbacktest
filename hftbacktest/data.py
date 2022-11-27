import numpy as np
import pandas as pd
import sys
from numba import njit

from hftbacktest import COL_EVENT, COL_EXCH_TIMESTAMP, COL_LOCAL_TIMESTAMP, COL_PRICE, COL_QTY, TRADE_EVENT, \
    DEPTH_EVENT, DEPTH_CLEAR_EVENT, DEPTH_SNAPSHOT_EVENT


@njit
def _validate_data(values, tick_size=None, lot_size=None, err_bound=1e-8):
    num_reversed_exch_timestamp = 0
    prev_exch_timestamp = 0
    prev_local_timestamp = 0
    for row_num in range(len(values)):
        event = values[row_num, COL_EVENT]
        exch_timestamp = values[row_num, COL_EXCH_TIMESTAMP]
        local_timestamp = values[row_num, COL_LOCAL_TIMESTAMP]
        price = values[row_num, COL_PRICE]
        qty = values[row_num, COL_QTY]

        if event in [TRADE_EVENT, DEPTH_EVENT, DEPTH_CLEAR_EVENT, DEPTH_SNAPSHOT_EVENT]:
            if tick_size is not None:
                v = price / tick_size
                e = v - round(v)
                if e > err_bound:
                    print('found a row that price does not match tick size. row_num =', row_num)
                    return -1
            if lot_size is not None:
                v = qty / lot_size
                e = v - round(v)
                if e > err_bound:
                    print('found a row that qty does not match lot size. row_num =', row_num)
                    return -1

        if local_timestamp != -1 \
                and exch_timestamp != -1 \
                and exch_timestamp > local_timestamp \
                and event in [TRADE_EVENT, DEPTH_EVENT, DEPTH_CLEAR_EVENT, DEPTH_SNAPSHOT_EVENT]:
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
        if event in [DEPTH_EVENT, DEPTH_CLEAR_EVENT, DEPTH_SNAPSHOT_EVENT]:
            if local_timestamp == -1 or exch_timestamp == -1:
                print('All depth events must have valid timestamp.')
                return -1

    return num_reversed_exch_timestamp


def validate_data(df, tick_size=None, lot_size=None, err_bound=1e-8):
    num_reversed_exch_timestamp = _validate_data(df.values, tick_size, lot_size, err_bound)
    if num_reversed_exch_timestamp > 0:
        print('found %d rows that exch_timestamp is ahead of the previous exch_timestamp' % num_reversed_exch_timestamp)
    return num_reversed_exch_timestamp


@njit
def _correct_local_timestamp(values, base_latency):
    latency = sys.maxsize
    for row_num in range(len(values)):
        exch_timestamp = values[row_num, COL_EXCH_TIMESTAMP]
        local_timestamp = values[row_num, COL_LOCAL_TIMESTAMP]

        latency = min(latency, local_timestamp - exch_timestamp)

    if latency < 0:
        local_timestamp_offset = -latency + base_latency
        print('local_timestamp is ahead of exch_timestamp by', -latency)
        for row_num in range(len(values)):
            values[row_num, COL_LOCAL_TIMESTAMP] += local_timestamp_offset

    return values


def correct_local_timestamp(df, base_latency):
    df_corr = pd.DataFrame(_correct_local_timestamp(df.to_numpy(), base_latency), columns=df.columns)
    for col in df_corr.columns:
        df_corr[col] = df_corr[col].astype(df[col].dtype)
    return df_corr


@njit
def _correct_exch_timestamp(values, num_corr):
    row_size, col_size = values.shape
    corr = np.zeros((row_size + num_corr, col_size), np.float64)
    prev_exch_timestamp = 0
    out_row_num = 0
    for row_num in range(len(values)):
        exch_timestamp = values[row_num, COL_EXCH_TIMESTAMP]

        found = False
        if exch_timestamp < prev_exch_timestamp:
            for i in range(out_row_num - 1, -1, -1):
                if exch_timestamp < corr[i, COL_EXCH_TIMESTAMP] or found:
                    found = True
                    if i == 0 or \
                            (i > 0
                             and exch_timestamp >= corr[i - 1, COL_EXCH_TIMESTAMP]
                             and corr[i - 1, COL_EXCH_TIMESTAMP] != -1):
                        corr[i + 1:out_row_num + 1, :] = corr[i:out_row_num, :]
                        corr[i, :] = values[row_num, :]
                        corr[i, COL_LOCAL_TIMESTAMP] = -1
                        out_row_num += 1
                        break
            corr[out_row_num, :] = values[row_num, :]
            corr[out_row_num, COL_EXCH_TIMESTAMP] = -1
        else:
            corr[out_row_num, :] = values[row_num, :]
            prev_exch_timestamp = exch_timestamp
        out_row_num += 1
    return corr[:out_row_num, :]


def correct_exch_timestamp(df, num_corr):
    df_corr = pd.DataFrame(_correct_exch_timestamp(df.to_numpy(), num_corr), columns=df.columns)
    for col in df_corr.columns:
        df_corr[col] = df_corr[col].astype(df[col].dtype)
    return df_corr


def correct(df, base_latency, tick_size=None, lot_size=None, err_bound=1e-8):
    df = correct_local_timestamp(df, base_latency)
    num_corr = validate_data(df, tick_size=tick_size, lot_size=lot_size, err_bound=err_bound)
    if num_corr < 0:
        raise ValueError
    df = correct_exch_timestamp(df, num_corr)
    print('Correction is done.')
    return df
