===========
HftBacktest
===========

|codeql| |python| |pypi| |downloads| |rustc| |crates| |license| |docs| |roadmap| |github|

High-Frequency Trading Backtesting Tool
=======================================

This framework is designed for developing high-frequency trading and market-making strategies. It focuses on accounting for both feed and order latencies, as well as the order queue position for order fill simulation. The framework aims to provide more accurate market replay-based backtesting, based on full order book and trade tick feed data.

Key Features
============

The experimental features are currently in the early stages of development, having been completely rewritten in Rust to
support the following features.

* Working in `Numba <https://numba.pydata.org/>`_ JIT function (Python).
* Complete tick-by-tick simulation with a customizable time interval or based on the feed and order receipt.
* Full order book reconstruction based on L2 Market-By-Price and L3 Market-By-Order feeds.
* Backtest accounting for both feed and order latency, using provided models or your own custom model.
* Order fill simulation that takes into account the order queue position, using provided models or your own custom model.
* Backtesting of multi-asset and multi-exchange models
* Deployment of a live trading bot using the same algorithm code: currently for Binance Futures and Bybit. (Rust-only)

Documentation
=============

See `full document here <https://hftbacktest.readthedocs.io/>`_.

Getting started
===============

Installation
------------

hftbacktest supports Python 3.10+. You can install hftbacktest using ``pip``:

.. code-block:: console

 pip install hftbacktest

Or you can clone the latest development version from the Git repository with:

.. code-block:: console

 git clone https://github.com/nkaz001/hftbacktest

Data Source & Format
--------------------

Please see `Data <https://hftbacktest.readthedocs.io/en/latest/data.html>`_ or `Data Preparation <https://hftbacktest.readthedocs.io/en/latest/tutorials/Data%20Preparation.html>`_.

You can also find some data `here <https://reach.stratosphere.capital/data/usdm/>`_, hosted by the supporter.

A Quick Example
---------------

Get a glimpse of what backtesting with hftbacktest looks like with these code snippets:

.. code-block:: python

    @njit
    def market_making_algo(hbt):
        asset_no = 0
        tick_size = hbt.depth(asset_no).tick_size
        lot_size = hbt.depth(asset_no).lot_size

        # in nanoseconds
        while hbt.elapse(10_000_000) == 0:
            hbt.clear_inactive_orders(asset_no)

            a = 1
            b = 1
            c = 1
            hs = 1

            # Alpha, it can be a combination of several indicators.
            forecast = 0
            # In HFT, it can be various measurements of short-term market movements,
            # such as the high-low range in the last X minutes.
            volatility = 0
            # Delta risk, it can be a combination of several risks.
            position = hbt.position(asset_no)
            risk = (c + volatility) * position
            half_spread = (c + volatility) * hs

            max_notional_position = 1000
            notional_qty = 100

            depth = hbt.depth(asset_no)

            mid_price = (depth.best_bid + depth.best_ask) / 2.0

            # fair value pricing = mid_price + a * forecast
            #                      or underlying(correlated asset) + adjustment(basis + cost + etc) + a * forecast
            # risk skewing = -b * risk
            reservation_price = mid_price + a * forecast - b * risk
            new_bid = reservation_price - half_spread
            new_ask = reservation_price + half_spread

            new_bid_tick = min(np.round(new_bid / tick_size), depth.best_bid_tick)
            new_ask_tick = max(np.round(new_ask / tick_size), depth.best_ask_tick)

            order_qty = np.round(notional_qty / mid_price / lot_size) * lot_size

            # Elapses a process time.
            if not hbt.elapse(1_000_000) != 0:
                return False

            last_order_id = -1
            update_bid = True
            update_ask = True
            buy_limit_exceeded = position * mid_price > max_notional_position
            sell_limit_exceeded = position * mid_price < -max_notional_position
            orders = hbt.orders(asset_no)
            order_values = orders.values()
            while order_values.has_next():
                order = order_values.get()
                if order.side == BUY:
                    if order.price_tick == new_bid_tick or buy_limit_exceeded:
                        update_bid = False
                    if order.cancellable and (update_bid or buy_limit_exceeded):
                        hbt.cancel(asset_no, order.order_id, False)
                        last_order_id = order.order_id
                elif order.side == SELL:
                    if order.price_tick == new_ask_tick or sell_limit_exceeded:
                        update_ask = False
                    if order.cancellable and (update_ask or sell_limit_exceeded):
                        hbt.cancel(asset_no, order.order_id, False)
                        last_order_id = order.order_id

            # It can be combined with a grid trading strategy by submitting multiple orders to capture better spreads and
            # have queue position.
            # This approach requires more sophisticated logic to efficiently manage resting orders in the order book.
            if update_bid:
                # There is only one order at a given price, with new_bid_tick used as the order ID.
                order_id = new_bid_tick
                hbt.submit_buy_order(asset_no, order_id, new_bid_tick * tick_size, order_qty, GTX, LIMIT, False)
                last_order_id = order_id
            if update_ask:
                # There is only one order at a given price, with new_ask_tick used as the order ID.
                order_id = new_ask_tick
                hbt.submit_sell_order(asset_no, order_id, new_ask_tick * tick_size, order_qty, GTX, LIMIT, False)
                last_order_id = order_id

            # All order requests are considered to be requested at the same time.
            # Waits until one of the order responses is received.
            if last_order_id >= 0:
                # Waits for the order response for a maximum of 5 seconds.
                timeout = 5_000_000_000
                if not hbt.wait_order_response(asset_no, last_order_id, timeout):
                    return False

        return True


Tutorials
=========
* `Data Preparation <https://hftbacktest.readthedocs.io/en/latest/tutorials/Data%20Preparation.html>`_
* `Getting Started <https://hftbacktest.readthedocs.io/en/latest/tutorials/Getting%20Started.html>`_
* `Working with Market Depth and Trades <https://hftbacktest.readthedocs.io/en/latest/tutorials/Working%20with%20Market%20Depth%20and%20Trades.html>`_
* `Integrating Custom Data <https://hftbacktest.readthedocs.io/en/latest/tutorials/Integrating%20Custom%20Data.html>`_
* `Making Multiple Markets - Introduction <https://hftbacktest.readthedocs.io/en/latest/tutorials/Making%20Multiple%20Markets%20-%20Introduction.html>`_
* `High-Frequency Grid Trading <https://hftbacktest.readthedocs.io/en/latest/tutorials/High-Frequency%20Grid%20Trading.html>`_
* `Impact of Order Latency <https://hftbacktest.readthedocs.io/en/latest/tutorials/Impact%20of%20Order%20Latency.html>`_
* `Order Latency Data <https://hftbacktest.readthedocs.io/en/latest/tutorials/Order%20Latency%20Data.html>`_
* `Guéant–Lehalle–Fernandez-Tapia Market Making Model and Grid Trading <https://hftbacktest.readthedocs.io/en/latest/tutorials/GLFT%20Market%20Making%20Model%20and%20Grid%20Trading.html>`_
* `Making Multiple Markets <https://hftbacktest.readthedocs.io/en/latest/tutorials/Making%20Multiple%20Markets.html>`_
* `Risk Mitigation through Price Protection in Extreme Market Conditions <https://hftbacktest.readthedocs.io/en/latest/tutorials/Risk%20Mitigation%20through%20Price%20Protection%20in%20Extreme%20Market%20Conditions.html>`_
* `Level-3 Backtesting <https://hftbacktest.readthedocs.io/en/latest/tutorials/Level-3%20Backtesting.html>`_
* `Market Making with Alpha - Order Book Imbalance <https://hftbacktest.readthedocs.io/en/latest/tutorials/Market%20Making%20with%20Alpha%20-%20Order%20Book%20Imbalance.html>`_
* `Queue-Based Market Making in Large Tick Size Assets <https://hftbacktest.readthedocs.io/en/latest/tutorials/Queue-Based%20Market%20Making%20in%20Large%20Tick%20Size%20Assets.html>`_

Examples
========

You can find more examples in `examples <https://github.com/nkaz001/hftbacktest/tree/master/examples>`_ directory and `Rust examples <https://github.com/nkaz001/hftbacktest/blob/master/hftbacktest/examples/>`_.

The complete process of backtesting Binance Futures
---------------------------------------------------
`high-frequency gridtrading <https://github.com/nkaz001/hftbacktest/blob/master/hftbacktest/examples/gridtrading.ipynb>`_: The complete process of backtesting Binance Futures using a high-frequency grid trading strategy implemented in Rust.

Migration to V2
===============
Please see the `migration guide <https://hftbacktest.readthedocs.io/en/latest/migration2.html>`_.

Roadmap
=======

Currently, new features are being implemented in Rust due to the limitations of Numba, as performance is crucial given the size of the high-frequency data.
The imminent task is to integrate hftbacktest in Python with hftbacktest in Rust by using the Rust implementation as the backend.
Meanwhile, the data format, which is currently different, needs to be unified.
On the pure Python side, the performance reporting tool should be improved to provide more performance metrics with increased speed.

Please see the `roadmap <https://github.com/nkaz001/hftbacktest/blob/master/ROADMAP.md>`_.

Contributing
============

Thank you for considering contributing to hftbacktest! Welcome any and all help to improve the project. If you have an
idea for an enhancement or a bug fix, please open an issue or discussion on GitHub to discuss it.

The following items are examples of contributions you can make to this project:

Please see the `roadmap <https://github.com/nkaz001/hftbacktest/blob/master/ROADMAP.md>`_.

.. |python| image:: https://shields.io/badge/python-3.10-blue
    :alt: Python Version
    :target: https://www.python.org/

.. |codeql| image:: https://github.com/nkaz001/hftbacktest/actions/workflows/codeql.yml/badge.svg?branch=master&event=push
    :alt: CodeQL
    :target: https://github.com/nkaz001/hftbacktest/actions/workflows/codeql.yml

.. |pypi| image:: https://badge.fury.io/py/hftbacktest.svg
    :alt: Package Version
    :target: https://pypi.org/project/hftbacktest

.. |downloads| image:: https://static.pepy.tech/badge/hftbacktest
    :alt: Downloads
    :target: https://pepy.tech/project/hftbacktest

.. |crates| image:: https://img.shields.io/crates/v/hftbacktest.svg
    :alt: Rust crates.io version
    :target: https://crates.io/crates/hftbacktest

.. |license| image:: https://img.shields.io/badge/License-MIT-green.svg
    :alt: License
    :target: https://github.com/nkaz001/hftbacktest/blob/master/LICENSE

.. |docs| image:: https://readthedocs.org/projects/hftbacktest/badge/?version=latest
    :target: https://hftbacktest.readthedocs.io/en/latest/?badge=latest
    :alt: Documentation Status

.. |roadmap| image:: https://img.shields.io/badge/Roadmap-gray
    :target: https://github.com/nkaz001/hftbacktest/blob/master/ROADMAP.md
    :alt: Roadmap

.. |github| image:: https://img.shields.io/github/stars/nkaz001/hftbacktest?style=social
    :target: https://github.com/nkaz001/hftbacktest
    :alt: Github

.. |rustc| image:: https://shields.io/badge/rustc-1.81.0-blue
    :alt: Rust Version
    :target: https://www.rust-lang.org/
