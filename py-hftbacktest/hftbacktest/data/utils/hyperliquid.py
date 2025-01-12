import gzip
from typing import Optional
from hftbacktest.data.validation import correct_event_order, correct_local_timestamp, validate_event_order
import json
from hftbacktest.data.utils.difforderbooksnapshot import (
    DiffOrderBookSnapshot,
    CHANGED,
    INSERTED,
)
from ...types import (
    DEPTH_EVENT,
    TRADE_EVENT,
    BUY_EVENT,
    SELL_EVENT,
    event_dtype
)
import numpy as np
from numpy.typing import NDArray

def convert(
        input_filename: str,
        output_filename: Optional[str] = None,
        base_latency: float = 0,
        buffer_size: int = 100_000_000,
        exch_ts_multiplier: float = 1e6,
) -> NDArray:

    tmp = np.empty(buffer_size, event_dtype)
    row_num = 0
    timestamp_slice = 19

    with gzip.open(input_filename, 'r') as f:
        while True:
            line = f.readline()
            if not line:
                break

            local_ts = int(line[:timestamp_slice])
            message = json.loads(line[timestamp_slice + 1:])
            if message.get("channel") == "trades":
                trades_data = message.get("trades", [])
                for trade in trades_data:
                    exch_ts = trade.get("time") * exch_ts_multiplier

                    tmp[row_num] = (
                        TRADE_EVENT | (SELL_EVENT if trade.get("side") == "A" else BUY_EVENT), # trade initiator's side
                        exch_ts,
                        float(local_ts),
                        float(trade.get("px")),
                        float(trade.get("sz")),
                        0,
                        0,
                        0
                    )
                    row_num += 1

            elif message.get("channel") == "l2Book":
                depth_data = message.get("data", {})

                exch_ts = depth_data.get("time") * exch_ts_multiplier
                levels = depth_data.get("levels")
                bids = levels[0]
                asks = levels[1]

                diff = DiffOrderBookSnapshot(len(bids), 0.1, 0.00001)

                bid_px = np.array([float(b["px"]) for b in bids])
                bid_qty = np.array([float(b["sz"]) for b in bids])
                ask_px = np.array([float(a["px"]) for a in asks])
                ask_qty = np.array([float(a["sz"]) for a in asks])

                bid, ask, bid_del, ask_del = diff.snapshot(bid_px, bid_qty, ask_px, ask_qty)

                for entry in bid:
                    if entry[2] == INSERTED or entry[2] == CHANGED:
                        tmp[row_num] = (
                            DEPTH_EVENT | BUY_EVENT,
                            exch_ts,
                            float(local_ts),
                            entry[0],
                            entry[1],
                            0,
                            0,
                            0
                        )
                        row_num += 1
                for entry in ask:
                    if entry[2] == INSERTED or entry[2] == CHANGED:
                        tmp[row_num] = (
                            DEPTH_EVENT | SELL_EVENT,
                            exch_ts,
                            float(local_ts),
                            entry[0],
                            entry[1],
                            0,
                            0,
                            0
                        )
                        row_num += 1
                for entry in bid_del:
                        tmp[row_num] = (
                            BUY_EVENT | DEPTH_EVENT,
                            exch_ts,
                            float(local_ts),
                            entry[0],
                            0,
                            0,
                            0,
                            0
                        )
                        row_num += 1
                for entry in ask_del:
                        tmp[row_num] = (
                            SELL_EVENT | DEPTH_EVENT,
                            exch_ts,
                            float(local_ts),
                            entry[0],
                            0,
                            0,
                            0,
                            0
                        )
                        row_num += 1

    tmp = tmp[:row_num]

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
