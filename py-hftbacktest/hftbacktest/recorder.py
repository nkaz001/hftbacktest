from typing import Any

import numpy as np
from numba import uint64, from_dtype
from numba.experimental import jitclass

from .types import record_dtype


@jitclass
class Recorder_:
    records: from_dtype(record_dtype)[:, :]
    i: uint64

    def __init__(self, num_assets: uint64, record_size: uint64):
        self.records = np.empty((record_size, num_assets), record_dtype)
        self.i = 0

    def record(self, hbt) -> None:
        timestamp = hbt.current_timestamp
        for asset_no in range(hbt.num_assets):
            depth = hbt.depth(asset_no)
            mid_price = (depth.best_bid + depth.best_ask) / 2.0
            state_values = hbt.state_values(asset_no)
            self.records[self.i, asset_no].timestamp = timestamp
            self.records[self.i, asset_no].price = mid_price
            self.records[self.i, asset_no].position = state_values.position
            self.records[self.i, asset_no].balance = state_values.balance
            self.records[self.i, asset_no].fee = state_values.fee
            self.records[self.i, asset_no].num_trades = state_values.num_trades
            self.records[self.i, asset_no].trading_volume = state_values.trading_volume
            self.records[self.i, asset_no].trading_value = state_values.trading_value

        self.i += 1
        if self.i == len(self.records):
            raise IndexError


class Recorder:
    """
    Record time-series state information for equity and performance metric calculation.

    Args:
        num_assets: Total number of assets.
        record_size: Maximum number of records to store.
    """

    def __init__(self, num_assets: uint64, record_size: uint64):
        self._recorder = Recorder_(num_assets, record_size)

    @property
    def recorder(self):
        """
        Returns the recorder instance used inside Numba code.

        You can use this instance to record state during execution, e.g.:
        ``recorder.record(hbt)``
        """
        return self._recorder

    def to_npz(self, file: str) -> None:
        """
        Save records to a file.

        Args:
            file: Path to the output file.
        """
        data = self._recorder.records[:self._recorder.i]
        kwargs = {str(asset_no): data[:, asset_no] for asset_no in range(data.shape[1])}
        np.savez_compressed(file, **kwargs)

    def get(self, asset_no: int) -> np.ndarray[Any, record_dtype]:
        """
        Get records for a given asset number.

        Args:
            asset_no: Asset number.

        Returns:
            The record associated with the given asset number.
        """
        return self._recorder.records[:self._recorder.i, asset_no]
