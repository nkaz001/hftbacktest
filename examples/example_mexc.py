import numpy as np

from numba import njit

from hftbacktest import BacktestAsset, HashMapMarketDepthBacktest, BUY, SELL, GTX, LIMIT
from hftbacktest.data.utils import mexc

@njit
def market_making_algo(hbt):
    while hbt.elapse(2.5e8) == 0:
        depth = hbt.depth(0)

        # Prints the best bid and the best offer.
        print(
            'current_timestamp:', hbt.current_timestamp,
            ', best_bid:', np.round(depth.best_bid, 1),
            ', best_ask:', np.round(depth.best_ask, 1)
        )
    return True

if __name__ == '__main__':
    data = mexc.convert(
        input_filename="examples/mexc/btcusdt_20250126.gz",
    )

    asset = (
        BacktestAsset()
            .data(data)
            .linear_asset(1.0)
            .power_prob_queue_model(2.0)
            .no_partial_fill_exchange()
            .trading_value_fee_model(-0.00005, 0.0007)
            .tick_size(0.1)
            .lot_size(0.001)
    )
    hbt = HashMapMarketDepthBacktest([asset])
    market_making_algo(hbt)
