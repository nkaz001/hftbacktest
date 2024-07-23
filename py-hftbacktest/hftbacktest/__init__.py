from typing import List, Any

import numpy as np
from numpy.typing import NDArray

from ._hftbacktest import (
    BacktestAsset as BacktestAsset_,
    build_hashmap_backtest,
    build_roivec_backtest
)
from .binding import (
    HashMapMarketDepthBacktest_,
    HashMapMarketDepthBacktest as HashMapMarketDepthBacktest_TypeHint,
    ROIVectorMarketDepthBacktest_,
    ROIVectorMarketDepthBacktest as ROIVectorMarketDepthBacktest_TypeHint,
    event_dtype
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
    LIMIT,
    MARKET,
)
from .recorder import Recorder
from .types import (
    ALL_ASSETS, EVENT_ARRAY
)

__all__ = (
    'BacktestAsset',
    'HashMapMarketDepthBacktest',
    'ROIVectorMarketDepthBacktest',

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

    'LIMIT',
    'MARKET',
    
    'Recorder'
)

__version__ = '2.0.0-alpha'


class BacktestAsset(BacktestAsset_):
    def add_data(self, data: EVENT_ARRAY):
        self._add_data_ndarray(data.ctypes.data, len(data))
        return self

    def data(self, data: str | List[str] | EVENT_ARRAY | List[EVENT_ARRAY]):
        if isinstance(data, str):
            self.add_file(data)
        elif isinstance(data, np.ndarray):
            self.add_data(data)
        elif isinstance(data, list):
            for item in data:
                if isinstance(item, str):
                    self.add_file(item)
                elif isinstance(item, np.ndarray):
                    self.add_data(item)
                else:
                    raise ValueError
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


def HashMapMarketDepthBacktest(
        assets: List[BacktestAsset]
) -> HashMapMarketDepthBacktest_TypeHint:
    """
    Constructs an instance of `HashMapMarketDepthBacktest`.

    Args:
        assets: A list of backtesting assets constructed using :class:`BacktestAsset`.

    Returns:
        A jit`ed `HashMapMarketDepthBacktest` that can be used in an ``njit`` function.
    """
    ptr = build_hashmap_backtest(assets)
    return HashMapMarketDepthBacktest_(ptr)


def ROIVectorMarketDepthBacktest(
        assets: List[BacktestAsset]
) -> ROIVectorMarketDepthBacktest_TypeHint:
    """
    Constructs an instance of `ROIVectorMarketBacktest`.

    Args:
        assets: A list of backtesting assets constructed using :class:`BacktestAsset`.

    Returns:
        A jit`ed `ROIVectorMarketBacktest` that can be used in an ``njit`` function.
    """
    ptr = build_roivec_backtest(assets)
    return ROIVectorMarketDepthBacktest_(ptr)
