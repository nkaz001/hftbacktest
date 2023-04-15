Latency Models
==============

Overview
--------

Latency is an important factor that you need to take into account when you backtest your HFT strategy.
HftBacktest has three types of latencies.

.. image:: images/latencies.png

* Feed latency

This is the latency between the time the exchange sends the feed events such as order book change or trade and the time
it is received by the local.
This latency is dealt with through two different timestamps: ``local_timestamp`` and ``exch_timestamp`` (exchange timestamp).

* Order entry latency

This is the latency between the time you send an order request and the time it is received by the exchange.

* Order response latency

This is the latency between the time the exchange processes an order request and the time the order response is received by the local.
The response to your order fill is also affected by this type of latency.

.. image:: images/latency-comparison.png

Order Latency Models
--------------------

HftBacktest provides the following order latency models and you can also implement your own latency model.

ConstantLatency
~~~~~~~~~~~~~~~
It's the most basic model that uses constant latencies. You just set the latencies.

..  code-block:: python

    from hftbacktest import ConstantLatency

    hbt = HftBacktest(
        data,
        tick_size=0.01,
        lot_size=0.001,
        maker_fee=-0.00005,
        taker_fee=0.0007,
        order_latency=ConstantLatency(entry_latency=50, response_latency=50),
        asset_type=Linear
    )


BackwardFeedLatency
~~~~~~~~~~~~~~~~~~~
This model uses the latest feed latency as order latencies.
The latencies are calculated according to the given arguments as follows.

.. code-block:: python

    feed_latency = local_timestamp - exch_timestamp
    entry_latency = entry_latency_mul * feed_latency + entry_latency
    resp_latency = resp_latency_mul * feed_latency + resp_latency

.. code-block:: python

    from hftbacktest import BackwardFeedLatency

    hbt = HftBacktest(
        data,
        tick_size=0.01,
        lot_size=0.001,
        maker_fee=-0.00005,
        taker_fee=0.0007,
        order_latency=BackwardFeedLatency(
            entry_latency_mul=1,
            resp_latency_mul=1,
            entry_latency=0,
            response_latency=0
        ),
        asset_type=Linear
    )


ForwardFeedLatency
~~~~~~~~~~~~~~~~~~
This model uses the next feed latency as order latencies using forward-looking information.

.. code-block:: python

    from hftbacktest import ForwardFeedLatency

    hbt = HftBacktest(
        data,
        tick_size=0.01,
        lot_size=0.001,
        maker_fee=-0.00005,
        taker_fee=0.0007,
        order_latency=ForwardFeedLatency(
            entry_latency_mul=1,
            resp_latency_mul=1,
            entry_latency=0,
            response_latency=0
        ),
        asset_type=Linear
    )


FeedLatency
~~~~~~~~~~~
This model uses the average of the latest and the next feed latency as order latencies.

.. code-block:: python

    from hftbacktest import FeedLatency

    hbt = HftBacktest(
        data,
        tick_size=0.01,
        lot_size=0.001,
        maker_fee=-0.00005,
        taker_fee=0.0007,
        order_latency=FeedLatency(
            entry_latency_mul=1,
            resp_latency_mul=1,
            entry_latency=0,
            response_latency=0
        ),
        asset_type=Linear
    )

IntpOrderLatency
~~~~~~~~~~~~~~~~
This model interpolates order latency based on the actual order latency data.
This is the most accurate among the provided models if you have the data with a fine time interval.
You can collect the latency data by submitting unexecutable orders regularly.

.. code-block:: python

    latency_data = np.load('order_latency')

    from hftbacktest import IntpOrderLatency

    hbt = HftBacktest(
        data,
        tick_size=0.01,
        lot_size=0.001,
        maker_fee=-0.00005,
        taker_fee=0.0007,
        order_latency=IntpOrderLatency(latency_data),
        asset_type=Linear
    )

**Data example**

.. code-block::

    request_timestamp_at_local, exch_timestamp, receive_response_timestamp_at_local
    1670026844751525, 1670026844759000, 1670026844762122
    1670026845754020, 1670026845762000, 1670026845770003


Implement your own order latency model
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
You need to implement ``numba`` ``jitclass`` that has two methods: ``entry`` and ``response``.

See `Latency model implementation <https://github.com/nkaz001/hftbacktest/blob/master/hftbacktest/models/latencies.py>`_

.. code-block:: python

    @jitclass
    class CustomLatencyModel:
        def __init__(self):
            pass

        def entry(self, timestamp, order, proc):
            # todo: return order entry latency.
            return 0

        def response(self, timestamp, order, proc):
            # todo: return order response latency.
            return 0

        def reset(self):
            pass

