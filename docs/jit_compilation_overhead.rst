JIT Compilation Overhead
========================

HftBacktest takes advantage of Numba's capabilities, with a significant portion of its implementation relying on Numba
JIT'ed classes. As a result, the first run of HftBacktest requires JIT compilation, which can take several tens of
seconds. Although this may not be significant when backtesting for multiple days, it can still be bothersome.

To minimize this overhead, you can consider using Numba's ``cache`` feature along with ``reset`` method to reset
HftBacktest. See the example below.

.. code-block:: python

    from numba import njit
    from hftbacktest import HftBacktest, IntpOrderLatency, SquareProbQueueModel, Linear

    # enables caching feature
    @njit(cache=True)
    def algo(arguments, hbt):
        # your algo implementation.

    hbt = HftBacktest(
        [
            'data/ethusdt_20221003.npz',
            'data/ethusdt_20221004.npz',
            'data/ethusdt_20221005.npz',
            'data/ethusdt_20221006.npz',
            'data/ethusdt_20221007.npz'
        ],
        tick_size=0.01,
        lot_size=0.001,
        maker_fee=-0.00005,
        taker_fee=0.0007,
        order_latency=IntpOrderLatency(),
        queue_model=SquareProbQueueModel(),
        asset_type=Linear,
        snapshot='data/ethusdt_20221002_eod.npz'
    )

    algo(arguments, hbt)

When you need to execute the same code using varying arguments or different datasets,
you can proceed as follows.

.. code-block:: python

    from hftbacktest import reset

    reset(
        hbt,
        [
            'data/ethusdt_20221003.npz',
            'data/ethusdt_20221004.npz',
            'data/ethusdt_20221005.npz',
            'data/ethusdt_20221006.npz',
            'data/ethusdt_20221007.npz'
        ],
        snapshot='data/ethusdt_20221002_eod.npz'
    )

    algo(arguments, hbt)
