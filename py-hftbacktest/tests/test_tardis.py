import os
import tempfile
import unittest

from hftbacktest.data.utils import tardis
from hftbacktest.types import DEPTH_CLEAR_EVENT, DEPTH_SNAPSHOT_EVENT, BUY_EVENT, SELL_EVENT


def _row(ts, is_snapshot, side, px, qty):
    return f'binance-futures,BTCUSDT,{ts},{ts + 5},{is_snapshot},{side},{px},{qty}\n'


class TestTardisConvert(unittest.TestCase):
    def test_later_snapshot_larger_than_first(self):
        # The second snapshot has more levels than the first one. All of its levels must be kept, and the clear events
        # must cover its full price range.
        rows = [
            _row(1000, 'true', 'bid', 100.0, 1),
            _row(1000, 'true', 'bid', 99.0, 1),
            _row(1000, 'true', 'ask', 101.0, 1),
            _row(1000, 'true', 'ask', 102.0, 1),
            _row(2000, 'false', 'bid', 99.5, 3),
            _row(3000, 'true', 'bid', 100.5, 2),
            _row(3000, 'true', 'bid', 100.0, 2),
            _row(3000, 'true', 'bid', 99.0, 2),
            _row(3000, 'true', 'bid', 98.0, 2),
            _row(3000, 'true', 'ask', 101.0, 2),
            _row(3000, 'true', 'ask', 102.0, 2),
            _row(3000, 'true', 'ask', 103.0, 2),
            _row(3000, 'true', 'ask', 104.0, 2),
            _row(4000, 'false', 'ask', 101.5, 1),
        ]

        with tempfile.TemporaryDirectory() as tmp_dir:
            filename = os.path.join(tmp_dir, 'binance-futures_incremental_book_L2_2025-12-01_BTCUSDT.csv')
            with open(filename, 'w') as f:
                f.write(','.join(tardis.depth_schema.keys()) + '\n')
                f.writelines(rows)

            data = tardis.convert([filename], buffer_size=100, ss_buffer_size=10)

        second = data[data['exch_ts'] == 3_000_000]

        snapshot = second[(second['ev'] & DEPTH_SNAPSHOT_EVENT) == DEPTH_SNAPSHOT_EVENT]
        bid_snapshot = snapshot[(snapshot['ev'] & BUY_EVENT) == BUY_EVENT]
        ask_snapshot = snapshot[(snapshot['ev'] & SELL_EVENT) == SELL_EVENT]
        self.assertEqual(sorted(bid_snapshot['px'].tolist(), reverse=True), [100.5, 100.0, 99.0, 98.0])
        self.assertEqual(sorted(ask_snapshot['px'].tolist()), [101.0, 102.0, 103.0, 104.0])

        clear = second[(second['ev'] & DEPTH_CLEAR_EVENT) == DEPTH_CLEAR_EVENT]
        bid_clear = clear[(clear['ev'] & BUY_EVENT) == BUY_EVENT]
        ask_clear = clear[(clear['ev'] & SELL_EVENT) == SELL_EVENT]
        self.assertEqual(bid_clear['px'].tolist(), [98.0])
        self.assertEqual(ask_clear['px'].tolist(), [104.0])


if __name__ == '__main__':
    unittest.main()
