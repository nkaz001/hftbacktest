import unittest

from hftbacktest import BUY, SELL
from hftbacktest.marketdepth import MarketDepth, INVALID_MAX, INVALID_MIN


class TestMarketDepth(unittest.TestCase):
    def setUp(self) -> None:
        self.depth = MarketDepth(0.1, 0.1)

    def test_bid(self):
        self.depth.update_bid_depth(1.2, 0.5, 1, None)
        self.depth.update_bid_depth(1.1, 0.4, 2, None)

        assert self.depth.best_bid_tick == 12
        assert self.depth.low_bid_tick == 11
        assert self.depth.bid_depth[12] == 0.5
        assert self.depth.bid_depth[11] == 0.4

        self.depth.update_bid_depth(1.7, 0.3, 3, None)

        assert self.depth.best_bid_tick == 17
        assert self.depth.bid_depth[17] == 0.3

        self.depth.update_bid_depth(1.2, 0, 4, None)
        self.depth.update_bid_depth(1.7, 0.01, 5, None)

        assert self.depth.best_bid_tick == 11

        self.depth.update_bid_depth(1.2, 1, 5, None)
        assert self.depth.best_bid_tick == 12
        self.depth.update_bid_depth(1.2, 0, 5, None)
        assert self.depth.best_bid_tick == 11
        self.depth.update_bid_depth(1.1, 0, 5, None)
        assert self.depth.best_bid_tick == INVALID_MIN
        assert self.depth.low_bid_tick == INVALID_MAX

    def test_ask(self):
        self.depth.update_ask_depth(2.1, 0.4, 2, None)
        self.depth.update_ask_depth(2.2, 0.5, 1, None)

        assert self.depth.best_ask_tick == 21
        assert self.depth.high_ask_tick == 22
        assert self.depth.ask_depth[22] == 0.5
        assert self.depth.ask_depth[21] == 0.4

        self.depth.update_ask_depth(1.8, 0.3, 3, None)

        assert self.depth.best_ask_tick == 18
        assert self.depth.ask_depth[18] == 0.3

        self.depth.update_ask_depth(2.1, 0, 4, None)
        self.depth.update_ask_depth(1.8, 0.01, 5, None)

        assert self.depth.best_ask_tick == 22

        self.depth.update_ask_depth(2.1, 1, 5, None)
        assert self.depth.best_ask_tick == 21
        self.depth.update_ask_depth(2.1, 0, 5, None)
        assert self.depth.best_ask_tick == 22
        self.depth.update_ask_depth(2.2, 0, 5, None)
        assert self.depth.best_ask_tick == INVALID_MAX
        assert self.depth.high_ask_tick == INVALID_MIN

    def test_clear(self):
        self.depth.update_bid_depth(1.2, 0.5, 1, None)
        self.depth.update_bid_depth(1.1, 0.4, 1, None)
        self.depth.update_ask_depth(2.1, 0.4, 1, None)
        self.depth.update_ask_depth(2.2, 0.5, 1, None)

        self.depth.clear_depth(0, 0)

        assert len(self.depth.bid_depth) == 0
        assert len(self.depth.ask_depth) == 0
        assert self.depth.best_bid_tick == INVALID_MIN
        assert self.depth.best_ask_tick == INVALID_MAX
        assert self.depth.low_bid_tick == INVALID_MAX
        assert self.depth.high_ask_tick == INVALID_MIN

        self.depth.update_bid_depth(1.2, 0.5, 1, None)
        self.depth.update_bid_depth(1.1, 0.4, 1, None)
        self.depth.update_bid_depth(0.9, 0.4, 1, None)
        self.depth.update_bid_depth(0.7, 0.4, 1, None)
        self.depth.update_bid_depth(0.6, 0.4, 1, None)
        self.depth.update_ask_depth(2.1, 0.4, 1, None)
        self.depth.update_ask_depth(2.2, 0.5, 1, None)
        self.depth.update_ask_depth(2.8, 0.5, 1, None)
        self.depth.update_ask_depth(3.0, 0.5, 1, None)
        self.depth.update_ask_depth(3.1, 0.5, 1, None)

        self.depth.clear_depth(BUY, 0.9)
        assert len(self.depth.bid_depth) == 2
        assert self.depth.best_bid_tick == 7

        self.depth.clear_depth(BUY, 0.6)
        assert len(self.depth.bid_depth) == 0
        assert self.depth.best_bid_tick == INVALID_MIN
        assert self.depth.low_bid_tick == INVALID_MAX

        self.depth.clear_depth(SELL, 2.8)
        assert len(self.depth.ask_depth) == 2
        assert self.depth.best_ask_tick == 30

        self.depth.clear_depth(SELL, 3.1)
        assert len(self.depth.ask_depth) == 0
        assert self.depth.best_ask_tick == INVALID_MAX
        assert self.depth.high_ask_tick == INVALID_MIN
