from typing import List, Any

import numpy as np
from numpy.typing import NDArray

from ._hftbacktest import (
    BacktestAsset as BacktestAsset_,
    build_backtester
)
from .binding import (
    MultiAssetMultiExchangeBacktest_,
    MultiAssetMultiExchangeBacktest as MultiAssetMultiExchangeBacktest_TypeHint,
    event_dtype
)
from .data import (
    correct_local_timestamp,
)
from .order import (
    BUY,
    SELL,
    NONE,
    NEW,
    EXPIRED,
    FILLED,
    CANCELED,
    GTC,
    GTX,
    order_dtype,
)
from .types import (
    ALL_ASSETS
)

__all__ = (
    'BacktestAsset',
    'MultiAssetMultiExchangeBacktest',

    'ALL_ASSETS',

    # Side
    'BUY',
    'SELL',

    # Order status
    'NONE',
    'NEW',
    'EXPIRED',
    'FILLED',
    'CANCELED',

    # Time-In-Force
    'GTC',
    'GTX',
)

__version__ = '2.0.0-alpha'

__hftbacktests__ = []


class BacktestAsset(BacktestAsset_):
    def add_data(self, data: str | np.ndarray[Any, event_dtype]):
        if isinstance(data, str):
            super().add_file(data)
        elif isinstance(data, np.ndarray):
            self._add_data_ndarray(data.ctypes.data, len(data))
        else:
            raise ValueError
        return self

    def intp_order_latency(self, data: str | NDArray):
        if isinstance(data, str):
            super().intp_order_latency_ndarray(data)
        elif isinstance(data, np.ndarray):
            self._intp_order_latency_ndarray(data.ctypes.data, len(data))
        else:
            raise ValueError
        return self

    def initial_snapshot(self, data: str | np.ndarray[Any, event_dtype]):
        if isinstance(data, str):
            super().initial_snapshot(data)
        elif isinstance(data, np.ndarray):
            self._initial_snapshot_ndarray(data.ctypes.data, len(data))
        else:
            raise ValueError
        return self


def MultiAssetMultiExchangeBacktest(assets: List[BacktestAsset]) -> MultiAssetMultiExchangeBacktest_TypeHint:
    """
    Constructs an instance of `MultiAssetMultiExchangeBacktest`.

    Args:
        assets: A list of backtesting assets constructed using :class:`BacktestAsset`.

    Returns:
        A jit`ed `MultiAssetMultiExchangeBacktest` that can be used in an ``njit`` function.
    """
    raw_hbt = build_backtester(assets)

    # Prevents the object from being gc`ed to avoid dangling references.
    __hftbacktests__.append(raw_hbt)

    return MultiAssetMultiExchangeBacktest_(raw_hbt.as_ptr())
