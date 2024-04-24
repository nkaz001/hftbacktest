===========
HftBacktest
===========

|codacy| |codeql| |pypi| |downloads| |license| |docs| |github|

High-Frequency Trading Backtesting Tool
=======================================

This framework is designed for developing high-frequency trading and market-making strategies. It focuses on accounting for both feed and order latencies, as well as the order queue position for order fill simulation. The framework aims to provide more accurate market replay-based backtesting, based on full order book and trade tick feed data.

Rust implementation with experimental features
==============================================

The experimental features are currently in the early stages of development, having been completely rewritten in Rust to
support the following features.

* Backtesting of multi-asset and multi-exchange models
* Deployment of a live trading bot using the same algo code.

Please see `rust <https://github.com/nkaz001/hftbacktest/tree/master/rust>`_ directory.

Key Features
============

* Working in `Numba <https://numba.pydata.org/>`_ JIT function.
* Complete tick-by-tick simulation with a variable time interval.
* Full order book reconstruction based on L2 feeds(Market-By-Price).
* Backtest accounting for both feed and order latency, using provided models or your own custom model.
* Order fill simulation that takes into account the order queue position, using provided models or your own custom model.

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

Tutorials
=========
* `Data Preparation <https://hftbacktest.readthedocs.io/en/latest/tutorials/Data%20Preparation.html>`_
* `Getting Started <https://hftbacktest.readthedocs.io/en/latest/tutorials/Getting%20Started.html>`_
* `Working with Market Depth and Trades <https://hftbacktest.readthedocs.io/en/latest/tutorials/Working%20with%20Market%20Depth%20and%20Trades.html>`_
* `Integrating Custom Data <https://hftbacktest.readthedocs.io/en/latest/tutorials/Integrating%20Custom%20Data.html>`_
* `High-Frequency Grid Trading <https://hftbacktest.readthedocs.io/en/latest/tutorials/High-Frequency%20Grid%20Trading.html>`_
* `Impact of Order Latency <https://hftbacktest.readthedocs.io/en/latest/tutorials/Impact%20of%20Order%20Latency.html>`_
* `Guéant–Lehalle–Fernandez-Tapia Market Making Model and Grid Trading <https://hftbacktest.readthedocs.io/en/latest/tutorials/GLFT%20Market%20Making%20Model%20and%20Grid%20Trading.html>`_
* `Making Multiple Markets <https://hftbacktest.readthedocs.io/en/latest/tutorials/Making%20Multiple%20Markets.html>`_
* `Risk Mitigation through Price Protection in Extreme Market Conditions <https://hftbacktest.readthedocs.io/en/latest/tutorials/Risk%20Mitigation%20through%20Price%20Protection%20in%20Extreme%20Market%20Conditions.html>`_

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
* Implement a live bot connector

.. |python| image:: https://img.shields.io/pypi/pyversions/hftbacktest.svg?style=plastic
    :alt: Python Version
    :target: https://badge.fury.io/py/hftbacktest

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

.. |license| image:: https://img.shields.io/badge/License-MIT-green.svg
    :alt: License
    :target: https://github.com/nkaz001/hftbacktest/blob/master/LICENSE

.. |docs| image:: https://readthedocs.org/projects/hftbacktest/badge/?version=latest
    :target: https://hftbacktest.readthedocs.io/en/latest/?badge=latest
    :alt: Documentation Status

.. |github| image:: https://img.shields.io/github/stars/nkaz001/hftbacktest?style=social
    :target: https://github.com/nkaz001/hftbacktest
    :alt: Github