import unittest

import polars as pl

from hftbacktest.stats import Volatility


class TestVolatility(unittest.TestCase):
    def test_volatility_is_annualized_from_pnl_changes(self):
        df = pl.DataFrame(
            {
                "timestamp": pl.datetime_range(
                    start=pl.datetime(2024, 1, 1),
                    end=pl.datetime(2024, 1, 1, 0, 0, 3),
                    interval="1s",
                    eager=True,
                ),
                "equity_wo_fee": [100.0, 101.0, 99.0, 102.0],
                "fee": [0.0, 0.0, 0.0, 0.0],
            }
        )

        result = Volatility(trading_days_per_year=365).compute(df, {})

        pnl = [1.0, -2.0, 3.0]
        expected = float(pl.Series(pnl).std()) * (86_400 * 365) ** 0.5
        self.assertAlmostEqual(result["Volatility"], expected)
