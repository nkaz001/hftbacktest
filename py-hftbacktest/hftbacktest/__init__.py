from typing import List

from .data import (
    correct_local_timestamp,
)
from .types import (
    ALL_ASSETS
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
from .binding import MultiAssetMultiExchangeBacktest_

from ._hftbacktest import (
    BacktestAsset,
    build_backtester
)

__all__ = (
    'BacktestAsset',

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


def MultiAssetMultiExchangeBacktest(assets: List[BacktestAsset]):
    raw_hbt = build_backtester(assets)
    __hftbacktests__.append(raw_hbt)
    return MultiAssetMultiExchangeBacktest_(raw_hbt.as_ptr())
