JIT Compilation Overhead
========================

HftBacktest takes advantage of Numba's capabilities, relying on Numba JIT'ed classes. As a result, importing
HftBacktest requires JIT compilation, which may take a few seconds. Additionally, the strategy function needs to be
JIT'ed' for performant backtesting, which also takes time to compile. Although this may not be significant when
backtesting for multiple days, it can still be bothersome. To minimize this overhead, you can consider using Numba's
``cache`` feature. See the example below.

.. code-block:: python

    from numba import njit
    # May take a few seconds
    from hftbacktest import BacktestAsset, HashMapMarketDepthBacktest

    # Enables caching feature
    @njit(cache=True)
    def algo(arguments, hbt):
        # your algo implementation.

    asset = (
        BacktestAsset()
            .linear_asset(1.0)
            .data([
                'data/ethusdt_20221003.npz',
                'data/ethusdt_20221004.npz',
                'data/ethusdt_20221005.npz',
                'data/ethusdt_20221006.npz',
                'data/ethusdt_20221007.npz'
            ])
            .initial_snapshot('data/ethusdt_20221002_eod.npz')
            .no_partial_fill_exchange()
            .intp_order_latency([
                'data/latency_20221003.npz',
                'data/latency_20221004.npz',
                'data/latency_20221005.npz',
                'data/latency_20221006.npz',
                'data/latency_20221007.npz'
            ])
            .power_prob_queue_model3(3.0)
            .tick_size(0.01)
            .lot_size(0.001)
            .trading_value_fee_model(0.0002, 0.0007)
    )

    hbt = HashMapMarketDepthBacktest([asset])
    algo(arguments, hbt)

