from typing import Literal

import databento as db
import numpy as np
import polars as pl
from numpy.typing import NDArray

from ..validation import correct_event_order, validate_event_order, correct_local_timestamp
from ...types import (
    event_dtype,
    BUY_EVENT,
    SELL_EVENT,
    DEPTH_CLEAR_EVENT,
    TRADE_EVENT,
    ADD_ORDER_EVENT,
    CANCEL_ORDER_EVENT,
    MODIFY_ORDER_EVENT,
    FILL_EVENT,
)


def convert(
        input_file: str,
        symbol: str | None,
        output_filename: str | None = None,
        base_latency: float = 0,
        file_type: Literal['mbo'] = 'mbo'
) -> NDArray:
    r"""
    Converts a DataBento L3 Market-By-Order data file into a format compatible with HftBacktest.

    Args:
        input_file: DataBento's DBN file. e.g. *.mbo.dbn.zst
        symbol: Specify the symbol to process in the given file. If the file contains multiple symbols, the symbol
                should be provided; otherwise, the output file will contain mixed symbols.
        output_filename: If provided, the converted data will be saved to the specified filename in ``npz`` format.
        base_latency: The value to be added to the feed latency.
                      See :func:`.correct_local_timestamp`.
        file_type: Currently, only 'mbo' is supported.
    Returns:
        Converted data compatible with HftBacktest.
    """

    if file_type != 'mbo':
        raise ValueError(f'{file_type} is unsupported')

    with open(input_file, 'rb') as f:
        stored_data = db.DBNStore.from_bytes(f)

    # Convert to dataframe
    pd_df = stored_data.to_df()
    df = pl.DataFrame(pd_df).with_columns(
        pl.Series('ts_recv', pd_df.index)
    )

    if symbol is not None:
        df = df.filter(
            pl.col('symbol') == symbol
        )

    df = df.select(['ts_event', 'action', 'side', 'price', 'size', 'order_id', 'flags', 'ts_recv'])

    tmp = np.empty(len(df), event_dtype)

    adjust_ts = False

    rn = 0
    for (ts_event, action, side, price, size, order_id, flags, ts_recv) in df.iter_rows():
        exch_ts = int(ts_event.timestamp() * 1_000_000_000)
        local_ts = int(ts_recv.timestamp() * 1_000_000_000)

        if action == 'A':
            ev = ADD_ORDER_EVENT
        elif action == 'C':
            ev = CANCEL_ORDER_EVENT
        elif action == 'M':
            ev = MODIFY_ORDER_EVENT
        elif action == 'R':
            ev = DEPTH_CLEAR_EVENT
        elif action == 'T':
            ev = TRADE_EVENT
        elif action == 'F':
            ev = FILL_EVENT
        else:
            raise ValueError(action)

        if side == 'B':
            ev |= BUY_EVENT
        elif side == 'A':
            ev |= SELL_EVENT
        elif side == 'N':
            pass
        else:
            raise ValueError(side)

        # The timestamp of the first clear event is significantly earlier than the next feed message (start of session),
        # causing an unnecessary delay by elapse method in reaching the session's start time.
        # To resolve this, the clear event's timestamp is adjusted to match the first session message.
        if rn == 0 and ev & DEPTH_CLEAR_EVENT == DEPTH_CLEAR_EVENT:
            adjust_ts = True
        if rn == 1 and adjust_ts:
            tmp[0]['exch_ts'] = exch_ts
            tmp[0]['local_ts'] = local_ts

        tmp[rn] = (ev, exch_ts, local_ts, price, size, order_id, flags, 0)

        rn += 1

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
