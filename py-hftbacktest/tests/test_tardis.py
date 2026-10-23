import os
import tempfile
import unittest

import numpy as np

from hftbacktest import (
    BUY_EVENT,
    DEPTH_CLEAR_EVENT,
    DEPTH_SNAPSHOT_EVENT,
    LOCAL_EVENT,
)
from hftbacktest.data.utils import tardis


DEPTH_HEAD = "exchange,symbol,timestamp,local_timestamp,is_snapshot,side,price,amount\n"
TRADE_HEAD = "exchange,symbol,timestamp,local_timestamp,id,side,price,amount\n"


class TestTardisConvert(unittest.TestCase):
    def test_later_larger_snapshot_keeps_all_rows(self):
        """A later snapshot batch larger than the first must not drop levels.

        Regression for https://github.com/nkaz001/hftbacktest/issues/339.
        """
        T = 1_700_000_000_000_000
        rows = []
        # 1st snapshot: 2 bids + 2 asks
        for px in ("100.0", "99.9"):
            rows.append((T + 100, T + 600, "true", "bid", px, "1"))
        for px in ("100.1", "100.2"):
            rows.append((T + 100, T + 600, "true", "ask", px, "1"))
        rows.append((T + 1000, T + 1500, "false", "ask", "100.1", "0.5"))
        # 2nd snapshot (e.g. after a reconnect): 5 bids + 4 asks
        for px in ("100.0", "99.9", "99.8", "99.7", "99.6"):
            rows.append((T + 7000, T + 7500, "true", "bid", px, "2"))
        for px in ("100.1", "100.2", "100.3", "100.4"):
            rows.append((T + 7000, T + 7500, "true", "ask", px, "2"))
        rows.append((T + 8000, T + 8500, "false", "ask", "100.1", "1.5"))

        with tempfile.TemporaryDirectory() as d:
            depth_path = os.path.join(d, "incremental_book_L2.csv")
            trade_path = os.path.join(d, "trades.csv")
            with open(depth_path, "w") as f:
                f.write(
                    DEPTH_HEAD
                    + "".join("ex,SYM,%d,%d,%s,%s,%s,%s\n" % r for r in rows)
                )
            with open(trade_path, "w") as f:
                f.write(
                    TRADE_HEAD
                    + "ex,SYM,%d,%d,1,buy,100.1,0.1\n" % (T + 900, T + 1400)
                )
            data = tardis.convert(
                [trade_path, depth_path], buffer_size=1000, ss_buffer_size=100
            )

        local = data[(data["ev"] & LOCAL_EVENT) != 0]
        kind = (local["ev"] & 0xFF).astype(int)
        is_bid = (local["ev"] & BUY_EVENT) != 0
        second = local["local_ts"] == (T + 7500) * 1000

        bid_rows = int((second & (kind == DEPTH_SNAPSHOT_EVENT) & is_bid).sum())
        ask_rows = int((second & (kind == DEPTH_SNAPSHOT_EVENT) & ~is_bid).sum())
        bid_clear = local["px"][second & (kind == DEPTH_CLEAR_EVENT) & is_bid]
        ask_clear = local["px"][second & (kind == DEPTH_CLEAR_EVENT) & ~is_bid]

        self.assertEqual(bid_rows, 5)
        self.assertEqual(ask_rows, 4)
        self.assertEqual(len(bid_clear), 1)
        self.assertEqual(len(ask_clear), 1)
        self.assertEqual(bid_clear[0], 99.6)
        self.assertEqual(ask_clear[0], 100.4)


if __name__ == "__main__":
    unittest.main()
