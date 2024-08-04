Data
====

Please see https://github.com/nkaz001/hftbacktest/tree/master/collector or
:doc:`Data Preparation <tutorials/Data Preparation>` regarding collecting and converting the feed data.

Format
------

`hftbacktest` can digest a `numpy` structured array. The data has 8 fields in the following order.
You can also find details in `Event <https://docs.rs/hftbacktest/0.3.1/hftbacktest/types/struct.Event.html>`_.

* ev (u64): You can find the possible flag combinations in `Constants <https://docs.rs/hftbacktest/0.3.1/hftbacktest/types/index.html#constants>`_.
* exch_ts (i64): Exchange timestamp, which is the time at which the event occurs on the exchange.
* local_ts (i64): Local timestamp, which is the time at which the event is received by the local.
* px (f64): Price
* qty (f64): Quantity
* order_id (u64): Order ID is only for the L3 Market-By-Order feed.
* ival (i64): Reserved for an additional i64 value
* faval (f64): Reserved for an additional f64 value

**Raw data**

 .. code-block::

    1676419207212527000 {'stream': 'btcusdt@depth@0ms', 'data': {'e': 'depthUpdate', 'E': 1676419206974, 'T': 1676419205108, 's': 'BTCUSDT', 'U': 2505118837831, 'u': 2505118838224, 'pu': 2505118837821, 'b': [['2218.80', '0.603'], ['5000.00', '2.641'], ['22160.60', '0.008'], ['22172.30', '0.551'], ['22173.40', '0.073'], ['22174.50', '0.006'], ['22176.80', '0.157'], ['22177.90', '0.425'], ['22181.20', '0.260'], ['22182.30', '3.918'], ['22182.90', '0.000'], ['22183.40', '0.014'], ['22203.00', '0.000']], 'a': [['22171.70', '0.000'], ['22187.30', '0.000'], ['22194.30', '0.270'], ['22194.70', '0.423'], ['22195.20', '2.075'], ['22209.60', '4.506']]}}
    1676419207212584000 {'stream': 'btcusdt@trade', 'data': {'e': 'trade', 'E': 1676419206976, 'T': 1676419205116, 's': 'BTCUSDT', 't': 3288803053, 'p': '22177.90', 'q': '0.001', 'X': 'MARKET', 'm': True}}

**Normalized data**

.. list-table::
   :widths: 5 10 10 5 5 5 5 5
   :header-rows: 1

   * - ev
     - exch_ts
     - local_ts
     - px
     - qty
     - order_id
     - ival
     - fval
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 2218.8
     - 0.603
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 5000.00
     - 2.641
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22160.60
     - 0.008
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22172.30
     - 0.551
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22173.40
     - 0.073
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22174.50
     - 0.006
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22176.80
     - 0.157
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22177.90
     - 0.425
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22181.20
     - 0.260
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22182.30
     - 3.918
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22182.90
     - 0.000
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22183.40
     - 0.014
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22203.00
     - 0.000
     - 0
     - 0
     - 0.0
   * - SELL_EVENT | DEPTH_EVENT | EXCH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22171.70
     - 0.000
     - 0
     - 0
     - 0.0
   * - SELL_EVENT | DEPTH_EVENT | EXCH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22187.30
     - 0.000
     - 0
     - 0
     - 0.0
   * - SELL_EVENT | DEPTH_EVENT | EXCH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22194.30
     - 0.270
     - 0
     - 0
     - 0.0
   * - SELL_EVENT | DEPTH_EVENT | EXCH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22194.70
     - 0.423
     - 0
     - 0
     - 0.0
   * - SELL_EVENT | DEPTH_EVENT | EXCH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22195.20
     - 2.075
     - 0
     - 0
     - 0.0
   * - SELL_EVENT | DEPTH_EVENT | EXCH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22209.60
     - 4.506
     - 0
     - 0
     - 0.0
   * - SELL_EVENT | TRADE_EVENT | EXCH_EVENT | LOCAL_EVENT
     - 1676419205116000000
     - 1676419207212584000
     - 22177.90
     - 0.001
     - 0
     - 0
     - 0.0

Validation
----------

1. All timestamps must be in the correct order, chronological order.

There can be cases where an event happens before another at the exchange, resulting in an earlier exchange timestamp,
but it is received locally after the other event.
This reverses the chronological order of exchange and local timestamps. To handle this situation, hftbacktest uses the
:const:`EXCH_EVENT <hftbacktest.types.EXCH_EVENT>` and :const:`LOCAL_EVENT <hftbacktest.types.LOCAL_EVENT>` flags.
Events flagged with :const:`EXCH_EVENT <hftbacktest.types.EXCH_EVENT>` should be in chronological order according to the
exchange timestamp, while events flagged with :const:`LOCAL_EVENT <hftbacktest.types.LOCAL_EVENT>` should be in
chronological order according to the local timestamp.

2. The exchange timestamp must be earlier than the local timestamp; the feed latency must be positive.

Due to potential errors in time synchronization between two sites, the local timestamp may be earlier than the exchange
timestamp, resulting in negative latency. The best way to address this is to improve time synchronization using PTP
(Precision Time Protocol), which minimizes the possibility of negative latency.
However, by adding a base latency or offsetting the size of the negative latency, you can ensure that the data remains
valid with only positive latencies, where the local timestamp is always later than the exchange timestamp of the event.

See the following example. The exchange timestamp of the depth feed is advanced to the prior trade feed even though
the depth feed is received after the trade feed.

 .. code-block::

    1676419207212385000 {'stream': 'btcusdt@trade', 'data': {'e': 'trade', 'E': 1676419206968, 'T': 1676419205111, 's': 'BTCUSDT', 't': 3288803051, 'p': '22177.90', 'q': '0.300', 'X': 'MARKET', 'm': True}}
    1676419207212480000 {'stream': 'btcusdt@trade', 'data': {'e': 'trade', 'E': 1676419206968, 'T': 1676419205111, 's': 'BTCUSDT', 't': 3288803052, 'p': '22177.90', 'q': '0.119', 'X': 'MARKET', 'm': True}}
    1676419207212527000 {'stream': 'btcusdt@depth@0ms', 'data': {'e': 'depthUpdate', 'E': 1676419206974, 'T': 1676419205108, 's': 'BTCUSDT', 'U': 2505118837831, 'u': 2505118838224, 'pu': 2505118837821, 'b': [['2218.80', '0.603'], ['5000.00', '2.641'], ['22160.60', '0.008'], ['22172.30', '0.551'], ['22173.40', '0.073'], ['22174.50', '0.006'], ['22176.80', '0.157'], ['22177.90', '0.425'], ['22181.20', '0.260'], ['22182.30', '3.918'], ['22182.90', '0.000'], ['22183.40', '0.014'], ['22203.00', '0.000']], 'a': [['22171.70', '0.000'], ['22187.30', '0.000'], ['22194.30', '0.270'], ['22194.70', '0.423'], ['22195.20', '2.075'], ['22209.60', '4.506']]}}
    1676419207212584000 {'stream': 'btcusdt@trade', 'data': {'e': 'trade', 'E': 1676419206976, 'T': 1676419205116, 's': 'BTCUSDT', 't': 3288803053, 'p': '22177.90', 'q': '0.001', 'X': 'MARKET', 'm': True}}
    1676419207212621000 {'stream': 'btcusdt@trade', 'data': {'e': 'trade', 'E': 1676419206976, 'T': 1676419205116, 's': 'BTCUSDT', 't': 3288803054, 'p': '22177.90', 'q': '0.005', 'X': 'MARKET', 'm': True}}


This should be converted into the following form. HftBacktest provides :meth:`correct_event_order <hftbacktest.data.correct_event_order>`
method to automatically correct this issue. :meth:`validate_event_order <hftbacktest.data.validate_event_order>`
helps to check if this issue exists.

 .. code-block::

    EXCH_EVENT               1676419207212527000 {'stream': 'btcusdt@depth@0ms', 'data': {'e': 'depthUpdate', 'E': 1676419206974, 'T': 1676419205108, 's': 'BTCUSDT', 'U': 2505118837831, 'u': 2505118838224, 'pu': 2505118837821, 'b': [['2218.80', '0.603'], ['5000.00', '2.641'], ['22160.60', '0.008'], ['22172.30', '0.551'], ['22173.40', '0.073'], ['22174.50', '0.006'], ['22176.80', '0.157'], ['22177.90', '0.425'], ['22181.20', '0.260'], ['22182.30', '3.918'], ['22182.90', '0.000'], ['22183.40', '0.014'], ['22203.00', '0.000']], 'a': [['22171.70', '0.000'], ['22187.30', '0.000'], ['22194.30', '0.270'], ['22194.70', '0.423'], ['22195.20', '2.075'], ['22209.60', '4.506']]}}
    EXCH_EVENT | LOCAL_EVENT 1676419207212385000 {'stream': 'btcusdt@trade', 'data': {'e': 'trade', 'E': 1676419206968, 'T': 1676419205111, 's': 'BTCUSDT', 't': 3288803051, 'p': '22177.90', 'q': '0.300', 'X': 'MARKET', 'm': True}}
    EXCH_EVENT | LOCAL_EVENT 1676419207212480000 {'stream': 'btcusdt@trade', 'data': {'e': 'trade', 'E': 1676419206968, 'T': 1676419205111, 's': 'BTCUSDT', 't': 3288803052, 'p': '22177.90', 'q': '0.119', 'X': 'MARKET', 'm': True}}
                 LOCAL_EVENT 1676419207212527000 {'stream': 'btcusdt@depth@0ms', 'data': {'e': 'depthUpdate', 'E': 1676419206974, 'T': 1676419205108, 's': 'BTCUSDT', 'U': 2505118837831, 'u': 2505118838224, 'pu': 2505118837821, 'b': [['2218.80', '0.603'], ['5000.00', '2.641'], ['22160.60', '0.008'], ['22172.30', '0.551'], ['22173.40', '0.073'], ['22174.50', '0.006'], ['22176.80', '0.157'], ['22177.90', '0.425'], ['22181.20', '0.260'], ['22182.30', '3.918'], ['22182.90', '0.000'], ['22183.40', '0.014'], ['22203.00', '0.000']], 'a': [['22171.70', '0.000'], ['22187.30', '0.000'], ['22194.30', '0.270'], ['22194.70', '0.423'], ['22195.20', '2.075'], ['22209.60', '4.506']]}}
    EXCH_EVENT | LOCAL_EVENT 1676419207212584000 {'stream': 'btcusdt@trade', 'data': {'e': 'trade', 'E': 1676419206976, 'T': 1676419205116, 's': 'BTCUSDT', 't': 3288803053, 'p': '22177.90', 'q': '0.001', 'X': 'MARKET', 'm': True}}
    EXCH_EVENT | LOCAL_EVENT 1676419207212621000 {'stream': 'btcusdt@trade', 'data': {'e': 'trade', 'E': 1676419206976, 'T': 1676419205116, 's': 'BTCUSDT', 't': 3288803054, 'p': '22177.90', 'q': '0.005', 'X': 'MARKET', 'm': True}}

**Normalized data**

.. list-table::
   :widths: 5 10 10 5 5 5 5 5
   :header-rows: 1

   * - ev
     - exch_ts
     - local_ts
     - px
     - qty
     - order_id
     - ival
     - fval
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 2218.8
     - 0.603
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 5000.00
     - 2.641
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22160.60
     - 0.008
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22172.30
     - 0.551
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22173.40
     - 0.073
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22174.50
     - 0.006
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22176.80
     - 0.157
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22177.90
     - 0.425
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22181.20
     - 0.260
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22182.30
     - 3.918
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22182.90
     - 0.000
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22183.40
     - 0.014
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | EXCH_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22203.00
     - 0.000
     - 0
     - 0
     - 0.0
   * - ...
     -
     -
     -
     -
     -
     -
     -
   * - SELL_EVENT | TRADE_EVENT | EXCH_EVENT | LOCAL_EVENT
     - 1676419205111000000
     - 1676419207212385000
     - 22177.90
     - 0.300
     - 0
     - 0
     - 0.0
   * - SELL_EVENT | TRADE_EVENT | EXCH_EVENT | LOCAL_EVENT
     - 1676419205111000000
     - 1676419207212480000
     - 22177.90
     - 0.119
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 2218.8
     - 0.603
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 5000.00
     - 2.641
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22160.60
     - 0.008
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22172.30
     - 0.551
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22173.40
     - 0.073
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22174.50
     - 0.006
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22176.80
     - 0.157
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22177.90
     - 0.425
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22181.20
     - 0.260
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22182.30
     - 3.918
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22182.90
     - 0.000
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22183.40
     - 0.014
     - 0
     - 0
     - 0.0
   * - BUY_EVENT | DEPTH_EVENT | LOCAL_EVENT
     - 1676419205108000000
     - 1676419207212527000
     - 22203.00
     - 0.000
     - 0
     - 0
     - 0.0
   * - ...
     -
     -
     -
     -
     -
     -
     -
   * - SELL_EVENT | TRADE_EVENT | EXCH_EVENT | LOCAL_EVENT
     - 1676419206976000000
     - 1676419207212584000
     - 22177.90
     - 0.001
     - 0
     - 0
     - 0.0
   * - SELL_EVENT | TRADE_EVENT | EXCH_EVENT | LOCAL_EVENT
     - 1676419206976000000
     - 1676419207212621000
     - 22177.90
     - 0.005
     - 0
     - 0
     - 0.0
