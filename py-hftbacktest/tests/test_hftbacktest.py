import unittest

from numba import njit

from hftbacktest import (
    AssetBuilder,
    build_backtester,
    MultiAssetMultiExchangeBacktest,
    ALL_ASSETS
)


@njit
def test_run(hbt):
    order_id = 0
    while hbt.elapse(10_000_000_000) == 0:
        current_timestamp = hbt.current_timestamp()
        depth = hbt.depth_typed(0)
        best_bid = depth.best_bid()
        best_ask = depth.best_ask()

        # trades = hbt.trade_typed(0)
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
        asset = AssetBuilder()
        asset.linear_asset(1.0)
        asset.data(['tmp_20240501.npz'])
        asset.no_partial_fill_exchange()
        asset.constant_latency(100, 100)
        asset.power_prob_queue_model3(3.0)
        asset.tick_size(0.000001)
        asset.lot_size(1.0)
        asset.trade_len(1000)
        raw_hbt = build_backtester([asset])

        hbt = MultiAssetMultiExchangeBacktest(raw_hbt.as_ptr())
        test_run(hbt)
