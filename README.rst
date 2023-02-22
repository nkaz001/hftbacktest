HftBacktest
===========

|codacy| |pypi| |license|

High-frequency trading and market-making backtesting tool that takes order queues and latencies into account
based on full order book and trade tick data.

Getting started
---------------

Installation
~~~~~~~~~~~~
`pip install hftbacktest`

Data Source & Format
~~~~~~~~~~~~~~~~~~~~
Please see https://github.com/nkaz001/collect-binancefutures regarding collecting and converting the feed data.

Examples
~~~~~~~~
Please see `example <https://github.com/nkaz001/hftbacktest/tree/master/example>`_ directory.

Documentation
-------------
* `Data <https://github.com/nkaz001/hftbacktest/wiki/Data>`_
* `Latency model <https://github.com/nkaz001/hftbacktest/wiki/Latency-model>`_
* `Order fill <https://github.com/nkaz001/hftbacktest/wiki/Order-fill>`_


.. |codacy| image:: https://app.codacy.com/project/badge/Grade/e2cef673757a45b18abfc361779feada
    :alt: |Codacy
    :target: https://www.codacy.com/gh/nkaz001/hftbacktest/dashboard?utm_source=github.com&amp;utm_medium=referral&amp;utm_content=nkaz001/hftbacktest&amp;utm_campaign=Badge_Grade

.. |pypi| image:: https://badge.fury.io/py/hftbacktest.svg
    :alt: |Python Version
    :target: https://pypi.org/project/hftbacktest

.. |license| image:: https://img.shields.io/badge/License-MIT-green.svg
    :alt: |License
    :target: https://github.com/nkaz001/hftbacktest/blob/master/LICENSE
