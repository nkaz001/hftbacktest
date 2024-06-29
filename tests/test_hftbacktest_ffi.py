import unittest
from numba import njit

from hftbacktest import (
    PyAssetBuilder,
    PyAssetType,
    PyExchangeKind,
    PyLatencyModel,
    build_backtester,
    MultiAssetMultiExchangeBacktest
)


@njit
def test_run(hbt):
    while hbt.elapse(10_000_000_000) == 0:
        current_timestamp = hbt.current_timestamp()
        depth = hbt.depth_typed(0)
        best_bid = depth.best_bid_tick()
        best_ask = depth.best_ask_tick()
        print(current_timestamp, best_bid, best_ask)


class TestFFI(unittest.TestCase):
    def setUp(self) -> None:
        pass

    def test_run_backtest(self):
        asset = PyAssetBuilder()
        asset.asset_type(PyAssetType.LinearAsset)
        asset.data(['tmp_20240501.npz'])
        asset.exchange(PyExchangeKind.NoPartialFillExchange)
        asset.latency_model(PyLatencyModel.ConstantLatency)

        raw_hbt = build_backtester([asset])

        hbt = MultiAssetMultiExchangeBacktest(raw_hbt.as_ptr())
        test_run(hbt)
