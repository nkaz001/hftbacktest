.. meta::
   :google-site-verification: IJcyhIoS28HF0lp6fGjBEOC65kVecelW6ZsFhbDaD-A

===========
HftBacktest
===========

|codeql| |python| |pypi| |downloads| |rustc| |crates| |license| |docs| |roadmap| |github|

**The master branch is switched to hftbacktest-2.0.0-alpha, which uses the Rust implementation as the backend. If you want to see the current version 1.8.4, please check out the corresponding tag.**

* `Browse v1.8.4 <https://github.com/nkaz001/hftbacktest/tree/20cd9470a431e90c526eca6975ef389073c9aca5>`_
* `Docs v1.8.4 <https://hftbacktest.readthedocs.io/en/v1.8.4/>`_

High-Frequency Trading Backtesting Tool
=======================================

This framework is designed for developing high-frequency trading and market-making strategies. It focuses on accounting for both feed and order latencies, as well as the order queue position for order fill simulation. The framework aims to provide more accurate market replay-based backtesting, based on full order book and trade tick feed data.

Key Features
============

The experimental features are currently in the early stages of development, having been completely rewritten in Rust to
support the following features.

* Working in `Numba <https://numba.pydata.org/>`_ JIT function (Python).
* Complete tick-by-tick simulation with a customizable time interval or based on the feed and order receipt.
* Full order book reconstruction based on L2 Market-By-Price and L3 Market-By-Order (Rust-only, WIP) feeds.
* Backtest accounting for both feed and order latency, using provided models or your own custom model.
* Order fill simulation that takes into account the order queue position, using provided models or your own custom model.
* Backtesting of multi-asset and multi-exchange models
* Deployment of a live trading bot using the same algorithm code: currently for Binance Futures and Bybit. (Rust-only)

Example: The complete process of backtesting Binance Futures
------------------------------------------------------------
`high-frequency gridtrading <https://github.com/nkaz001/hftbacktest/blob/master/hftbacktest/examples/gridtrading.ipynb>`_: The complete process of backtesting Binance Futures using a high-frequency grid trading strategy implemented in Rust.

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

A Quick Example
---------------

Get a glimpse of what backtesting with hftbacktest looks like with these code snippets:

.. code-block:: python

    @njit
    def simple_two_sided_quote(hbt):
        max_position = 5
        half_spread = hbt.tick_size * 20
        skew = 1
        order_qty = 0.1
        last_order_id = -1
        order_id = 0
        asset_no = 0

        # Checks every 0.1s
        while hbt.elapse(100_000_000) == 0:
            # Clears cancelled, filled or expired orders.
            hbt.clear_inactive_orders(asset_no)

            # Gets the market depth.
            depth = hbt.depth(asset_no)

            # Obtains the current mid-price and computes the reservation price.
            mid_price = (depth.best_bid + depth.best_ask) / 2.0
            reservation_price = mid_price - skew * hbt.position(asset_no) * depth.tick_size

            buy_order_price = reservation_price - half_spread
            sell_order_price = reservation_price + half_spread

            last_order_id = -1
            # Cancel all outstanding orders
            orders = hbt.orders(asset_no)
            values = orders.values()
            while True:
                order = values.next()
                if order is None:
                    break
                if order.cancellable:
                    hbt.cancel(asset_no, order.order_id)
                    last_order_id = order.order_id

            # All order requests are considered to be requested at the same time.
            # Waits until one of the order cancellation responses is received.
            if last_order_id >= 0:
                hbt.wait_order_response(asset_no, last_order_id)

            # Clears cancelled, filled or expired orders.
            hbt.clear_inactive_orders(asset_no)

	        last_order_id = -1
            if hbt.position < max_position:
                # Submits a new post-only limit bid order.
                order_id += 1
                hbt.submit_buy_order(
                    asset_no,
                    order_id,
                    buy_order_price,
                    order_qty,
                    GTX,
                    LIMIT,
                    False
                )
                last_order_id = order_id

            if hbt.position > -max_position:
                # Submits a new post-only limit ask order.
                order_id += 1
                hbt.submit_sell_order(
                    asset_no,
                    order_id,
                    sell_order_price,
                    order_qty,
                    GTX,
                    LIMIT,
                    False
                )
                last_order_id = order_id

            # All order requests are considered to be requested at the same time.
            # Waits until one of the order responses is received.
            if last_order_id >= 0:
                hbt.wait_order_response(asset_no, last_order_id)


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

Examples
========

You can find more examples in `examples <https://github.com/nkaz001/hftbacktest/tree/master/examples>`_ directory and `Rust examples <https://github.com/nkaz001/hftbacktest/tree/master/rust/examples>`_.

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

.. |rustc| image:: https://shields.io/badge/rustc-1.79-blue
    :alt: Rust Version
    :target: https://www.rust-lang.org/

.. toctree::
   :maxdepth: 1
   :caption: Tutorials
   :hidden:

   tutorials/Data Preparation
   tutorials/Getting Started
   tutorials/Working with Market Depth and Trades
   tutorials/Integrating Custom Data
   tutorials/Making Multiple Markets - Introduction
   tutorials/High-Frequency Grid Trading
   tutorials/Impact of Order Latency
   tutorials/Order Latency Data
   tutorials/GLFT Market Making Model and Grid Trading
   tutorials/Making Multiple Markets
   tutorials/Probability Queue Models
   tutorials/Risk Mitigation through Price Protection in Extreme Market Conditions
   tutorials/examples

.. toctree::
   :maxdepth: 2
   :caption: User Guide
   :hidden:

   Data <data>
   Latency Models <latency_models>
   Order Fill <order_fill>
   JIT Compilation Overhead <jit_compilation_overhead>
   Debugging Backtesting and Live Discrepancies <debugging_backtesting_and_live_discrepancies>

.. toctree::
   :maxdepth: 2
   :caption: API Reference
   :hidden:

   Initialization <reference/initialization>
   Backtester <reference/backtester>
   Constants <reference/constants>
   Statistics <reference/stats>
   Data Validation <reference/data_validation>
   Data Utilities <reference/data_utilities>
   Index <genindex>
