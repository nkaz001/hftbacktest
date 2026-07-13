"""Tests for CryptoHFTData download and HftBacktest event conversion."""

import unittest
from unittest.mock import patch

import polars as pl

from hftbacktest import (
    BUY_EVENT,
    DEPTH_CLEAR_EVENT,
    DEPTH_EVENT,
    DEPTH_SNAPSHOT_EVENT,
    EXCH_EVENT,
    LOCAL_EVENT,
    SELL_EVENT,
    TRADE_EVENT,
)
from hftbacktest.data.utils import cryptohftdata


def _strip_order_flags(ev):
    return int(ev) & ~(EXCH_EVENT | LOCAL_EVENT)


class FakeCryptoHFTDataClient:
    """Provide deterministic SDK-shaped order book and trade frames."""
    def __init__(self, orderbook, trades):
        self._orderbook = orderbook
        self._trades = trades
        self.calls = []

    def get_orderbook(self, **kwargs):
        self.calls.append(("orderbook", kwargs))
        return self._orderbook

    def get_trades(self, **kwargs):
        self.calls.append(("trades", kwargs))
        return self._trades


class TestCryptoHFTDataUtils(unittest.TestCase):
    """Verify CryptoHFTData schema normalization and event semantics."""
    def _update_orderbook_df(self, include_transaction_time=True):
        data = {
            "received_time": [1_000_000_000, 1_000_000_000],
            "event_time": [1_000, 1_000],
            "event_type": ["update", "update"],
            "side": ["bid", "ask"],
            "price": [100.0, 101.0],
            "quantity": [2.0, 3.0],
        }
        if include_transaction_time:
            data["transaction_time"] = [999, 999]
        return pl.DataFrame(data)

    def _trade_df(self, *, include_trade_time=True, is_buyer_maker=False):
        data = {
            "received_time": [1_000_000_000],
            "event_time": [1_000],
            "price": [100.5],
            "quantity": [0.25],
            "is_buyer_maker": [is_buyer_maker],
        }
        if include_trade_time:
            data["trade_time"] = [998]
        return pl.DataFrame(data)

    def test_convert_update_only_keeps_trade_before_depth_at_equal_timestamps(self):
        orderbook = self._update_orderbook_df()
        trades = self._trade_df()

        data = cryptohftdata.convert(orderbook, trades)

        self.assertEqual(len(data), 3)
        self.assertEqual(_strip_order_flags(data[0]["ev"]), BUY_EVENT | TRADE_EVENT)
        self.assertEqual(_strip_order_flags(data[1]["ev"]), BUY_EVENT | DEPTH_EVENT)
        self.assertEqual(_strip_order_flags(data[2]["ev"]), SELL_EVENT | DEPTH_EVENT)
        self.assertEqual(data[0]["px"], 100.5)
        self.assertEqual(data[1]["px"], 100.0)
        self.assertEqual(data[2]["px"], 101.0)

    def test_convert_falls_back_to_event_time_when_transaction_time_missing(self):
        orderbook = self._update_orderbook_df(include_transaction_time=False)
        trades = self._trade_df(include_trade_time=False)

        data = cryptohftdata.convert(orderbook, trades)

        self.assertTrue((data["exch_ts"] == 1_000_000_000).all())

    def test_convert_maps_buyer_maker_trade_to_sell_initiator(self):
        data = cryptohftdata.convert(self._update_orderbook_df(), self._trade_df(is_buyer_maker=True))

        self.assertEqual(_strip_order_flags(data[0]["ev"]), SELL_EVENT | TRADE_EVENT)

    def test_convert_translates_snapshot_rows_to_clear_and_snapshot_events(self):
        orderbook = pl.DataFrame(
            {
                "received_time": [2_000_000_000] * 4,
                "event_time": [2_000] * 4,
                "event_type": ["snapshot"] * 4,
                "side": ["bid", "bid", "ask", "ask"],
                "price": [100.0, 99.0, 101.0, 102.0],
                "quantity": [1.0, 2.0, 3.0, 4.0],
                "first_update_id": [10] * 4,
                "final_update_id": [11] * 4,
            }
        )
        trades = pl.DataFrame(
            {
                "received_time": [],
                "event_time": [],
                "price": [],
                "quantity": [],
                "is_buyer_maker": [],
            },
            schema={
                "received_time": pl.Int64,
                "event_time": pl.Int64,
                "price": pl.Float64,
                "quantity": pl.Float64,
                "is_buyer_maker": pl.Boolean,
            },
        )

        data = cryptohftdata.convert(orderbook, trades)

        self.assertEqual(len(data), 6)
        expected = [
            (BUY_EVENT | DEPTH_CLEAR_EVENT, 99.0, 0.0),
            (BUY_EVENT | DEPTH_SNAPSHOT_EVENT, 100.0, 1.0),
            (BUY_EVENT | DEPTH_SNAPSHOT_EVENT, 99.0, 2.0),
            (SELL_EVENT | DEPTH_CLEAR_EVENT, 102.0, 0.0),
            (SELL_EVENT | DEPTH_SNAPSHOT_EVENT, 101.0, 3.0),
            (SELL_EVENT | DEPTH_SNAPSHOT_EVENT, 102.0, 4.0),
        ]
        for row, (ev, px, qty) in zip(data, expected):
            self.assertEqual(_strip_order_flags(row["ev"]), ev)
            self.assertEqual(row["px"], px)
            self.assertEqual(row["qty"], qty)

    def test_download_uses_injected_client(self):
        orderbook = self._update_orderbook_df()
        trades = self._trade_df()
        client = FakeCryptoHFTDataClient(orderbook, trades)

        downloaded_orderbook, downloaded_trades = cryptohftdata.download(
            symbol="BTCUSDT",
            exchange="binance_futures",
            start_date="2026-03-01",
            end_date="2026-03-01",
            client=client,
            max_workers=4,
        )

        self.assertIs(downloaded_orderbook, orderbook)
        self.assertIs(downloaded_trades, trades)
        self.assertEqual(
            client.calls,
            [
                (
                    "orderbook",
                    {
                        "symbol": "BTCUSDT",
                        "exchange": "binance_futures",
                        "start_date": "2026-03-01",
                        "end_date": "2026-03-01",
                        "max_workers": 4,
                    },
                ),
                (
                    "trades",
                    {
                        "symbol": "BTCUSDT",
                        "exchange": "binance_futures",
                        "start_date": "2026-03-01",
                        "end_date": "2026-03-01",
                        "max_workers": 4,
                    },
                ),
            ],
        )

    @patch.object(cryptohftdata, "_CryptoHFTDataClient")
    @patch.dict("os.environ", {}, clear=True)
    def test_download_creates_keyless_client_for_free_tier(self, client_factory):
        client = client_factory.return_value
        client.get_orderbook.return_value = self._update_orderbook_df()
        client.get_trades.return_value = self._trade_df()

        cryptohftdata.download(
            symbol="BTCUSDT",
            exchange="binance_futures",
            start_date="2026-03-01",
            end_date="2026-03-01",
        )

        client_factory.assert_called_once_with(api_key=None)

    @patch.object(cryptohftdata, "_CryptoHFTDataClient")
    @patch.dict("os.environ", {"CRYPTOHFTDATA_API_KEY": "environment-key"}, clear=True)
    def test_download_uses_api_key_from_environment(self, client_factory):
        client = client_factory.return_value
        client.get_orderbook.return_value = self._update_orderbook_df()
        client.get_trades.return_value = self._trade_df()

        cryptohftdata.download(
            symbol="BTCUSDT",
            exchange="binance_futures",
            start_date="2026-03-01",
            end_date="2026-03-01",
        )

        client_factory.assert_called_once_with(api_key="environment-key")

    def test_download_and_convert_uses_injected_client(self):
        client = FakeCryptoHFTDataClient(self._update_orderbook_df(), self._trade_df())

        data = cryptohftdata.download_and_convert(
            symbol="BTCUSDT",
            exchange="binance_futures",
            start_date="2026-03-01",
            end_date="2026-03-01",
            client=client,
        )

        self.assertEqual(len(data), 3)
        self.assertEqual(_strip_order_flags(data[0]["ev"]), BUY_EVENT | TRADE_EVENT)

    def test_convert_accepts_pandas_dataframes(self):
        orderbook = self._update_orderbook_df().to_pandas()
        trades = self._trade_df().to_pandas()

        data = cryptohftdata.convert(orderbook, trades)

        self.assertEqual(len(data), 3)
        self.assertEqual(_strip_order_flags(data[0]["ev"]), BUY_EVENT | TRADE_EVENT)


if __name__ == "__main__":
    unittest.main()
