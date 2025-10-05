import numpy as np

from numba import njit

from hftbacktest import BacktestAsset, HashMapMarketDepthBacktest
from hftbacktest.data.utils import bybit

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


if __name__ == "__main__":
    data = bybit.convert_fused(
        input_filename="examples/bybit/btcusdt_20250926.gz",
        tick_size=0.1,
        lot_size=0.001,
    )

    print(f"Loaded {len(data)} events")

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
