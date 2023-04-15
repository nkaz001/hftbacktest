hftbacktest package
===================

Methods
-------

.. autofunction:: hftbacktest.HftBacktest

.. autofunction:: hftbacktest.reset

Backtester
----------

.. autoclass:: hftbacktest.backtest.SingleAssetHftBacktest
   :members:

Asset Types
-----------

.. autoclass:: hftbacktest.assettype.LinearAsset
    :members:

.. autoclass:: hftbacktest.assettype.InverseAsset
    :members:

Order Latency Models
--------------------

.. autoclass:: hftbacktest.models.latencies.ConstantLatency
    :members:

.. autoclass:: hftbacktest.models.latencies.FeedLatency
    :members:

.. autoclass:: hftbacktest.models.latencies.ForwardFeedLatency
    :members:

.. autoclass:: hftbacktest.models.latencies.BackwardFeedLatency
    :members:

.. autoclass:: hftbacktest.models.latencies.IntpOrderLatency
    :members:

Queue Models
------------

.. autoclass:: hftbacktest.models.queue.RiskAverseQueueModel
    :members:

.. autoclass:: hftbacktest.models.queue.ProbQueueModel
    :members:

.. autoclass:: hftbacktest.models.queue.IdentityProbQueueModel
    :members:
    :show-inheritance:

.. autoclass:: hftbacktest.models.queue.SquareProbQueueModel
    :members:
    :show-inheritance:

.. autoclass:: hftbacktest.models.queue.LogProbQueueModel
    :members:
    :show-inheritance:

Stat
----

.. automodule:: hftbacktest.stat
    :members:
    :show-inheritance:

Data Validation
---------------

.. automodule:: hftbacktest.data.validation
   :members:

Data Utilities
--------------

.. toctree::
   :maxdepth: 4

   hftbacktest.data.utils.binancefutures
   hftbacktest.data.utils.snapshot
   hftbacktest.data.utils.tardis
