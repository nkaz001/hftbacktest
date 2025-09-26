import gzip
import json
from typing import Optional, Literal

import numpy as np
from numpy.typing import NDArray

from ..validation import (
    correct_event_order,
    correct_local_timestamp,
    validate_event_order,
)
from ...types import (
    DEPTH_EVENT,
    DEPTH_CLEAR_EVENT,
    DEPTH_SNAPSHOT_EVENT,
    TRADE_EVENT,
    BUY_EVENT,
    SELL_EVENT,
    event_dtype,
)


def convert(
    input_filename: str,
    output_filename: Optional[str] = None,
    base_latency: float = 0,
    buffer_size: int = 100_000_000,
) -> NDArray:
    r"""
    Converts raw Bybit feed stream file into a format compatible with HftBacktest.
    If you encounter an ``IndexError`` due to an out-of-bounds, try increasing the ``buffer_size``.

    **File Format:**

    .. code-block::

        local_timestamp raw_stream
        1758841137168651303 {"topic":"orderbook.1.BTCUSDT","type":"snapshot","ts":1758841134603,"data":{"s":"BTCUSDT","b":[["109378.80","1.273"]],"a":[["109378.90","6.278"]],"u":14869255,"seq":457514742271},"cts":1758841134598}
        1758841138293664629 {"topic":"publicTrade.BTCUSDT","type":"snapshot","ts":1758841135824,"data":[{"T":1758841135823,"s":"BTCUSDT","S":"Buy","v":"0.020","p":"109378.90","L":"ZeroPlusTick","i":"2a74ac78-691c-5b54-9dbe-5aafe8364627","BT":false,"RPI":false,"seq":457514743450}]}
        1758845432767124368 {"topic":"orderbook.50.BTCUSDT","type":"delta","ts":1758845431663,"data":{"s":"BTCUSDT","b":[["109147.80","1.687"],["109144.10","0.027"]],"a":[["109148.00","0"],["109154.30","0.076"],["109156.40","0"],["109165.30","0.003"],["109165.40","0.007"]],"u":21111423,"seq":457538796406},"cts":1758845431662}

    Args:
        input_filename: Input filename with path.
        output_filename: If provided, the converted data will be saved to the specified filename in ``npz`` format.
        base_latency: The value to be added to the feed latency.
                      See :func:`.correct_local_timestamp`.
        buffer_size: Sets a preallocated row size for the buffer.

    Returns:
        Converted data compatible with HftBacktest.
    """
    timestamp_slice = 19
    timestamp_mul = 1000000

    tmp = np.empty(buffer_size, event_dtype)
    row_num = 0
    with gzip.open(input_filename, "r") as f:
        while True:
            line = f.readline()
            if not line:
                break
            local_timestamp = int(line[:timestamp_slice])
            message = json.loads(line[timestamp_slice + 1 :])

            topic = message.get("topic", "")
            data = message.get("data")
            ts = message.get("ts", 0)

            if data is not None:
                exch_timestamp = int(ts) * timestamp_mul

                # [orderbook.1.SYMBOL, orderbook.50.SYMBOL, orderbook.500.SYMBOL]. Reference: hftbacktest\collector\src\main.rs --> bybit
                if topic.startswith("orderbook."):
                    message_type = message.get("type", "")

                    if message_type == "snapshot":
                        # clear and rebuild orderbook
                        bids = data.get("b", [])
                        asks = data.get("a", [])

                        if len(bids) > 0:
                            bid_clear_upto = float(bids[-1][0])
                            # 1: clear the existing market depth upto the prices in the snapshot.
                            tmp[row_num] = (
                                DEPTH_CLEAR_EVENT | BUY_EVENT,
                                exch_timestamp,
                                local_timestamp,
                                bid_clear_upto,
                                0,
                                0,
                                0,
                                0,
                            )
                            row_num += 1
                            # 2: insert the snapshot.
                            for px, qty in bids:
                                tmp[row_num] = (
                                    DEPTH_SNAPSHOT_EVENT | BUY_EVENT,
                                    exch_timestamp,
                                    local_timestamp,
                                    float(px),
                                    float(qty),
                                    0,
                                    0,
                                    0,
                                )
                                row_num += 1

                        if len(asks) > 0:
                            ask_clear_upto = float(asks[-1][0])
                            # 1: clear the existing market depth upto the prices in the snapshot.
                            tmp[row_num] = (
                                DEPTH_CLEAR_EVENT | SELL_EVENT,
                                exch_timestamp,
                                local_timestamp,
                                ask_clear_upto,
                                0,
                                0,
                                0,
                                0,
                            )
                            row_num += 1
                            # 2: insert the snapshot.
                            for px, qty in asks:
                                tmp[row_num] = (
                                    DEPTH_SNAPSHOT_EVENT | SELL_EVENT,
                                    exch_timestamp,
                                    local_timestamp,
                                    float(px),
                                    float(qty),
                                    0,
                                    0,
                                    0,
                                )
                                row_num += 1

                    elif message_type == "delta":
                        for px, qty in data.get("b", []):
                            tmp[row_num] = (
                                DEPTH_EVENT | BUY_EVENT,
                                exch_timestamp,
                                local_timestamp,
                                float(px),
                                float(qty),
                                0,
                                0,
                                0,
                            )
                            row_num += 1
                        for px, qty in data.get("a", []):
                            tmp[row_num] = (
                                DEPTH_EVENT | SELL_EVENT,
                                exch_timestamp,
                                local_timestamp,
                                float(px),
                                float(qty),
                                0,
                                0,
                                0,
                            )
                            row_num += 1

                # [publicTrade.SYMBOL]. Reference: hftbacktest\collector\src\main.rs  --> bybit
                elif topic.startswith("publicTrade."):
                    if isinstance(data, list):
                        for trade in data:
                            trade_timestamp = trade.get("T", ts)
                            price = trade.get("p", "0")
                            qty = trade.get("v", "0")
                            side = trade.get("S", "Buy")

                            trade_exch_timestamp = int(trade_timestamp) * timestamp_mul

                            tmp[row_num] = (
                                TRADE_EVENT
                                | (SELL_EVENT if side == "Sell" else BUY_EVENT),
                                trade_exch_timestamp,
                                local_timestamp,
                                float(price),
                                float(qty),
                                0,
                                0,
                                0,
                            )
                            row_num += 1
            else:
                if "code" in message:
                    print(message["code"], message.get("msg", ""))

    tmp = tmp[:row_num]

    print("Correcting the latency")
    tmp = correct_local_timestamp(tmp, base_latency)

    print("Correcting the event order")
    data = correct_event_order(
        tmp,
        np.argsort(tmp["exch_ts"], kind="mergesort"),
        np.argsort(tmp["local_ts"], kind="mergesort"),
    )

    validate_event_order(data)

    if output_filename is not None:
        print("Saving to %s" % output_filename)
        np.savez_compressed(output_filename, data=data)

    return data
