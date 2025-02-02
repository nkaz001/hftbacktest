from typing import Optional
from hftbacktest.data.validation import correct_event_order, correct_local_timestamp, validate_event_order
import gzip
import numpy as np
import json
from numpy.typing import NDArray

from ...types import (
    DEPTH_EVENT,
    DEPTH_SNAPSHOT_EVENT,
    TRADE_EVENT,
    BUY_EVENT,
    SELL_EVENT,
    event_dtype
)

def convert(
        input_filename: str,
        output_filename: Optional[str] = None,
        base_latency: float = 0,
        buffer_size: int = 100_000_000,
        exch_ts_multiplier: float = 1e6,
) -> NDArray:
    r"""
    Converts raw MEXC spot feed stream file into a format compatible with HftBacktest.
    If you encounter an ``IndexError`` due to an out-of-bounds, try increasing the ``buffer_size``.

    **File Format:**

    .. code-block::

        local_timestamp raw_stream
        1736682893942252094 {"c":"spot@public.limit.depth.v3.api@SOLUSDT@10","d":{"bids":[{"p":"186.69","v":"46.87"},{"p":"186.68","v":"451.23"},{"p":"186.67","v":"561.54"},{"p":"186.65","v":"25.24"},{"p":"186.64","v":"300.87"},{"p":"186.63","v":"291.28"},{"p":"186.62","v":"116.40"},{"p":"186.61","v":"141.14"},{"p":"186.60","v":"173.39"},{"p":"186.59","v":"422.41"}],"asks":[{"p":"186.70","v":"340.03"},{"p":"186.71","v":"357.15"},{"p":"186.72","v":"259.61"},{"p":"186.74","v":"144.87"},{"p":"186.75","v":"68.77"},{"p":"186.76","v":"127.24"},{"p":"186.77","v":"56.46"},{"p":"186.78","v":"148.36"},{"p":"186.79","v":"116.03"},{"p":"186.80","v":"319.97"}],"e":"spot@public.limit.depth.v3.api","r":"4474505330"},"s":"SOLUSDT","t":1736682893806}
        1736682894188268890 {"c":"spot@public.increase.depth.v3.api@SOLUSDT","d":{"asks":[{"p":"187.57","v":"0.00"}],"e":"spot@public.increase.depth.v3.api","r":"4474505331"},"s":"SOLUSDT","t":1736682894061}
        1736682894303489257 {"c":"spot@public.increase.depth.v3.api@SOLUSDT","d":{"asks":[{"p":"187.26","v":"0.22"}],"e":"spot@public.increase.depth.v3.api","r":"4474505332"},"s":"SOLUSDT","t":1736682894171}
        1736682894442732943 {"c":"spot@public.limit.depth.v3.api@SOLUSDT@10","d":{"bids":[{"p":"186.69","v":"46.87"},{"p":"186.68","v":"451.23"},{"p":"186.67","v":"561.54"},{"p":"186.65","v":"25.24"},{"p":"186.64","v":"300.87"},{"p":"186.63","v":"291.28"},{"p":"186.62","v":"116.40"},{"p":"186.61","v":"141.14"},{"p":"186.60","v":"173.39"},{"p":"186.59","v":"422.41"}],"asks":[{"p":"186.70","v":"340.03"},{"p":"186.71","v":"357.15"},{"p":"186.72","v":"259.61"},{"p":"186.74","v":"144.87"},{"p":"186.75","v":"68.77"},{"p":"186.76","v":"127.24"},{"p":"186.77","v":"56.46"},{"p":"186.78","v":"148.36"},{"p":"186.79","v":"116.03"},{"p":"186.80","v":"319.97"}],"e":"spot@public.limit.depth.v3.api","r":"4474505332"},"s":"SOLUSDT","t":1736682894315}
        1736682894932699002 {"c":"spot@public.limit.depth.v3.api@SOLUSDT@10","d":{"bids":[{"p":"186.69","v":"46.87"},{"p":"186.68","v":"451.23"},{"p":"186.67","v":"561.54"},{"p":"186.65","v":"25.24"},{"p":"186.64","v":"300.87"},{"p":"186.63","v":"291.28"},{"p":"186.62","v":"116.40"},{"p":"186.61","v":"141.14"},{"p":"186.60","v":"173.39"},{"p":"186.59","v":"422.41"}],"asks":[{"p":"186.70","v":"340.03"},{"p":"186.71","v":"357.15"},{"p":"186.72","v":"259.61"},{"p":"186.74","v":"144.87"},{"p":"186.75","v":"68.77"},{"p":"186.76","v":"127.24"},{"p":"186.77","v":"56.46"},{"p":"186.78","v":"148.36"},{"p":"186.79","v":"116.03"},{"p":"186.80","v":"319.97"}],"e":"spot@public.limit.depth.v3.api","r":"4474505332"},"s":"SOLUSDT","t":1736682894807}
        1736682895313594980 {"c":"spot@public.increase.depth.v3.api@SOLUSDT","d":{"asks":[{"p":"186.70","v":"339.74"}],"e":"spot@public.increase.depth.v3.api","r":"4474505333"},"s":"SOLUSDT","t":1736682895186}
        1736682895377438738 {"c":"spot@public.increase.depth.v3.api@SOLUSDT","d":{"bids":[{"p":"186.20","v":"0.00"}],"e":"spot@public.increase.depth.v3.api","r":"4474505334"},"s":"SOLUSDT","t":1736682895251}

    Args:
        input_filename: Input filename with path.
        output_filename: If provided, the converted data will be saved to the specified filename in ``npz`` format.
        base_latency: The value to be added to the feed latency.
                      See :func:`.correct_local_timestamp`.
        buffer_size: Sets a preallocated row size for the buffer.

    Returns:
        Converted data compatible with HftBacktest.
    """

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
            if message.get("id") == 0:
                continue

            exch_ts = message.get("t") * exch_ts_multiplier
            dataset = message.get("c")
            if dataset.startswith("spot@public.increase.depth.v3.api"):
                depth_chg = message.get("d")
                bids = depth_chg.get("bids", [])
                asks = depth_chg.get("asks", [])

                for bid in bids:
                    vol = bid.get("v")
                    if vol == 0:
                        tmp[row_num] = (
                            DEPTH_EVENT | BUY_EVENT,
                            exch_ts,
                            float(local_ts),
                            bid.get("p"),
                            0,
                            0,
                            0,
                            0
                        )
                        row_num += 1
                    else:
                        tmp[row_num] = (
                            DEPTH_EVENT | BUY_EVENT,
                            exch_ts,
                            float(local_ts),
                            bid.get("p"),
                            bid.get("v"),
                            0,
                            0,
                            0
                        )
                        row_num += 1

                for ask in asks:
                    vol = ask.get("v")
                    if vol == 0:
                        tmp[row_num] = (
                            DEPTH_EVENT | SELL_EVENT,
                            exch_ts,
                            float(local_ts),
                            ask.get("p"),
                            0,
                            0,
                            0,
                            0
                        )
                        row_num += 1
                    else:
                        tmp[row_num] = (
                            DEPTH_EVENT | SELL_EVENT,
                            exch_ts,
                            float(local_ts),
                            ask.get("p"),
                            ask.get("v"),
                            0,
                            0,
                            0
                        )
                        row_num += 1

            elif dataset.startswith("spot@public.limit.depth.v3.api"):
                depth_chg = message.get("d")
                bids = depth_chg.get("bids", [])
                asks = depth_chg.get("asks", [])
                for bid in bids:
                    vol = bid.get("v")
                    tmp[row_num] = (
                        DEPTH_SNAPSHOT_EVENT | BUY_EVENT,
                        exch_ts,
                        float(local_ts),
                        bid.get("p"),
                        bid.get("v"),
                        0,
                        0,
                        0
                    )
                    row_num += 1
        
                for ask in asks:
                    vol = ask.get("v")
                    tmp[row_num] = (
                        DEPTH_SNAPSHOT_EVENT | SELL_EVENT,
                        exch_ts,
                        float(local_ts),
                        ask.get("p"),
                        ask.get("v"),
                        0,
                        0,
                        0
                    )
                    row_num += 1

            elif dataset.startswith("spot@public.deals.v3.api"):
                trade_data = message.get("d")
                deals = trade_data.get("deals")
                for trade in deals:
                    #Mexc trades have field denoting time of trade distinct from message time field
                    deal_ts = trade.get("t") * exch_ts_multiplier
                    price = trade.get("p")
                    qty = trade.get("v")
                    tmp[row_num] = (
                        TRADE_EVENT | (SELL_EVENT if trade.get("S") == 2 else BUY_EVENT), # trade initiator's side
                        deal_ts,
                        float(local_ts),
                        float(price),
                        float(qty),
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
