import gzip
import json
from enum import IntEnum
from typing import Optional

import numpy as np
from numpy.typing import NDArray

from .. import FuseMarketDepth
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


class BybitDepthLevel(IntEnum):
    """Reference: https://bybit-exchange.github.io/docs/v5/websocket/public/orderbook#depths"""
    LEVEL_1 = 1
    LEVEL_25 = 25
    LEVEL_50 = 50
    LEVEL_100 = 100
    LEVEL_200 = 200
    LEVEL_1000 = 1000


class _Fuse:
    def __init__(self, tick_size: float, lot_size: float):
        self.depth = FuseMarketDepth(tick_size, lot_size)
        self.ev = np.zeros(1, event_dtype)

    def close(self):
        self.depth.close()

    def process_depth_event(self, message_type, data, exch_timestamp, local_timestamp):
        """Process depth events using FuseMarketDepth for multi-level fusion"""

        if message_type == "snapshot":
            bids = data.get("b", [])
            asks = data.get("a", [])

            for px, qty in bids:
                self.ev[0] = (
                    DEPTH_SNAPSHOT_EVENT | BUY_EVENT,
                    exch_timestamp,
                    local_timestamp,
                    float(px),
                    float(qty),
                    0,
                    0,
                    0,
                )
                self.depth.process_event(self.ev, 0, True)

            for px, qty in asks:
                self.ev[0] = (
                    DEPTH_SNAPSHOT_EVENT | SELL_EVENT,
                    exch_timestamp,
                    local_timestamp,
                    float(px),
                    float(qty),
                    0,
                    0,
                    0,
                )
                self.depth.process_event(self.ev, 0, True)

        elif message_type == "delta":
            bids = data.get("b", [])
            asks = data.get("a", [])
            for px, qty in bids:
                self.ev[0] = (
                    DEPTH_EVENT | BUY_EVENT,
                    exch_timestamp,
                    local_timestamp,
                    float(px),
                    float(qty),
                    0,
                    0,
                    0,
                )
                self.depth.process_event(self.ev, 0, True)

            for px, qty in asks:
                self.ev[0] = (
                    DEPTH_EVENT | SELL_EVENT,
                    exch_timestamp,
                    local_timestamp,
                    float(px),
                    float(qty),
                    0,
                    0,
                    0,
                )
                self.depth.process_event(self.ev, 0, True)

    @property
    def fused_events(self):
        return self.depth.fused_events


def convert_fused(
    input_filename: str,
    output_filename: Optional[str] = None,
    base_latency: float = 0,
    buffer_size: int = 100_000_000,
    tick_size: float = 0.01,
    lot_size: float = 0.001,
) -> NDArray:
    r"""
    Converts raw Bybit feed stream file into a format compatible with HftBacktest using fused market depth processing.
    If you encounter an ``IndexError`` due to an out-of-bounds, try increasing the ``buffer_size``.

    This function **fuses** multiple depth levels into a single market depth representation.
    Use :func:`.convert_depth` if you wish to process only a single depth level

    **Example:**

    .. code-block:: python
      # Fuse all depth levels into a single market depth representation
      data = convert_fused('input.gz', tick_size=0.01, lot_size=0.001)


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
        tick_size: Tick size for the instrument (required for fusion processing).
        lot_size: Lot size for the instrument (required for fusion processing).

    Returns:
        Converted data compatible with HftBacktest.
    """
    timestamp_slice = 19
    timestamp_mul = 1000000

    fuse = _Fuse(tick_size, lot_size)

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

                if topic.startswith("orderbook."):
                    message_type = message.get("type", "")
                    fuse.process_depth_event(
                        message_type, data, exch_timestamp, local_timestamp
                    )

                elif topic.startswith("publicTrade."):
                    for trade in data:
                        trade_timestamp = trade.get("T", ts)
                        price = trade.get("p", "0")
                        qty = trade.get("v", "0")
                        side = trade.get("S", "Buy")

                        trade_exch_timestamp = int(trade_timestamp) * timestamp_mul

                        tmp[row_num] = (
                            TRADE_EVENT | (SELL_EVENT if side == "Sell" else BUY_EVENT),
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
                    # No-throw because we don't want to stop processing
                    print(message["code"], message.get("msg", ""))

    fused_data = fuse.fused_events
    if len(fused_data) == 0:
        tmp = tmp[:row_num]
    else:
        trade_events = tmp[:row_num]
        trade_mask = (trade_events["ev"] & TRADE_EVENT) != 0
        trade_only = trade_events[trade_mask]

        if len(trade_only) > 0:
            all_events = np.concatenate([fused_data, trade_only])
            sort_idx = np.lexsort((all_events["local_ts"], all_events["exch_ts"]))
            tmp = all_events[sort_idx]
        else:
            tmp = fused_data

    fuse.close()

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


def _convert_depth(
    tmp,
    row_num,
    topic,
    data,
    message,
    exch_timestamp,
    local_timestamp,
    single_depth_level: BybitDepthLevel,
) -> int:
    """Auxiliary function for :func:`.convert_depth` handling depth and trade processing logic."""

    if topic.startswith("orderbook."):
        expected_prefix = f"orderbook.{single_depth_level}."
        if not topic.startswith(expected_prefix):
            return row_num

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

    elif topic.startswith("publicTrade."):
        for trade in data:
            trade_timestamp = trade.get("T", message.get("ts", 0))
            price = trade.get("p", "0")
            qty = trade.get("v", "0")
            side = trade.get("S", "Buy")

            trade_exch_timestamp = int(trade_timestamp) * 1000000

            tmp[row_num] = (
                TRADE_EVENT | (SELL_EVENT if side == "Sell" else BUY_EVENT),
                trade_exch_timestamp,
                local_timestamp,
                float(price),
                float(qty),
                0,
                0,
                0,
            )
            row_num += 1

    return row_num


def convert_depth(
    input_filename: str,
    output_filename: Optional[str] = None,
    base_latency: float = 0,
    buffer_size: int = 100_000_000,
    single_depth_level: BybitDepthLevel = BybitDepthLevel.LEVEL_50,
) -> NDArray:
    r"""
    Converts raw Bybit feed stream file into a format compatible with HftBacktest.
    If you encounter an ``IndexError`` due to an out-of-bounds, try increasing the ``buffer_size``.

    Use :func:`.convert_fused` if you need fused processing of multiple depth levels.

    **Example:**

    .. code-block:: python
        # Process with default depth level (50)
        data = convert_depth('input.gz')

        # Process a specific depth level using enum
        data = convert_depth('input.gz', single_depth_level=BybitDepthLevel.LEVEL_1)

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
        single_depth_level: Depth level to process. Use `BybitDepthLevel` enum values.

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
                row_num = _convert_depth(
                    tmp,
                    row_num,
                    topic,
                    data,
                    message,
                    exch_timestamp,
                    local_timestamp,
                    single_depth_level,
                )
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
