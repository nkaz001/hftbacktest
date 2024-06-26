.. hftbacktest documentation master file, created by
   sphinx-quickstart on Fri Apr 14 20:09:35 2023.
   You can adapt this file completely to your liking, but it should at least
   contain the root `toctree` directive.

.. meta::
   :google-site-verification: IJcyhIoS28HF0lp6fGjBEOC65kVecelW6ZsFhbDaD-A

===========
HftBacktest
===========

|codacy| |codeql| |python| |pypi| |downloads| |crates| |license| |docs| |roadmap| |github|

High-Frequency Trading Backtesting Tool
=======================================

This framework is designed for developing high-frequency trading and market-making strategies. It focuses on accounting for both feed and order latencies, as well as the order queue position for order fill simulation. The framework aims to provide more accurate market replay-based backtesting, based on full order book and trade tick feed data.

Rust implementation with experimental features
==============================================

The experimental features are currently in the early stages of development, having been completely rewritten in Rust to
support the following features.

* Complete tick-by-tick simulation with a customizable time interval or based on the feed and order receipt.
* Full order book reconstruction based on L2 Market-By-Price and L3 Market-By-Order (WIP) feeds.
* Backtest accounting for both feed and order latency, using provided models or your own custom model.
* Order fill simulation that takes into account the order queue position, using provided models or your own custom model.
* Backtesting of multi-asset and multi-exchange models
* Deployment of a live trading bot using the same algorithm code: currently for Binance Futures and Bybit.

Please see `rust <https://github.com/nkaz001/hftbacktest/tree/master/rust>`_ directory.

Example: The complete process of backtesting Binance Futures
------------------------------------------------------------
`high-frequency gridtrading <https://github.com/nkaz001/hftbacktest/blob/master/rust/examples/gridtrading.ipynb>`_: The complete process of backtesting Binance Futures using a high-frequency grid trading strategy implemented in Rust.

Key Features
============

* Working in `Numba <https://numba.pydata.org/>`_ JIT function.
* Complete tick-by-tick simulation with a variable time interval.
* Full order book reconstruction based on L2 feeds(Market-By-Price).
* Backtest accounting for both feed and order latency, using provided models or your own custom model.
* Order fill simulation that takes into account the order queue position, using provided models or your own custom model.

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

Please see :doc:`Data <data>` or :doc:`Data Preparation <tutorials/Data Preparation>`.

A Quick Example
---------------

Get a glimpse of what backtesting with hftbacktest looks like with these code snippets:

.. code-block:: python

    @njit
    def simple_two_sided_quote(hbt, stat):
        max_position = 5
        half_spread = hbt.tick_size * 20
        skew = 1
        order_qty = 0.1
        last_order_id = -1
        order_id = 0

        # Checks every 0.1s
        while hbt.elapse(100_000):
            # Clears cancelled, filled or expired orders.
            hbt.clear_inactive_orders()

            # Obtains the current mid-price and computes the reservation price.
            mid_price = (hbt.best_bid + hbt.best_ask) / 2.0
            reservation_price = mid_price - skew * hbt.position * hbt.tick_size

            buy_order_price = reservation_price - half_spread
            sell_order_price = reservation_price + half_spread

            last_order_id = -1
            # Cancel all outstanding orders
            for order in hbt.orders.values():
                if order.cancellable:
                    hbt.cancel(order.order_id)
                    last_order_id = order.order_id

            # All order requests are considered to be requested at the same time.
            # Waits until one of the order cancellation responses is received.
            if last_order_id >= 0:
                hbt.wait_order_response(last_order_id)

            # Clears cancelled, filled or expired orders.
            hbt.clear_inactive_orders()

	        last_order_id = -1
            if hbt.position < max_position:
                # Submits a new post-only limit bid order.
                order_id += 1
                hbt.submit_buy_order(
                    order_id,
                    buy_order_price,
                    order_qty,
                    GTX
                )
                last_order_id = order_id

            if hbt.position > -max_position:
                # Submits a new post-only limit ask order.
                order_id += 1
                hbt.submit_sell_order(
                    order_id,
                    sell_order_price,
                    order_qty,
                    GTX
                )
                last_order_id = order_id

            # All order requests are considered to be requested at the same time.
            # Waits until one of the order responses is received.
            if last_order_id >= 0:
                hbt.wait_order_response(last_order_id)

            # Records the current state for stat calculation.
            stat.record(hbt)


Examples
========

You can find more examples in `examples <https://github.com/nkaz001/hftbacktest/tree/master/examples>`_ directory.

Contributing
============

Thank you for considering contributing to hftbacktest! Welcome any and all help to improve the project. If you have an
idea for an enhancement or a bug fix, please open an issue or discussion on GitHub to discuss it.

The following items are examples of contributions you can make to this project:

* Improve performance statistics reporting
* Implement test code
* Add additional queue or exchange models
* Update documentation and examples

.. |python| image:: https://shields.io/badge/python-3.10-blue
    :alt: Python Version
    :target: https://pypi.org/project/hftbacktest

.. |codacy| image:: https://app.codacy.com/project/badge/Grade/e2cef673757a45b18abfc361779feada
    :alt: Codacy
    :target: https://www.codacy.com/gh/nkaz001/hftbacktest/dashboard?utm_source=github.com&amp;utm_medium=referral&amp;utm_content=nkaz001/hftbacktest&amp;utm_campaign=Badge_Grade

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

.. |github| image:: https://img.shields.io/github/stars/nkaz001/hftbacktest?style=social
    :target: https://github.com/nkaz001/hftbacktest
    :alt: Github

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
   Asset Types <reference/asset_types>
   Order Latency Models <reference/order_latency_models>
   Queue Models <reference/queue_models>
   Stat <reference/stat>
   Data Validation <reference/data_validation>
   Data Utilities <reference/data_utilities>
   Index <genindex>
