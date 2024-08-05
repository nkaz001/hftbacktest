from typing import List

import numpy as np
from numpy.typing import NDArray

from ... import BacktestAsset, HashMapMarketDepthBacktest


def create_last_snapshot(
        data: List[str],
        tick_size: float,
        lot_size: float,
        initial_snapshot: str | None = None,
        output_snapshot_filename: str | None = None
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
    asset = (
        BacktestAsset()
            .data(data)
            .tick_size(tick_size)
            .lot_size(lot_size)
    )
    if initial_snapshot is not None:
        asset.initial_snapshot(initial_snapshot)

    hbt = HashMapMarketDepthBacktest([asset])

    # Go to the end of the data.
    if hbt._goto_end() not in [0, 1]:
        raise RuntimeError

    depth = hbt.depth(0)
    snapshot = depth.snapshot()
    snapshot_copied = snapshot.copy()
    depth.snapshot_free(snapshot)

    if output_snapshot_filename is not None:
        np.savez_compressed(output_snapshot_filename, data=snapshot_copied)

    return snapshot_copied
