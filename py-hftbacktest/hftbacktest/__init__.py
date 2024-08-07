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
    ALL_ASSETS,
    EVENT_ARRAY,
    DEPTH_EVENT,
    TRADE_EVENT,
    DEPTH_CLEAR_EVENT,
    DEPTH_SNAPSHOT_EVENT,
    DEPTH_BBO_EVENT,
    ADD_ORDER_EVENT,
    CANCEL_ORDER_EVENT,
    MODIFY_ORDER_EVENT,
    FILL_EVENT,
    EXCH_EVENT,
    LOCAL_EVENT,
    BUY_EVENT,
    SELL_EVENT
)

__all__ = (
    'BacktestAsset',
    'HashMapMarketDepthBacktest',
    'ROIVectorMarketDepthBacktest',

    'ALL_ASSETS',

    # Event flags
    'DEPTH_EVENT',
    'TRADE_EVENT',
    'DEPTH_CLEAR_EVENT',
    'DEPTH_SNAPSHOT_EVENT',
    'DEPTH_BBO_EVENT',
    'ADD_ORDER_EVENT',
    'CANCEL_ORDER_EVENT',
    'MODIFY_ORDER_EVENT',
    'FILL_EVENT',
    'EXCH_EVENT',
    'LOCAL_EVENT',
    'EXCH_EVENT',
    'LOCAL_EVENT'
    'BUY_EVENT',
    'SELL_EVENT',

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

__version__ = '2.0.0rc1'


class BacktestAsset(BacktestAsset_):
    def add_data(self, data: EVENT_ARRAY):
        self._add_data_ndarray(data.ctypes.data, len(data))
        return self

    def data(self, data: str | List[str] | EVENT_ARRAY | List[EVENT_ARRAY]):
        """
        Sets the feed data.

        Args:
            data: A list of file paths for the feed data in `.npz` format, or a list of NumPy arrays containing the feed
                  data.
        """
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

    def intp_order_latency(self, data: str | NDArray | List[str]):
        """
        Uses `IntpOrderLatency <https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.IntpOrderLatency.html>`_
        for the order latency model.
        Please see the data format.
        The units of the historical latencies should match the timestamp units of your data.
        Nanoseconds are typically used in HftBacktest.

        Args:
            data: A list of file paths for the historical order latency data in `npz`, or a NumPy array of the
                  historical order latency data.
        """
        if isinstance(data, str):
            super().intp_order_latency([data])
        elif isinstance(data, np.ndarray):
            self._intp_order_latency_ndarray(data.ctypes.data, len(data))
        elif isinstance(data, list):
            super().intp_order_latency(data)
        else:
            raise ValueError
        return self

    def initial_snapshot(self, data: str | np.ndarray[Any, event_dtype]):
        """
        Sets the initial snapshot.

        Args:
            data: The initial snapshot file path, or a NumPy array of the initial snapshot.
        """
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
