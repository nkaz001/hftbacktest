from typing import List

import numpy as np
from numpy.typing import NDArray

from ...binding import event_dtype
from ...types import SELL_EVENT, BUY_EVENT
from ...types import UNTIL_END_OF_DATA, DEPTH_SNAPSHOT_EVENT, LOCAL_EVENT, EXCH_EVENT


def create_last_snapshot(
        data: List[str],
        tick_size: float,
        lot_size: float,
        initial_snapshot: str | None = None,
        output_snapshot_filename: str | None = None,
        snapshot_size: int = 100_000_000
) -> NDArray:
    r"""
    Creates a snapshot of the last market depth for the specified data, which can be used as the initial snapshot data
    for subsequent data.

    Args:
         data: Data to be processed to obtain the last market depth snapshot.
         tick_size: Minimum price increment for the given asset.
         lot_size: Minimum order quantity for the given asset.
         initial_snapshot: The initial market depth snapshot.
         output_snapshot_filename: If provided, the snapshot data will be saved to the specified filename in ``npz``
                                   format.

    Returns:
        Snapshot of the last market depth compatible with HftBacktest.
    """
    # Just to reconstruct order book from the given snapshot to the end of the given data.
    # fixme: use hftbacktest-backend version.
    asset = AssetBuilder()
    asset.linear_asset(1.0)
    asset.data(data)
    asset.no_partial_fill_exchange()
    asset.constant_latency(0, 0)
    asset.power_prob_queue_model3(0)
    asset.tick_size(tick_size)
    asset.lot_size(lot_size)
    asset.trade_len(0)
    raw_hbt = build_backtester([asset])
    hbt = MultiAssetMultiExchangeBacktest(raw_hbt.as_ptr())

    # Go to the end of the data.
    hbt.goto(UNTIL_END_OF_DATA)

    snapshot = np.empty(snapshot_size, event_dtype)
    out_rn = 0
    for bid, qty in sorted(hbt.bid_depth.items(), key=lambda v: -float(v[0])):
        snapshot[out_rn] = (
            DEPTH_SNAPSHOT_EVENT | EXCH_EVENT | LOCAL_EVENT | BUY_EVENT,
            # fixme: timestamp
            hbt.last_timestamp,
            hbt.last_timestamp,
            float(bid * tick_size),
            float(qty)
        )
        out_rn += 1
    for ask, qty in sorted(hbt.ask_depth.items(), key=lambda v: float(v[0])):
        snapshot[out_rn] = (
            DEPTH_SNAPSHOT_EVENT | EXCH_EVENT | LOCAL_EVENT | SELL_EVENT,
            # fixme: timestamp
            hbt.last_timestamp,
            hbt.last_timestamp,
            float(ask * tick_size),
            float(qty)
        )
        out_rn += 1

    if output_snapshot_filename is not None:
        np.savez_compressed(output_snapshot_filename, data=snapshot)

    return snapshot
