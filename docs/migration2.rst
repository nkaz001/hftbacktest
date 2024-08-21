===============
Migration to v2
===============

Overview
--------

The migration from version 1 to version 2 introduces several significant changes that can cause errors if the same code
is used without modification. It is highly recommended to review the updated tutorials. This guide aims to help you
avoid common pitfalls during the migration process.

Checking Success: Use ``elapse() == 0``
---------------------------------------
In version 1, ``elapse`` function returns ``True`` on success and ``False`` otherwise. Typically, the strategy loop
checks for successful elapsing using ``while elapse(duration)``. However, in version 2, elapse returns a code instead
of a boolean, with ``0`` indicating success and any other value indicating an error. Consequently, the code should be
updated to check if the return value equals ``0``.

For instance: ``while elapse(duration) == 0`` If the code remains unchanged, it will fail because a return value of
``0`` (indicating success) will be treated as ``False``. Other methods that involve elapsing, such as
``submit_buy_order`` or ``submit_sell_order``, also return a code similar to ``elapse`` instead of a boolean. Ensure to
check if their return values equal ``0`` to confirm success instead of checking for ``True``.

Data Format Changes
-------------------
The data format fed into HftBacktest has undergone significant changes. It is strongly recommended to reprocess the data
from raw data to preserve all information. However, if raw data is unavailable,
:mod:`the data conversion utility <hftbacktest.data.utils.migration2>` from v1 to v2 is provided.

The major changes are as follows:

* SOA to AOS: The format has shifted from a columnar array (SOA) to a structured array (AOS).

* Side Column Removal: ``side`` column has been removed. In version 2, the side is indicated by the ``ev`` field flags,
  :const:`BUY_EVENT <hftbacktest.types.BUY_EVENT>` and :const:`SELL_EVENT <hftbacktest.types.SELL_EVENT>`.

* Timestamp Handling: In version 1, the data utility corrects the event order by replacing one of the timestamps with
  ``-1`` to indicate an invalid event on either the exchange or the local side. In version 2, the validity of events on
  the exchange or local side is determined by `ev` field's :const:`EXCH_EVENT <hftbacktest.types.EXCH_EVENT>` and
  :const:`LOCAL_EVENT <hftbacktest.types.LOCAL_EVENT>` flags.

* Timestamp Unit: Although not strictly enforced, the timestamp unit has changed from microseconds to nanoseconds.

Additionally, the format for live order latency data has changed from SOA to AOS.