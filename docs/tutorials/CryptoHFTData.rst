CryptoHFTData
=============

`CryptoHFTData <https://www.cryptohftdata.com/>`_ provides historical cryptocurrency order book and trade data as
hourly Parquet files. HftBacktest can download those files through the official Python SDK and convert them directly
to its normalized event format.

Install the CryptoHFTData integration
-------------------------------------

Install HftBacktest with the optional CryptoHFTData dependency:

.. code-block:: console

   pip install "hftbacktest[cryptohftdata]"

An API key is optional. Without one, the SDK uses the rate-limited free tier. To use an account key without putting it
in source code, set it in the environment:

.. code-block:: console

   export CRYPTOHFTDATA_API_KEY="your-api-key"

Download and convert
--------------------

The standalone example downloads an inclusive range of UTC dates and creates one compressed HftBacktest event file:

.. code-block:: console

   python examples/cryptohftdata.py \
       BTCUSDT binance_futures 2026-03-01 2026-03-02 \
       data/btcusdt_20260301_20260302.npz

The same workflow is available from Python:

.. code-block:: python

   from hftbacktest.data.utils import cryptohftdata

   data = cryptohftdata.download_and_convert(
       symbol="BTCUSDT",
       exchange="binance_futures",
       start_date="2026-03-01",
       end_date="2026-03-02",
       output_filename="data/btcusdt_20260301_20260302.npz",
   )

``download`` returns the SDK's order book and trade DataFrames when they need to be inspected or filtered before
conversion. ``convert`` accepts either pandas or Polars DataFrames:

.. code-block:: python

   orderbook, trades = cryptohftdata.download(
       symbol="BTCUSDT",
       exchange="binance_futures",
       start_date="2026-03-01",
       end_date="2026-03-01",
   )
   data = cryptohftdata.convert(orderbook, trades)

Event mapping
-------------

The converter preserves exchange and local timing in nanoseconds:

* order book ``local_ts`` comes from ``received_time``;
* order book ``exch_ts`` uses ``transaction_time`` when available and otherwise ``event_time``;
* trade ``local_ts`` comes from ``received_time``;
* trade ``exch_ts`` uses ``trade_time`` when available and otherwise ``event_time``;
* ``is_buyer_maker=True`` becomes a sell-initiated trade;
* incremental book rows become depth events; and
* snapshot rows become side-specific clear and snapshot events.

The normal HftBacktest latency and event-order corrections run after conversion.

Starting from a valid book
--------------------------

Some ranges begin with incremental updates rather than a snapshot. Start the download early enough to include a
snapshot or warm up the book using earlier data. To carry a reconstructed book into a later backtest range, create an
end-of-range snapshot:

.. code-block:: python

   from hftbacktest.data.utils.snapshot import create_last_snapshot

   snapshot = create_last_snapshot(
       ["data/btcusdt_20260301.npz"],
       tick_size=0.1,
       lot_size=0.001,
       output_snapshot_filename="data/btcusdt_20260301_eod.npz",
   )

Use that file with ``BacktestAsset.initial_snapshot`` for the following range. The
`CryptoHFTData workflow notebook <https://github.com/nkaz001/hftbacktest/blob/master/examples/CryptoHFTData%20Workflow.ipynb>`_
demonstrates the complete download, conversion, snapshot, replay, and statistics workflow.
