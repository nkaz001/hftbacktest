import unittest
import numpy as np

from numba import njit

from hftbacktest import (
    BacktestAsset,
    BUY_EVENT,
    DEPTH_EVENT,
    EXCH_EVENT,
    HashMapMarketDepthBacktest,
    LOCAL_EVENT,
    ROIVectorMarketDepthBacktest,
    SELL_EVENT,
    ALL_ASSETS,
    event_dtype,
)


@njit
def test_run(hbt):
    order_id = 0
    while hbt.elapse(10_000_000_000) == 0:
        current_timestamp = hbt.current_timestamp
        depth = hbt.depth(0)
        best_bid = depth.best_bid
        best_ask = depth.best_ask

        # trades = hbt.last_trades(0)
        #
        # i = 0
        # for trade in trades:
        #     print(trade.local_ts, trade.px, trade.qty)
        #     i += 1
        #     if i > 5:
        #         break

        hbt.clear_last_trades(ALL_ASSETS)

        cnt = 0
        orders = hbt.orders(0)
        values = orders.values()
        while True:
            order = values.next()
            if order is None:
                break
            cnt += 1
            print(order.order_id, order.side, order.price_tick, order.qty)

        hbt.clear_inactive_orders(ALL_ASSETS)

        if cnt <= 2:
            hbt.submit_buy_order(0, order_id, best_bid, 1, 1, 0, False)
            order_id += 1
            hbt.submit_sell_order(0, order_id, best_ask, 1, 1, 0, False)
            order_id += 1

        print(current_timestamp, best_bid, best_ask)


class TestPyHftBacktest(unittest.TestCase):
    def setUp(self) -> None:
        pass

    def test_run_backtest(self):
        arr = np.load('tmp_20240501.npz')['data']

        asset = (
            BacktestAsset()
                .linear_asset(1.0)
                .data(['tmp_20240501.npz'])
                .no_partial_fill_exchange()
                .constant_latency(100, 100)
                .power_prob_queue_model3(3.0)
                .tick_size(0.000001)
                .lot_size(1.0)
                .trade_len(1000)
                .roi_lb(0.0)
                .roi_ub(1.0)
        )

        # hbt = HashMapMarketDepthMultiAssetMultiExchangeBacktest([asset])
        hbt = ROIVectorMarketDepthBacktest([asset])
        test_run(hbt)

    def test_roi_vector_depth_returns_quantities_at_best_prices(self):
        data = np.zeros(2, event_dtype)
        data["ev"] = [
            DEPTH_EVENT | BUY_EVENT | EXCH_EVENT | LOCAL_EVENT,
            DEPTH_EVENT | SELL_EVENT | EXCH_EVENT | LOCAL_EVENT,
        ]
        data["exch_ts"] = data["local_ts"] = [1_000_000_000, 1_000_000_001]
        data["px"] = [1.0419, 1.0420]
        data["qty"] = [23_266.0, 18_000.0]

        asset = (
            BacktestAsset()
            .data([data])
            .linear_asset(1.0)
            .constant_order_latency(0, 0)
            .no_partial_fill_exchange()
            .risk_adverse_queue_model()
            .tick_size(0.0001)
            .lot_size(0.1)
            .roi_lb(0.8)
            .roi_ub(2.0)
        )
        hbt = ROIVectorMarketDepthBacktest([asset])
        try:
            hbt.elapse(10_000_000_000)
            depth = hbt.depth(0)
            self.assertEqual(depth.best_bid_qty, 23_266.0)
            self.assertEqual(depth.best_ask_qty, 18_000.0)
        finally:
            hbt.close()
